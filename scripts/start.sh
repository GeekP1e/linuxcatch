#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! command -v docker >/dev/null 2>&1; then
  echo "Нужен Docker Engine с Compose или Docker Desktop." >&2
  exit 1
fi
# Rootless Docker uses a socket under /run/user instead of /var/run.
if [[ -n "${DOCKER_CONTEXT:-}" ]]; then
  docker_endpoint="$(docker context inspect "$DOCKER_CONTEXT" --format '{{.Endpoints.docker.Host}}')"
else
  docker_endpoint="${DOCKER_HOST:-$(docker context inspect --format '{{.Endpoints.docker.Host}}')}"
fi
if [[ "$docker_endpoint" == unix://* ]]; then
  export DOCKER_SOCKET="${DOCKER_SOCKET:-${docker_endpoint#unix://}}"
else
  echo "Нужен локальный Docker Engine с Unix socket. Удалённый Docker не поддерживается." >&2
  exit 1
fi

docker info >/dev/null
docker compose version >/dev/null
docker compose up --build -d --wait
echo "LinuxCatch: http://localhost:8080"
if command -v xdg-open >/dev/null 2>&1; then
  xdg-open http://localhost:8080 >/dev/null 2>&1 || true
elif command -v open >/dev/null 2>&1; then
  open http://localhost:8080 || true
fi
