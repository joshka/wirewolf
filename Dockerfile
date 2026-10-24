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
FROM --platform=$BUILDPLATFORM node:26-bookworm-slim@sha256:28147155d47dc31abe0a46e0d12b59b2fb3d0a2dcc5dc65fce76a074180b16fe AS ui
WORKDIR /ui
RUN npm install -g pnpm@11.11.0
COPY crates/wirewolf-web/ui-shadcn/package.json crates/wirewolf-web/ui-shadcn/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY crates/wirewolf-web/ui-shadcn/ ./
RUN pnpm build

# The Rust stage runs natively on the build machine and cross-compiles for the target, so an
# amd64 image builds quickly (and reliably) on an arm64 Mac without emulating rustc.
FROM --platform=$BUILDPLATFORM rust:1-bookworm@sha256:48b7f435b8fc6378ce43b44f15e3a47fd8919b2825f835c39bd4bd3da50af5b8 AS build
ARG TARGETARCH
RUN case "$TARGETARCH" in \
      amd64) echo x86_64-unknown-linux-gnu > /rust-target; gcc=gcc-x86-64-linux-gnu ;; \
      arm64) echo aarch64-unknown-linux-gnu > /rust-target; gcc=gcc-aarch64-linux-gnu ;; \
      *) echo "unsupported TARGETARCH $TARGETARCH" >&2; exit 1 ;; \
    esac \
    && apt-get update && apt-get install -y --no-install-recommends "$gcc" libc6-dev-"$TARGETARCH"-cross \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add "$(cat /rust-target)"
ENV CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER=x86_64-linux-gnu-gcc \
    CC_x86_64_unknown_linux_gnu=x86_64-linux-gnu-gcc \
    CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc \
    CC_aarch64_unknown_linux_gnu=aarch64-linux-gnu-gcc
WORKDIR /src
COPY . .
COPY --from=ui /ui/dist crates/wirewolf-web/ui-shadcn/dist
ENV WIREWOLF_UI_SHADCN_PREBUILT=1
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/src/target,id=wirewolf-target-$TARGETARCH \
    target="$(cat /rust-target)" \
    && cargo build --release --locked --bin wirewolf --target "$target" \
    && cp "target/$target/release/wirewolf" /wirewolf

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
