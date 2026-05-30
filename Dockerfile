# ── Build stage ──────────────────────────────────────────────────────────────
FROM rust:1.96-slim AS builder

WORKDIR /build

# Cache dependencies before copying source.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src && echo 'fn main(){}' > src/main.rs \
    && cargo fetch --locked

COPY src/ src/
RUN cargo build --release --locked

# ── Runtime stage ─────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd --system ladon && useradd --system --gid ladon ladon

WORKDIR /app
RUN mkdir -p data && chown ladon:ladon data

COPY --from=builder /build/target/release/ladon /usr/local/bin/ladon

USER ladon

# Run the pool daemon by default using Config.toml mounted into WORKDIR.
# Override CMD to use derive/decrypt or pass --config before the sub-command.
ENTRYPOINT ["ladon"]
CMD ["pool"]
