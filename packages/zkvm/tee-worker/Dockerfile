# Build context must be packages/zkvm (the Rust workspace root) so the worker's path
# dependencies (revm-executor, robinhood-bridge) are available.
FROM rust:1-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY revm-executor ./revm-executor
COPY robinhood-bridge ./robinhood-bridge
COPY tee-worker ./tee-worker
RUN cargo build --release -p zkhood-tee-worker

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /build/target/release/zkhood-tee-worker /usr/local/bin/zkhood-tee-worker
ENV PORT=4000
EXPOSE 4000
ENTRYPOINT ["/usr/local/bin/zkhood-tee-worker"]
