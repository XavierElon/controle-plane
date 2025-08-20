#!/usr/bin/env bash
set -euo pipefail

# Determine repo root (script lives in scripts/)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
LOG_DIR="${ROOT}/logs"
mkdir -p "${LOG_DIR}"

CONTROLLER_LOG="${LOG_DIR}/controller.log"
AGENT_LOG="${LOG_DIR}/agent.log"

kill_port() {
  local p="$1"
  [[ -z "$p" ]] && return 0

  echo "[dev-up] Closing port ${p}…"
  for _ in $(seq 1 10); do
    if lsof -i :"${p}" -sTCP:LISTEN >/dev/null 2>&1; then
      local pids
      pids="$(lsof -ti tcp:"${p}" -sTCP:LISTEN || true)"
      if [[ -n "${pids}" ]]; then
        kill ${pids} >/dev/null 2>&1 || true
        sleep 0.2
      fi
    else
      echo "[dev-up] Port ${p} is closed."
      return 0
    fi
  done

  local pids
  pids="$(lsof -ti tcp:"${p}" -sTCP:LISTEN || true)"
  if [[ -n "${pids}" ]]; then
    echo "[dev-up] Force killing pids on ${p}: ${pids}"
    kill -9 ${pids} >/dev/null 2>&1 || true
  fi
}

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

  # Kill process groups (cargo + spawned binaries)
  kill_group() {
    local pid="$1"
    [[ -z "${pid}" ]] && return 0
    local pgid
    pgid="$(ps -o pgid= -p "${pid}" | tr -d ' ' || true)"
    if [[ -n "${pgid}" ]]; then
      kill -TERM -"${pgid}" >/dev/null 2>&1 || true
      sleep 0.3
      kill -KILL -"${pgid}" >/dev/null 2>&1 || true
    else
      kill -TERM "${pid}" >/dev/null 2>&1 || true
      sleep 0.3
      kill -KILL "${pid}" >/dev/null 2>&1 || true
    fi
  }

  kill_group "${AGENT_PID:-}"
  kill_group "${CONTROLLER_PID:-}"

  # Only wait if the PID exists; avoid 'wait' with no args
  if [[ -n "${AGENT_PID:-}" ]] && kill -0 "${AGENT_PID}" 2>/dev/null; then
    wait "${AGENT_PID}" >/dev/null 2>&1 || true
  fi
  if [[ -n "${CONTROLLER_PID:-}" ]] && kill -0 "${CONTROLLER_PID}" 2>/dev/null; then
    wait "${CONTROLLER_PID}" >/dev/null 2>&1 || true
  fi

  # Ensure the controller port is closed
  kill_port "${PORT:-}"
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