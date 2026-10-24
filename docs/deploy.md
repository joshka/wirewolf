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
