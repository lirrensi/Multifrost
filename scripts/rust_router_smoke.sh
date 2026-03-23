#!/usr/bin/env bash
set -euo pipefail

repo_root="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
rust_dir="$repo_root/rust"
router_bin="$repo_root/router/target/debug/multifrost-router"

case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    exe_suffix=".exe"
    ;;
  *)
    exe_suffix=""
    ;;
esac

worker_bin="$rust_dir/target/debug/examples/math_worker$exe_suffix"
caller_bin="$rust_dir/target/debug/examples/connect$exe_suffix"

temp_home="$(mktemp -d)"
router_output_log="$temp_home/router-process.log"
router_port="$(python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
)"

export HOME="$temp_home"
export MULTIFROST_ROUTER_PORT="$router_port"
export MULTIFROST_ROUTER_BIN="$router_bin"

router_pid=""
worker_a_pid=""
worker_b_pid=""

cleanup() {
  status=$?
  for pid in "$worker_b_pid" "$worker_a_pid" "$router_pid"; do
    if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
    fi
  done
  for pid in "$worker_b_pid" "$worker_a_pid" "$router_pid"; do
    if [ -n "$pid" ]; then
      wait "$pid" 2>/dev/null || true
    fi
  done
  if [ "$status" -ne 0 ] && [ -f "$HOME/.multifrost/router.log" ]; then
    echo ""
    echo "Router log:"
    cat "$HOME/.multifrost/router.log"
    if [ -f "$router_output_log" ]; then
      echo ""
      echo "Router process output:"
      cat "$router_output_log"
    fi
  fi
  rm -rf "$temp_home"
  exit "$status"
}
trap cleanup EXIT INT TERM

wait_for_router_startup() {
  local attempts=0
  local router_log="$HOME/.multifrost/router.log"
  while :; do
    if [ -f "$router_log" ] && grep -q "multifrost-router starting on" "$router_log"; then
      return 0
    fi
    attempts=$((attempts + 1))
    if [ "$attempts" -ge 100 ]; then
      return 1
    fi
    sleep 0.1
  done
}

echo "Running router smoke test on ws://127.0.0.1:$router_port"
echo "HOME for this run: $HOME"

"$router_bin" >"$router_output_log" 2>&1 &
router_pid=$!

wait_for_router_startup

echo ""
echo "Starting first worker: math-service-a"
"$worker_bin" --service-id math-service-a &
worker_a_pid=$!

echo ""
echo "Calling math-service-a through the router"
"$caller_bin" --target math-service-a

echo ""
echo "Starting second worker: math-service-b"
"$worker_bin" --service-id math-service-b &
worker_b_pid=$!

echo ""
echo "Calling math-service-b through the existing router"
"$caller_bin" --target math-service-b

echo ""
echo "RUST_SMOKE_OK"
