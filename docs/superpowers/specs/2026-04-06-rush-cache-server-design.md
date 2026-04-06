# Rush HTTP Build Cache Server — Design Spec

## Purpose

An HTTP server that acts as an authenticated proxy between Rush's `@rushstack/rush-http-build-cache-plugin` and a private S3 bucket. It stores and retrieves opaque binary blobs (tar/gzip archives of build outputs) keyed by Rush-generated cache identifiers.

Clients: CI pipelines (read-write on main, read-only on PRs) and developer machines (read-only).

## Architecture

```
  Rush clients (CI / devs)
         │
    Bearer token auth
         │
  ┌──────▼──────┐
  │  rush-cache  │   Actix-web + Tokio
  │    server    │   Streaming proxy
  └──────┬──────┘
         │
    AWS SDK creds
    (env / instance role)
         │
  ┌──────▼──────┐
  │   S3 Bucket  │   Private, no public access
  └─────────────┘
```

Single Rust binary. No database. No disk. All state lives in S3.

## HTTP API

### `GET /artifacts/{cacheId}`

Retrieve a cached build artifact.

- **Auth**: Bearer token with `read-only` or `read-write` permission.
- **200**: Cache hit — raw binary blob streamed from S3.
- **404**: Cache miss (Rush treats this as silent, not an error).
- **401**: Missing/invalid token.
- **503**: S3 temporarily unreachable (Rush retries on 5xx).
- **500**: Internal/S3 error.

### `PUT /artifacts/{cacheId}`

Store a build artifact.

- **Auth**: Bearer token with `read-write` permission only.
- **Request body**: Raw binary blob. No Content-Type header from Rush. Up to 500 MB.
- **200**: Stored successfully. Body: `{ "success": true }`.
- **401**: Missing/invalid token.
- **403**: Valid token but lacks write permission.
- **500**: Internal/S3 error.

The server streams the request body directly to S3 — no in-memory buffering.

### `GET /health`

No authentication. Returns `200` with `{ "status": "healthy" }`.

## Authentication & Authorization

Two-tier static Bearer token system.

| Permission   | GET | PUT |
|-------------|-----|-----|
| `read-only`  | Yes | 403 |
| `read-write` | Yes | Yes |

**Token resolution flow:**

1. Extract `Authorization` header.
2. Verify `Bearer ` prefix (case-insensitive scheme).
3. Look up token in configured token sets.
4. Not found -> `401`. Found but insufficient permission -> `403`.

**Token configuration via environment variables:**

```
CACHE_TOKENS_READ_ONLY=token1,token2
CACHE_TOKENS_READ_WRITE=token3,token4
```

Token validation is a local HashSet lookup. No external auth calls.

## S3 Storage

**Object key format**: `{S3_PREFIX}/{cacheId}` (default prefix: `rush-cache`).

| Server operation | S3 operation | Streaming |
|-----------------|-------------|-----------|
| GET /artifacts/{cacheId} | GetObject | S3 -> HTTP response |
| PUT /artifacts/{cacheId} | PutObject | HTTP request -> S3 |

**S3 error mapping:**

| S3 error | HTTP response |
|----------|--------------|
| NoSuchKey | 404 |
| AccessDenied | 500 (log misconfiguration) |
| Timeout/connection | 503 |
| Other | 500 |

**S3 authentication**: Standard AWS credential chain (env vars, instance profile, ECS task role). Configured via `S3_REGION`, `S3_BUCKET`, and optionally `S3_ENDPOINT` for S3-compatible stores.

## Configuration

### Required env vars

| Variable | Description | Example |
|----------|-------------|---------|
| `S3_BUCKET` | Bucket name | `buffer-rush-build-cache` |
| `S3_REGION` | AWS region | `us-east-1` |
| `CACHE_TOKENS_READ_ONLY` | Comma-separated read-only tokens | `tok_abc,tok_def` |
| `CACHE_TOKENS_READ_WRITE` | Comma-separated read-write tokens | `tok_ghi` |

### Optional env vars

| Variable | Default | Description |
|----------|---------|-------------|
| `PORT` | `8080` | HTTP listen port |
| `HOST` | `0.0.0.0` | Bind address |
| `S3_PREFIX` | `rush-cache` | S3 key prefix |
| `S3_ENDPOINT` | AWS default | Custom S3 endpoint (MinIO, LocalStack) |
| `S3_ACCESS_KEY` | (from AWS chain) | Explicit S3 access key |
| `S3_SECRET_KEY` | (from AWS chain) | Explicit S3 secret key |
| `S3_USE_PATH_STYLE` | `false` | Use path-style S3 URLs |
| `LOG_LEVEL` | `info` | Logging verbosity |
| `MAX_BODY_SIZE` | `524288000` | Max upload size (500 MB) |
| `LOGS_DIRECTORY` | (none) | Optional file logging directory |

## Observability

### Structured logging

All requests logged as structured JSON via `tracing` + `tracing-subscriber` with Bunyan formatting:

```json
{
  "ts": "2026-04-06T18:30:00Z",
  "method": "GET",
  "path": "/artifacts/abc123def",
  "status": 200,
  "duration_ms": 45,
  "cache_id": "abc123def",
  "result": "hit",
  "token_prefix": "tok_abc"
}
```

Never log full tokens — only a prefix/truncated form.

### OpenTelemetry

Full OTLP integration for Datadog compatibility:

- **Traces**: Distributed tracing via `opentelemetry` + `opentelemetry-otlp` with gRPC (tonic) transport.
- **Metrics**: System metrics (CPU, memory) via `sysinfo` crate exposed as OpenTelemetry gauges.
- **Exporter**: OTLP/gRPC exporter compatible with Datadog Agent's OTLP receiver.

Controlled by standard OTel env vars (`OTEL_SDK_DISABLED`, `OTEL_EXPORTER_OTLP_ENDPOINT`, etc.).

Matches the turbo-cache-server telemetry architecture: `tracing` for structured logs, OpenTelemetry SDK for traces and metrics, tonic for gRPC transport.

## Error Response Format

All non-200 responses return:

```json
{ "error": "human-readable message" }
```

No internal details (bucket names, AWS errors, stack traces) in responses. Log them server-side.

## Project Structure

Mirrors turbo-cache-server layout:

```
src/
  main.rs              # Entry point: load config, init telemetry, bind listener, start server
  lib.rs               # Module declarations
  app_settings.rs      # Configuration from env vars
  startup.rs           # Actix-web server setup, route registration
  storage.rs           # S3 storage abstraction (get, put, streaming)
  telemetry.rs         # Tracing + OpenTelemetry setup, system metrics
  auth/
    mod.rs             # Auth module
    bearer_token.rs    # Bearer token validation middleware (two-tier)
  routes/
    mod.rs             # Routes module
    artifacts.rs       # GET/PUT /artifacts/{cacheId} handlers
    health_check.rs    # GET /health handler

tests/e2e/
  main.rs              # Test module root
  helpers.rs           # Test app spawning, mock S3 setup
  health_check.rs      # Health endpoint tests
  auth.rs              # Auth tests (missing, invalid, read-only write attempt, valid)
  artifacts.rs         # Artifact upload/download/miss tests

.github/workflows/
  ci.yml               # Lint (fmt, clippy) + test
  build.yml            # PR binary builds
  build-binaries.yml   # Reusable: Linux AMD64 + ARM64 cross-compilation
  release.yml          # Version bump, build, Docker push, GH release

Dockerfile             # Multi-stage: cargo-chef + cargo-zigbuild -> scratch
docker-compose.yml     # Local dev: S3-compatible store + OTel stack
.env.example           # Configuration template
```

## Dependencies (Cargo.toml)

| Crate | Purpose |
|-------|---------|
| actix-web 4 | HTTP framework |
| tokio 1 | Async runtime |
| rust-s3 | S3 client |
| serde / serde_json | Serialization |
| tracing / tracing-subscriber / tracing-appender | Structured logging |
| opentelemetry / opentelemetry-otlp / opentelemetry-stdout | OTel SDK |
| tonic | gRPC transport for OTLP |
| sysinfo | System metrics |
| openssl (vendored) | TLS |
| futures | Stream utilities |
| http | HTTP types |

**Dev dependencies**: reqwest, wiremock, pretty_assertions, tokio (test)

## Testing Strategy

### E2E tests (tests/e2e/)

All tests use Wiremock to mock S3 responses. Test app binds to random port.

**Health check**: Verify 200 + expected body.

**Auth tests**:
- Missing Authorization header -> 401
- Invalid token -> 401
- Read-only token on GET -> 200
- Read-only token on PUT -> 403
- Read-write token on PUT -> 200

**Artifact tests**:
- PUT artifact, verify S3 receives correct key and body
- GET artifact, verify byte-for-byte response from S3
- GET non-existent key -> 404
- PUT + GET round-trip

### Unit tests

- `app_settings.rs`: Config parsing, defaults, required field validation
- `auth/bearer_token.rs`: Token lookup logic

## Build & Deployment

### Docker image

Multi-stage Dockerfile (mirrors turbo-cache-server):

1. **chef**: Rust + cargo-chef + cargo-zigbuild + build deps
2. **planner**: Analyze dependency graph
3. **builder**: Cross-compile for `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`
4. **final**: Scratch image with binary + CA certs only

Release profile: `strip = true`, `lto = true`, size-optimized.

Runs as non-root. Exposes port 8080. Graceful shutdown on SIGTERM.

### CI/CD (GitHub Actions)

- **ci.yml**: On push/PR — `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`
- **build.yml**: On PR — build Linux AMD64 + ARM64 binaries
- **build-binaries.yml**: Reusable workflow for cross-compilation via Docker
- **release.yml**: Manual trigger — version bump, build, Docker push (Hub + GHCR), GH release with changelog

No macOS builds.

## Non-Goals

- Cache eviction (use S3 lifecycle policies)
- Cache invalidation API (Rush uses content-addressed hashes)
- Multi-bucket support (use S3_PREFIX for namespacing)
- Request deduplication (S3 last-writer-wins is fine — same hash = same content)
- Web UI (machine-to-machine service)
- macOS binary builds
