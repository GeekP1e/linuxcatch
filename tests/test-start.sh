#!/usr/bin/env bash
set -euo pipefail
project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT

# The stub records the socket passed to Compose; it never starts containers.
cat > "$temporary/docker" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
case "$1" in
  context) printf '%s\n' "$MOCK_ENDPOINT" ;;
  compose)
    if [[ "$2" == up ]]; then
      printf '%s' "$DOCKER_SOCKET" > "$RESULT_FILE"
    fi
    ;;
esac
STUB
printf '#!/bin/sh\nexit 0\n' > "$temporary/xdg-open"
chmod +x "$temporary/docker" "$temporary/xdg-open"

run_case() {
  local expected="$1"
  shift
  rm -f "$temporary/result"
  env -u DOCKER_HOST -u DOCKER_CONTEXT -u DOCKER_SOCKET \
    PATH="$temporary:$PATH" MOCK_ENDPOINT='unix:///run/user/7777/context.sock' \
    RESULT_FILE="$temporary/result" "$@" bash "$project_root/scripts/start.sh" >/dev/null
  [[ "$(cat "$temporary/result")" == "$expected" ]]
}

run_case /run/user/7777/context.sock
run_case /tmp/host.sock DOCKER_HOST=unix:///tmp/host.sock
run_case /run/user/7777/context.sock DOCKER_HOST=unix:///tmp/host.sock DOCKER_CONTEXT=selected
run_case /tmp/override.sock DOCKER_CONTEXT=selected DOCKER_SOCKET=/tmp/override.sock

rm -f "$temporary/result"
if env -u DOCKER_CONTEXT -u DOCKER_SOCKET PATH="$temporary:$PATH" \
  RESULT_FILE="$temporary/result" DOCKER_HOST=tcp://remote.invalid:2375 \
  bash "$project_root/scripts/start.sh" >/dev/null 2>&1; then
  echo 'Remote Docker was not rejected' >&2
  exit 1
fi
[[ ! -f "$temporary/result" ]]
echo 'Docker context, host, explicit socket, and remote rejection: OK'
