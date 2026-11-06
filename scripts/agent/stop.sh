#!/usr/bin/env bash
# scripts/agent/stop.sh         — muloyim: joriy sikl tugagach to'xtaydi
# scripts/agent/stop.sh --now   — darhol (joriy sikl keyingi startda tiklanadi)
cd "$(dirname "$0")/../.."
touch .agent/STOP
if [ "${1:-}" = "--now" ] && [ -f .agent/supervisor.lock ]; then
  PID=$(cat .agent/supervisor.lock); kill -TERM "$PID" 2>/dev/null
  pkill -TERM -g "$(ps -o sid= -p "$PID" 2>/dev/null | tr -d ' ')" 2>/dev/null || true
  pkill -TERM -f "scripts/agent/supervisor.py" 2>/dev/null || true
  echo "darhol to'xtatildi (pid $PID)"
else
  echo "STOP fayli qo'yildi — joriy sikl tugagach to'xtaydi"
fi
