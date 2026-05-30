#!/usr/bin/env bash
set -euo pipefail

cleanup() {
  kill "${server_pid:-}" "${vite_pid:-}" 2>/dev/null || true
}

trap cleanup EXIT INT TERM

cargo run &
server_pid=$!

bun run dev:frontend &
vite_pid=$!

server_status=""
vite_status=""

while true; do
  if [[ -z "$server_status" ]] && ! kill -0 "$server_pid" 2>/dev/null; then
    if wait "$server_pid"; then
      server_status=0
    else
      server_status=$?
    fi
  fi

  if [[ -z "$vite_status" ]] && ! kill -0 "$vite_pid" 2>/dev/null; then
    if wait "$vite_pid"; then
      vite_status=0
    else
      vite_status=$?
    fi
  fi

  if [[ -n "$server_status" || -n "$vite_status" ]]; then
    break
  fi

  sleep 1
done

if [[ -n "$server_status" && -z "$vite_status" ]]; then
  kill "$vite_pid" 2>/dev/null || true
  wait "$vite_pid" || true
fi

if [[ -n "$vite_status" && -z "$server_status" ]]; then
  kill "$server_pid" 2>/dev/null || true
  wait "$server_pid" || true
fi

if [[ -n "$server_status" && "$server_status" -ne 0 ]]; then
  exit "$server_status"
fi

if [[ -n "$vite_status" && "$vite_status" -ne 0 ]]; then
  exit "$vite_status"
fi
