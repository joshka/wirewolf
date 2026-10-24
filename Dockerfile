# syntax=docker/dockerfile:1
# Multi-stage build for ghcr.io/joshka/wirewolf.
#
#   docker build -t ghcr.io/joshka/wirewolf .
#   docker run --rm --network host -v wirewolf:/data ghcr.io/joshka/wirewolf
#
# `--network host` lets the probes see your real LAN and router. In bridge mode the gateway hop
# measures the Docker bridge instead (the UI says so).

# The dashboard is built with Node here; the final image has no Node.
FROM node:22-bookworm-slim AS ui
WORKDIR /ui
RUN corepack enable
COPY crates/wirewolf-web/ui-shadcn/package.json crates/wirewolf-web/ui-shadcn/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY crates/wirewolf-web/ui-shadcn/ ./
RUN pnpm build

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
COPY --from=ui /ui/dist crates/wirewolf-web/ui-shadcn/dist
ENV WIREWOLF_UI_SHADCN_PREBUILT=1
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked --bin wirewolf \
    && cp target/release/wirewolf /wirewolf

# distroless/cc has glibc, libgcc, and CA certificates, and nothing else.
FROM gcr.io/distroless/cc-debian12
COPY --from=build /wirewolf /usr/local/bin/wirewolf
ENV WIREWOLF_DB=/data/wirewolf.db \
    WIREWOLF_CONFIG=/data/wirewolf.toml
VOLUME /data
EXPOSE 7465
# Root inside the container so ICMP can use a raw socket (Docker grants CAP_NET_RAW by default),
# which enables the ISP TTL probes. As a non-root user the gateway still uses unprivileged ICMP.
ENTRYPOINT ["/usr/local/bin/wirewolf"]
CMD ["serve"]
