# ── Build stage ──────────────────────────────────────────────────────────────
ARG RUST_VERSION=1.97
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
    && cargo build --release --locked --features full --bin ladon \
    && rm -rf src

COPY src/ src/
# The dependency-cache build uses a dummy binary. Refresh every Rust source
# input after copying the real crate so Cargo always rebuilds the application.
RUN find src -type f -name '*.rs' -exec touch {} + \
    && cargo build --release --locked --features full --bin ladon

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

LABEL org.opencontainers.image.title="Ladon" \
      org.opencontainers.image.description="Fast multi-chain HD wallet CLI and address-pool daemon" \
      org.opencontainers.image.licenses="MIT" \
      org.opencontainers.image.source="https://github.com/melonask/ladon"

USER ladon

ENV LADON_CONFIG=/etc/ladon/Config.toml

HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD ["ladon", "check"]

ENTRYPOINT ["ladon"]
CMD ["pool"]
