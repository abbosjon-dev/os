#!/usr/bin/env bash
# Avtonom supervisorni fon rejimida ishga tushiradi.
#   scripts/agent/start.sh               — boshlash (billing himoyasi avval tekshiriladi)
#   scripts/agent/start.sh --clear-halt  — limit/billing to'xtashidan keyin qo'lda qayta ruxsat
#   scripts/agent/start.sh --fg [--max-cycles N]  — oldingi rejimda
set -euo pipefail
cd "$(dirname "$0")/../.."
mkdir -p .agent/logs
FG=0; ARGS=()
for a in "$@"; do
  case "$a" in
    --clear-halt) if [ -f .agent/HALT ]; then echo "HALT sababi: $(head -1 .agent/HALT)"; rm -f .agent/HALT; echo "HALT olib tashlandi"; fi ;;
    --fg) FG=1 ;;
    *) ARGS+=("$a") ;;
  esac
done
rm -f .agent/STOP
[ -f .agent/HALT ] && { echo "HALT bor: $(cat .agent/HALT)"; exit 4; }
# Fail-closed: himoya o'tmasa — ishga tushmaydi
python3 scripts/agent/guard.py || exit 2
if [ "$FG" = 1 ]; then exec python3 scripts/agent/supervisor.py "${ARGS[@]}"; fi
nohup python3 scripts/agent/supervisor.py "${ARGS[@]}" >>.agent/logs/supervisor.out 2>&1 &
echo "supervisor ishga tushdi (pid $!). Kuzatish: tail -f .agent/logs/supervisor.log"
