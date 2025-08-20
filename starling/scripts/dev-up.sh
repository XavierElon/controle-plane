#!/usr/bin/env bash
set -euo pipefail

# Determine repo root (script lives in scripts/)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
LOG_DIR="${ROOT}/logs"
mkdir -p "${LOG_DIR}"

CONTROLLER_LOG="${LOG_DIR}/controller.log"
AGENT_LOG="${LOG_DIR}/agent.log"

# Build everything once
echo "[dev-up] Building workspace…"
(
  cd "${ROOT}"
  cargo build
)

# Start controller in background
echo "[dev-up] Starting controller… logs: ${CONTROLLER_LOG}"
# Clear old log
: > "${CONTROLLER_LOG}"
(
  cd "${ROOT}/controller"
  RUST_LOG=info cargo run
) >> "${CONTROLLER_LOG}" 2>&1 &
CONTROLLER_PID=$!

cleanup() {
  echo
  echo "[dev-up] Shutting down…"
  kill ${AGENT_PID:-} >/dev/null 2>&1 || true
  kill ${CONTROLLER_PID:-} >/dev/null 2>&1 || true
  wait ${AGENT_PID:-} >/dev/null 2>&1 || true
  wait ${CONTROLLER_PID:-} >/dev/null 2>&1 || true
}
trap cleanup INT TERM EXIT

# Wait for controller to print its bind address and extract port
echo -n "[dev-up] Waiting for controller to announce port"
PORT=""
for _ in $(seq 1 50); do
  if grep -q "Starting controller on " "${CONTROLLER_LOG}"; then
    ADDR_LINE="$(grep "Starting controller on " "${CONTROLLER_LOG}" | tail -n1)"
    # expected format: "Starting controller on 0.0.0.0:PORT"
    PORT="$(echo "${ADDR_LINE}" | sed -E 's/.* on .*:([0-9]+).*/\1/')"
    break
  fi
  echo -n "."
  sleep 0.2
done
echo

if [[ -z "${PORT}" ]]; then
  echo "[dev-up] Could not detect controller port. Check ${CONTROLLER_LOG}."
  exit 1
fi

# Wait until the port is actually listening
echo -n "[dev-up] Waiting for port ${PORT} to listen"
for _ in $(seq 1 50); do
  if lsof -i :"${PORT}" -sTCP:LISTEN >/dev/null 2>&1; then
    break
  fi
  echo -n "."
  sleep 0.2
done
echo
echo "[dev-up] Controller listening on http://127.0.0.1:${PORT}"

# Start agent against detected controller URL
echo "[dev-up] Starting agent… logs: ${AGENT_LOG}"
: > "${AGENT_LOG}"
(
  cd "${ROOT}/agent"
  CONTROLLER_URL="http://127.0.0.1:${PORT}" cargo run
) >> "${AGENT_LOG}" 2>&1 &
AGENT_PID=$!

echo "[dev-up] Tailing logs. Press Ctrl-C to stop."
echo "[dev-up] Submit a job with:"
echo "  curl -s -X POST http://127.0.0.1:${PORT}/v1/jobs -H 'content-type: application/json' -d '{\"name\":\"echo-hello\",\"replicas\":2,\"command\":[\"echo\",\"hello from starling\"],\"constraints\":[],\"priority\":5}'"
echo

# Tail both logs
tail -n +1 -f "${CONTROLLER_LOG}" "${AGENT_LOG}"