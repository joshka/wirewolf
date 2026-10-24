# syntax=docker/dockerfile:1
# Multi-stage build for ghcr.io/joshka/wirewolf.
#
#   docker build -t ghcr.io/joshka/wirewolf .
#   docker run --rm --network host --cap-drop ALL --cap-add NET_RAW --read-only \
#     --security-opt no-new-privileges -v wirewolf:/data ghcr.io/joshka/wirewolf
#
# docs/deploy.md explains these options and the security settings.
#
# `--network host` lets the probes see your real LAN and router. In bridge mode the gateway hop
# measures the Docker bridge instead (the UI says so).

# Base images are pinned by digest so a build is reproducible and a tag can't be swapped under
# us; Dependabot (.github/dependabot.yml) proposes digest updates.

# The dashboard is built with Node here; the final image has no Node.
FROM node:22-bookworm-slim@sha256:de170b39439eb0e5a3444ad7d60b015614a381e10fecae2e30ec9e735122f38d AS ui
WORKDIR /ui
RUN corepack enable
COPY crates/wirewolf-web/ui-shadcn/package.json crates/wirewolf-web/ui-shadcn/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY crates/wirewolf-web/ui-shadcn/ ./
RUN pnpm build

FROM rust:1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS build
WORKDIR /src
COPY . .
COPY --from=ui /ui/dist crates/wirewolf-web/ui-shadcn/dist
ENV WIREWOLF_UI_SHADCN_PREBUILT=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin wirewolf \
    && cp target/release/wirewolf /wirewolf

# distroless/cc has glibc, libgcc, and CA certificates, and nothing else.
FROM gcr.io/distroless/cc-debian12@sha256:e5d81ddde149641e2a9ba55be4545bc125c67de07508b03ba4c22e6eb0ded5aa
COPY --from=build /wirewolf /usr/local/bin/wirewolf
ENV WIREWOLF_DB=/data/wirewolf.db \
    WIREWOLF_CONFIG=/data/wirewolf.toml
VOLUME /data
# Relative paths in the config (such as auth.session_key_file) resolve here, inside the volume,
# so the session key survives a container being recreated.
WORKDIR /data
EXPOSE 7465
# Root inside the container so ICMP can use a raw socket (Docker grants CAP_NET_RAW by default),
# which enables the ISP TTL probes. As a non-root user the gateway still uses unprivileged ICMP.
ENTRYPOINT ["/usr/local/bin/wirewolf"]
CMD ["serve"]
