# Rush Cache Server

A high-performance HTTP build cache server for [Rush](https://rushjs.io/) monorepos. Acts as an authenticated proxy between Rush's [`@rushstack/rush-http-build-cache-plugin`](https://github.com/nicolo-ribaudo/rushstack/tree/main/rush-plugins/rush-http-build-cache-plugin) and a private S3 bucket, so your bucket never needs to be publicly accessible.

Written in Rust. Single binary. Constant memory usage regardless of artifact size.

```
  Rush clients (CI / devs)
         |
    Bearer token auth
         |
  +------v------+
  |  rush-cache  |   Actix-web + Tokio
  |    server    |   Streaming proxy
  +------+------+
         |
    AWS credential chain
    (env / instance role)
         |
  +------v------+
  |   S3 Bucket  |   Private, no public access
  +-------------+
```

## Features

- **Two-tier token auth** -- read-only tokens for devs and PR builds, read-write tokens for main branch CI
- **Streaming** -- artifacts are proxied between client and S3 with no in-memory buffering
- **S3-compatible** -- works with Amazon S3, Cloudflare R2, MinIO, RustFS, or any S3-compatible store
- **OpenTelemetry** -- distributed tracing and system metrics via OTLP/gRPC, compatible with Datadog, Honeycomb, Jaeger, and more
- **Multi-platform** -- Linux AMD64 and ARM64 binaries and Docker images
- **Minimal footprint** -- scratch-based Docker image, starts in under 1 second, ~50 MB memory baseline

## Quick Start

### Docker

```bash
docker run -p 8080:8080 \
  -e S3_BUCKET=my-rush-cache \
  -e S3_REGION=us-east-1 \
  -e CACHE_TOKENS_READ_ONLY=tok_ro_dev1,tok_ro_dev2 \
  -e CACHE_TOKENS_READ_WRITE=tok_rw_ci \
  -e OTEL_SDK_DISABLED=true \
  ghcr.io/brunojppb/rush-cache-server:latest
```

### Binary

Download a prebuilt binary from the [Releases](https://github.com/brunojppb/rush-cache-server/releases) page and run it directly:

```bash
export S3_BUCKET=my-rush-cache
export S3_REGION=us-east-1
export CACHE_TOKENS_READ_ONLY=tok_ro_dev1
export CACHE_TOKENS_READ_WRITE=tok_rw_ci
./rush-cache-server
```

### GitHub Action

Run the cache server as a background process in your CI workflow. The action starts the server, waits for it to become healthy, and automatically shuts it down in a post step.

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Start Rush Cache Server
        uses: brunojppb/rush-cache-server@v1
        env:
          S3_BUCKET: ${{ vars.CACHE_S3_BUCKET }}
          S3_REGION: us-east-1
          S3_ACCESS_KEY: ${{ secrets.CACHE_S3_ACCESS_KEY }}
          S3_SECRET_KEY: ${{ secrets.CACHE_S3_SECRET_KEY }}
          CACHE_TOKENS_READ_ONLY: ${{ secrets.CACHE_TOKEN_RO }}
          CACHE_TOKENS_READ_WRITE: ${{ secrets.CACHE_TOKEN_RW }}
          OTEL_SDK_DISABLED: "true"
          HOST: "127.0.0.1"
          PORT: "8080"

      - name: Rush build
        env:
          RUSH_BUILD_CACHE_CREDENTIAL: "Bearer ${{ secrets.CACHE_TOKEN_RW }}"
          RUSH_BUILD_CACHE_WRITE_ALLOWED: "1"
          RUSH_BUILD_CACHE_OVERRIDE_JSON: >-
            {
              "buildCacheEnabled": true,
              "cacheProvider": "http",
              "httpConfiguration": {
                "url": "http://127.0.0.1:8080/artifacts",
                "uploadMethod": "PUT",
                "isCacheWriteAllowed": true
              }
            }
        run: rush rebuild --verbose
```

Since the server runs on `localhost` inside the CI runner, Rush needs to know about it. The `url` in `build-cache.json` is a static value checked into your repo (typically pointing to a deployed instance), so CI uses `RUSH_BUILD_CACHE_OVERRIDE_JSON` to replace the entire config at runtime and point Rush at the local server instead.

The server binaries (Linux x64 and arm64) are committed to the `action/` directory on each tagged release, so the action works without downloading anything at runtime.

## Rush Client Configuration

In your `build-cache.json`:

```jsonc
{
  "buildCacheEnabled": true,
  "cacheProvider": "http",
  "httpConfiguration": {
    "url": "https://your-cache-server.internal/artifacts",
    "uploadMethod": "PUT",
    "isCacheWriteAllowed": false
  }
}
```

On each client machine or CI job, set the credentials:

```bash
# All clients (devs + CI)
export RUSH_BUILD_CACHE_CREDENTIAL="Bearer tok_ro_dev1"

# CI on main branch (write access)
export RUSH_BUILD_CACHE_CREDENTIAL="Bearer tok_rw_ci"
export RUSH_BUILD_CACHE_WRITE_ALLOWED=1
```

### Pointing Rush to a local server

The `url` in `build-cache.json` is checked into your repo and typically points to a deployed cache server. To test against a locally running instance, use `RUSH_BUILD_CACHE_OVERRIDE_JSON` to replace the config at runtime without editing any committed files:

```bash
export RUSH_BUILD_CACHE_CREDENTIAL="Bearer tok_readwrite_1"
export RUSH_BUILD_CACHE_WRITE_ALLOWED=1
export RUSH_BUILD_CACHE_OVERRIDE_JSON='{
  "buildCacheEnabled": true,
  "cacheProvider": "http",
  "httpConfiguration": {
    "url": "http://localhost:8080/artifacts",
    "uploadMethod": "PUT",
    "isCacheWriteAllowed": true
  }
}'

rush rebuild --verbose
```

Alternatively, you can point to a JSON file with `RUSH_BUILD_CACHE_OVERRIDE_JSON_FILE_PATH` instead of inlining the JSON. These two variables are mutually exclusive.

## Configuration

### Required

| Variable                  | Description                       | Example             |
| ------------------------- | --------------------------------- | ------------------- |
| `S3_BUCKET`               | S3 bucket name                    | `my-rush-cache`     |
| `S3_REGION`               | AWS region                        | `us-east-1`         |
| `CACHE_TOKENS_READ_ONLY`  | Comma-separated read-only tokens  | `tok_ro_1,tok_ro_2` |
| `CACHE_TOKENS_READ_WRITE` | Comma-separated read-write tokens | `tok_rw_1`          |

### Optional

| Variable            | Default         | Description                                      |
| ------------------- | --------------- | ------------------------------------------------ |
| `PORT`              | `8080`          | HTTP listen port                                 |
| `HOST`              | `0.0.0.0`       | Bind address                                     |
| `S3_PREFIX`         | `rush-cache`    | Object key prefix in the bucket                  |
| `S3_ENDPOINT`       | _(AWS default)_ | Custom S3 endpoint for MinIO, R2, etc.           |
| `S3_ACCESS_KEY`     | _(AWS chain)_   | Explicit S3 access key                           |
| `S3_SECRET_KEY`     | _(AWS chain)_   | Explicit S3 secret key                           |
| `S3_USE_PATH_STYLE` | `false`         | Use path-style URLs (required by MinIO)          |
| `LOG_LEVEL`         | `info`          | Log verbosity (`debug`, `info`, `warn`, `error`) |
| `MAX_BODY_SIZE`     | `524288000`     | Max upload size in bytes (default 500 MB)        |
| `LOGS_DIRECTORY`    | _(none)_        | Directory for file-based log output              |

### S3 Authentication

The server resolves AWS credentials using the standard chain:

1. `S3_ACCESS_KEY` / `S3_SECRET_KEY` environment variables (explicit)
2. AWS environment variables (`AWS_ACCESS_KEY_ID`, etc.)
3. Instance profile / ECS task role / IRSA (when running on AWS)

## API

### `GET /artifacts/{cacheId}`

Retrieve a cached artifact. Requires a read-only or read-write token.

- `200` -- cache hit, returns raw binary blob
- `404` -- cache miss (Rush treats this as a silent skip)
- `401` -- missing or invalid token

### `PUT /artifacts/{cacheId}`

Store an artifact. Requires a read-write token.

- `200` -- stored successfully
- `403` -- valid token but lacks write permission
- `401` -- missing or invalid token

### `GET /health`

Health check. No authentication required. Returns `{"status": "healthy"}`.

## OpenTelemetry

The server ships with full OTLP support for traces and metrics, compatible with Datadog, Honeycomb, New Relic, Jaeger, Prometheus, and any OTLP-capable backend.

| Variable                      | Default                 | Description                            |
| ----------------------------- | ----------------------- | -------------------------------------- |
| `OTEL_SDK_DISABLED`           | `false`                 | Set to `true` to disable OTel entirely |
| `OTEL_EXPORTER_OTLP_ENDPOINT` | `http://localhost:4317` | OTLP gRPC endpoint                     |

System metrics (CPU and memory) are automatically registered as OpenTelemetry gauges when OTel is enabled.

To try the observability stack locally:

```bash
docker compose up -d
```

This starts RustFS (S3-compatible), OTel Collector, Prometheus, and Jaeger. The Jaeger UI is available at [http://localhost:16686](http://localhost:16686).

## Development

### Prerequisites

- [Rust](https://rustup.rs/) 1.94+
- Docker (for local S3 and observability stack)

### Running locally

```bash
# Start the local S3 store
docker compose up s3_bucket -d

# Copy and edit the example config
cp .env.example .env

# Run the server
cargo run
```

### Running tests

```bash
cargo test
```

Tests use [Wiremock](https://crates.io/crates/wiremock) to mock S3 responses -- no running S3 instance required.

### Linting

```bash
cargo fmt -- --check
cargo clippy -- -D warnings
```

## Deployment

### Docker

Multi-platform images (AMD64 + ARM64) are published to both Docker Hub and GitHub Container Registry on every release.

```bash
docker pull ghcr.io/brunojppb/rush-cache-server:latest
```

The image is built from `scratch` -- it contains only the static binary and CA certificates. It runs as a non-root user and shuts down gracefully on `SIGTERM`.

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: rush-cache-server
spec:
  replicas: 2
  selector:
    matchLabels:
      app: rush-cache-server
  template:
    metadata:
      labels:
        app: rush-cache-server
    spec:
      containers:
        - name: rush-cache-server
          image: ghcr.io/brunojppb/rush-cache-server:latest
          ports:
            - containerPort: 8080
          env:
            - name: S3_BUCKET
              value: my-rush-cache
            - name: S3_REGION
              value: us-east-1
            - name: CACHE_TOKENS_READ_ONLY
              valueFrom:
                secretKeyRef:
                  name: rush-cache-tokens
                  key: read-only
            - name: CACHE_TOKENS_READ_WRITE
              valueFrom:
                secretKeyRef:
                  name: rush-cache-tokens
                  key: read-write
            - name: OTEL_SDK_DISABLED
              value: "true"
          livenessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 2
            periodSeconds: 10
          readinessProbe:
            httpGet:
              path: /health
              port: 8080
            initialDelaySeconds: 2
            periodSeconds: 5
          resources:
            requests:
              cpu: 100m
              memory: 64Mi
            limits:
              memory: 128Mi
```

### Cache Expiration

The server does not manage cache expiration. Use [S3 lifecycle rules](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lifecycle-mgmt.html) to automatically delete objects older than N days:

```json
{
  "Rules": [
    {
      "ID": "expire-rush-cache",
      "Filter": { "Prefix": "rush-cache/" },
      "Status": "Enabled",
      "Expiration": { "Days": 30 }
    }
  ]
}
```

## License

MIT
