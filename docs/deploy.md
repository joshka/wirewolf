# Deploying wirewolf

## Docker

```sh
docker run -d --name wirewolf --restart unless-stopped --network host \
  --cap-drop ALL --cap-add NET_RAW --read-only --security-opt no-new-privileges \
  -v wirewolf:/data ghcr.io/joshka/wirewolf
```

`compose.yaml` sets the same options. The process runs as root inside the container only so ICMP
can use a raw socket for the ISP TTL probes; every capability except `NET_RAW` is dropped, the
root filesystem is read-only, and only `/data` (the database, `wirewolf.toml`, and the session
key) is writable. As a non-root user the probes still work, but the ISP hop is inferred instead
of measured.

`--network host` lets the probes see the real LAN and router, and it also means wirewolf listens
on every interface the host has (`server.listen = "0.0.0.0:7465"`). On a shared or guest
network, set `auth.require_for_reads` or listen on one address.

## Configuring with environment variables

Every key in `wirewolf.toml` can also be set as `WIREWOLF_<SECTION>_<KEY>`, so a deployment can
live entirely in `compose.yaml` and the image stays generic:

| Setting | Variable |
|---|---|
| `auth.issuer` | `WIREWOLF_AUTH_ISSUER` |
| `auth.trusted_header.user` | `WIREWOLF_AUTH_TRUSTED_HEADER_USER` |
| `server.allowed_hosts` | `WIREWOLF_SERVER_ALLOWED_HOSTS` |
| `probe.interval` | `WIREWOLF_PROBE_INTERVAL` |

- **Precedence:** defaults, then `wirewolf.toml`, then `WIREWOLF_<SECTION>_<KEY>`, then the older
  short names (`WIREWOLF_LISTEN`, `WIREWOLF_INTERVAL`, `WIREWOLF_DB`, …) and command line flags.
- **Values** parse like the file: durations such as `30s`, booleans `true`/`false` (or `1`/`0`,
  `yes`/`no`, `on`/`off`). Lists are comma-separated (`a,b`) or a TOML array (`["a", "b"]`);
  lists of tables (`targets`, `dns.resolvers`, …) take only the TOML form. A bad value stops
  startup with an error naming the variable.
- **Locked in Settings:** values from the environment show as `env` on the Settings page and
  can't be changed there.

OIDC with [Pocket ID](https://pocket-id.org) behind a reverse proxy, with no config file at all:

```yaml
services:
  wirewolf:
    image: ghcr.io/joshka/wirewolf:latest
    restart: unless-stopped
    network_mode: host
    cap_drop: [ALL]
    cap_add: [NET_RAW]
    read_only: true
    security_opt: [no-new-privileges:true]
    volumes:
      - wirewolf:/data
    secrets: [wirewolf_oidc]
    environment:
      WIREWOLF_HOSTNAME: wirewolf
      WIREWOLF_SERVER_ALLOWED_HOSTS: wirewolf.example.com
      WIREWOLF_AUTH_ISSUER: https://id.example.com
      WIREWOLF_AUTH_CLIENT_ID: wirewolf
      WIREWOLF_AUTH_CLIENT_SECRET_FILE: /run/secrets/wirewolf_oidc
      WIREWOLF_AUTH_REDIRECT_URL: https://wirewolf.example.com/auth/callback
      WIREWOLF_AUTH_SCOPES: email,profile,groups
      WIREWOLF_AUTH_ALLOWED_GROUPS: wirewolf-admins
      WIREWOLF_AUTH_SESSION_KEY_FILE: /data/wirewolf-session.key
secrets:
  wirewolf_oidc:
    file: ./wirewolf_oidc_secret.txt   # the client secret from the issuer, mode 600
volumes:
  wirewolf:
```

Keep the client secret out of `environment:` where you can: `WIREWOLF_AUTH_CLIENT_SECRET` works,
but `docker inspect` shows it, while a secret file doesn't.

## Who can see and change what

- **Reads** are open to anyone who can reach the port unless `auth.require_for_reads`. Readers who
  can't change settings don't see `[auth]`, the config file path, or the unredacted AI brief.
- **Writes** are off until a sign-in method is configured, or `auth.allow_unauthenticated_writes`.
- **Host names.** wirewolf answers to IP addresses, `localhost`, single-label names, and local
  names (`.local`, `.lan`, `.home.arpa`, `.internal`, `.ts.net`). Behind a reverse proxy with a
  public name, add it to `server.allowed_hosts` (the OIDC `redirect_url` host is added for you).
  Other names get `421`; that stops DNS rebinding, where a web page points its own name at your
  server to read the API.
- **Targets** may point anywhere on the LAN, but never at cloud metadata addresses
  (`169.254.0.0/16`, `fd00:ec2::254`).

## OIDC

List who may sign in with `auth.allowed_emails` or `auth.allowed_groups`. With both empty,
wirewolf refuses to start, because with a public issuer (Google, Microsoft) anyone with an account
could change settings. Set `auth.allow_any_user = true` if your issuer only has your own users.

Emails count only when the issuer marks them verified (`email_verified: true`); some issuers
leave the claim out and let users set any address.

Sessions live in an encrypted cookie and can't be revoked one at a time: signing out deletes the
cookie from that browser, but a copied cookie works until `auth.session_ttl` runs out. Removing
someone from `allowed_emails` or `allowed_groups` ends their session once wirewolf restarts. To
sign everyone out, stop wirewolf, delete `auth.session_key_file`, and start it again.

## Trusted headers from a reverse proxy

`[auth.trusted_header]` believes `user`, `email`, `name`, and `groups` headers from the addresses
in `proxies` only. Two things to get right:

- **The proxy must set or strip every configured header**, not just the user. If it sets
  `Remote-User` but passes a client's own `Remote-Groups` through, anyone it signs in can claim a
  group in `auth.allowed_groups`. Configure the proxy to remove these headers from incoming
  requests.
- **Keep `proxies` narrow.** With `--network host`, `127.0.0.1` trusts every process on the
  host, and a Docker bridge range trusts every container on it. List the proxy's own address.
