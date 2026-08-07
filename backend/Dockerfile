# syntax=docker.1ms.run/docker/dockerfile:1.7

ARG RUST_IMAGE=docker.1ms.run/rust:1.97-slim-bookworm@sha256:99e09cb2284e2ddbb73a995deee3e91783fd04d177602ccf6eab326d778ee777
ARG DEBIAN_IMAGE=docker.1ms.run/debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

FROM ${RUST_IMAGE} AS builder
WORKDIR /app

RUN set -eux; \
    if [ -f /etc/apt/sources.list ]; then \
        sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list; \
        sed -i 's/security.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list; \
    fi; \
    if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
        sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources; \
        sed -i 's/security.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources; \
    fi; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
        build-essential \
        ca-certificates \
        cmake \
        libssl-dev \
        pkg-config; \
    rm -rf /var/lib/apt/lists/*

ENV RUSTUP_DIST_SERVER=https://mirrors.aliyun.com/rustup \
    RUSTUP_UPDATE_ROOT=https://mirrors.aliyun.com/rustup/rustup \
    CARGO_NET_RETRY=10 \
    CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

RUN mkdir -p "$CARGO_HOME" \
    && cat <<'EOF' > "$CARGO_HOME/config.toml"
[source.crates-io]
replace-with = "rsproxy-sparse"

[source.rsproxy-sparse]
registry = "sparse+https://rsproxy.cn/index/"

[net]
git-fetch-with-cli = true
EOF

COPY Cargo.toml Cargo.lock ./
COPY apps ./apps
COPY config ./config
COPY crates ./crates
COPY database ./database
COPY entities ./entities
COPY services ./services

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --locked --release \
        -p web-api --bin web-api --bin init_super_admin \
    && install -D /app/target/release/web-api /tmp/bin/web-api \
    && install -D /app/target/release/init_super_admin /tmp/bin/init_super_admin

FROM ${DEBIAN_IMAGE} AS runtime
WORKDIR /app

RUN set -eux; \
    if [ -f /etc/apt/sources.list ]; then \
        sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list; \
        sed -i 's/security.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list; \
    fi; \
    if [ -f /etc/apt/sources.list.d/debian.sources ]; then \
        sed -i 's/deb.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources; \
        sed -i 's/security.debian.org/mirrors.aliyun.com/g' /etc/apt/sources.list.d/debian.sources; \
    fi; \
    apt-get update; \
    apt-get install -y --no-install-recommends ca-certificates curl libssl3 tzdata; \
    rm -rf /var/lib/apt/lists/*; \
    groupadd --system --gid 10001 rs-project-template; \
    useradd --system --uid 10001 --gid rs-project-template \
        --home-dir /app --shell /usr/sbin/nologin rs-project-template; \
    mkdir -p /app/logs; \
    chown -R rs-project-template:rs-project-template /app

COPY --from=builder /tmp/bin/web-api /usr/local/bin/web-api
COPY --from=builder /tmp/bin/init_super_admin /usr/local/bin/init_super_admin

USER rs-project-template

ENV RUST_LOG=info \
    LOG_FORMAT=json \
    TZ=Asia/Shanghai

EXPOSE 10001

HEALTHCHECK --interval=10s --timeout=5s --start-period=20s --retries=5 \
    CMD curl --fail --silent --show-error --max-time 3 \
        http://127.0.0.1:10001/health || exit 1

ENTRYPOINT ["web-api"]
CMD ["--config-path", "/app/config.toml"]
