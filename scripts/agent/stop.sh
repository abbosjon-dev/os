#!/usr/bin/env bash
# scripts/agent/stop.sh         — muloyim: joriy sikl tugagach to'xtaydi
# scripts/agent/stop.sh --now   — darhol (joriy sikl keyingi startda tiklanadi)
cd "$(dirname "$0")/../.."
touch .agent/STOP
if [ "${1:-}" = "--now" ] && [ -f .agent/supervisor.lock ]; then
  # lock JSON ({"pid": N, ...}) yoki eski format (faqat raqam)
  PID=$(python3 -c 'import json,sys;d=json.load(open(sys.argv[1]));print(d["pid"] if isinstance(d,dict) else int(d))' .agent/supervisor.lock 2>/dev/null)
  case "$PID" in ''|*[!0-9]*) echo "lock faylidan PID o'qilmadi"; exit 1;; esac
  # avval supervisor bolalari (claude -p), keyin supervisorning o'zi
  pkill -TERM -P "$PID" 2>/dev/null || true
  kill -TERM "$PID" 2>/dev/null || true
  # faqat aniq supervisor jarayoni (chaqiruvchi shell emas)
  pkill -TERM -f '^python3 scripts/agent/supervisor.py' 2>/dev/null || true
  echo "darhol to'xtatildi (pid $PID)"
else
  echo "STOP fayli qo'yildi — joriy sikl tugagach to'xtaydi"
fi
