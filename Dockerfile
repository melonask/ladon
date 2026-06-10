# ── Build stage ──────────────────────────────────────────────────────────────
ARG RUST_VERSION=1.96
FROM rust:${RUST_VERSION}-slim-bookworm AS builder

WORKDIR /build

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        pkg-config \
        libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Cache dependencies before copying source.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs && touch src/lib.rs \
    && cargo build --release --locked --bin ladon \
    && rm -rf src

COPY src/ src/
RUN cargo build --release --locked --bin ladon

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        libsqlite3-0 \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system ladon \
    && useradd \
        --system \
        --gid ladon \
        --home-dir /app \
        --shell /usr/sbin/nologin \
        ladon

WORKDIR /app

RUN mkdir -p /app/data /etc/ladon \
    && chown -R ladon:ladon /app /etc/ladon

COPY --from=builder /build/target/release/ladon /usr/local/bin/ladon

USER ladon

ENV LADON_CONFIG=/etc/ladon/Config.toml

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["ladon", "ping"]

ENTRYPOINT ["ladon"]
CMD ["--config", "/etc/ladon/Config.toml"]
