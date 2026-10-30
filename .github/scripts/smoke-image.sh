#!/usr/bin/env bash
# Validate packaging without contacting S3. Usage: smoke-image.sh IMAGE BINARY
set -euo pipefail
image="$1"
binary="$2"
container=$(docker create -p 127.0.0.1::8080 \
  -e S3_BUCKET=smoke-test -e S3_REGION=us-east-1 \
  -e OTEL_SDK_DISABLED=true -e S3_ACCESS_KEY=smoke-test -e S3_SECRET_KEY=smoke-test \
  -e S3_ENDPOINT=http://127.0.0.1:9000 "$image")
scratch_dir=$(mktemp -d)
cleanup() {
  docker rm -f "$container" >/dev/null
  rm -rf "$scratch_dir"
}
trap cleanup EXIT
docker cp "$container:/usr/bin/rush-cache-server" "$scratch_dir/rush-cache-server"
cmp "$binary" "$scratch_dir/rush-cache-server"
docker cp "$container:/etc/ssl/certs/ca-certificates.crt" "$scratch_dir/ca-certificates.crt"
test -s "$scratch_dir/ca-certificates.crt"
docker start "$container" >/dev/null
address=$(docker port "$container" 8080/tcp)
for attempt in {1..30}; do
  if curl --fail --silent "http://$address/health" >/dev/null; then
    exit 0
  fi
  sleep 1
done
docker logs "$container"
echo 'Runtime health check failed' >&2
exit 1
