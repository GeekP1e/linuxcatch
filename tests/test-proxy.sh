#!/usr/bin/env bash
set -euo pipefail

# Isolated regression test: no published ports and no changes to the running app.
prefix="linuxcatch-proxy-test-$$"
network="$prefix-network"
cleanup() {
  docker rm -f "$prefix-web" "$prefix-backend" "$prefix-reserve" >/dev/null 2>&1 || true
  docker network rm "$network" >/dev/null 2>&1 || true
}
trap cleanup EXIT

server='from http.server import BaseHTTPRequestHandler, HTTPServer
class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        body = self.path.encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
HTTPServer(("0.0.0.0", 3000), Handler).serve_forever()'

start_backend() {
  docker run -d --name "$prefix-backend" --network "$network" \
    --network-alias backend linuxcatch-sandbox:local python3 -u -c "$server" >/dev/null
}

wait_for_proxy() {
  for _ in {1..24}; do
    if [[ "$(docker exec "$prefix-web" wget -q -T 1 -O - \
      'http://127.0.0.1/api/health?probe=1' 2>/dev/null || true)" == '/api/health?probe=1' ]]; then
      return 0
    fi
    sleep 0.5
  done
  return 1
}

docker network create "$network" >/dev/null
start_backend
docker run -d --name "$prefix-web" --network "$network" linuxcatch-frontend:latest >/dev/null
wait_for_proxy || { echo 'Initial proxy request failed' >&2; exit 1; }

old_ip="$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$prefix-backend")"
docker rm -f "$prefix-backend" >/dev/null
# Occupy the old address so Docker must assign a different IP to the replacement.
docker run -d --name "$prefix-reserve" --network "$network" --ip "$old_ip" \
  linuxcatch-sandbox:local sleep 60 >/dev/null
start_backend
new_ip="$(docker inspect --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$prefix-backend")"
[[ "$old_ip" != "$new_ip" ]] || { echo 'Backend IP did not change' >&2; exit 1; }
wait_for_proxy || { echo 'Proxy did not recover after backend IP changed' >&2; exit 1; }
echo 'Proxy recovered after backend replacement; path and query preserved.'
