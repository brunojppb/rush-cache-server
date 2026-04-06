## Stage 1: Get CA certificates
FROM alpine:latest AS ca-certificates
RUN apk add --no-cache ca-certificates

## Stage 2: Install build tooling
FROM rust:1.94-alpine AS chef
RUN apk add --no-cache musl-dev openssl-dev zig perl make && \
  cargo install --locked cargo-zigbuild cargo-chef && \
  rustup target add x86_64-unknown-linux-musl aarch64-unknown-linux-musl

## Stage 3: Plan dependencies
FROM chef AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

## Stage 4: Build for Linux AMD64 and ARM64
FROM chef AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json

# Cache dependencies
RUN cargo chef cook --release --recipe-path recipe.json --target x86_64-unknown-linux-gnu && \
    cargo chef cook --release --recipe-path recipe.json --target aarch64-unknown-linux-gnu

COPY . .

# Build for AMD64
RUN cargo zigbuild --release --target x86_64-unknown-linux-gnu && \
    cp target/x86_64-unknown-linux-gnu/release/rush-cache-server /rush-cache-server-x64

# Build for ARM64
RUN cargo zigbuild --release --target aarch64-unknown-linux-gnu && \
    cp target/aarch64-unknown-linux-gnu/release/rush-cache-server /rush-cache-server-arm64

## Stage 5: Final minimal image
FROM scratch AS final
COPY --from=ca-certificates /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
ARG TARGETARCH
COPY --from=builder /rush-cache-server-${TARGETARCH:-x64} /rush-cache-server
EXPOSE 8080
ENTRYPOINT ["/rush-cache-server"]
