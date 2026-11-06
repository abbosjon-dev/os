#!/usr/bin/env bash
cd "$(dirname "$0")/../.."
P=$(cat .agent/supervisor.lock 2>/dev/null); if [ -n "$P" ] && grep -q supervisor.py /proc/$P/cmdline 2>/dev/null && ! grep -q ") Z" /proc/$P/stat; then echo "ISHLAYAPTI (pid $(cat .agent/supervisor.lock))"; else echo "ishlamayapti"; fi
[ -f .agent/HALT ] && echo "HALT: $(head -1 .agent/HALT)"
[ -f .agent/STOP ] && echo "STOP so'ralgan"
python3 - <<'PY'
import json
s=json.load(open('.agent/state.json')) if __import__('os').path.exists('.agent/state.json') else {}
print("holat:", {k:s.get(k) for k in ("status","cycle","task","consecutive_failures","est_usd_total")})
t=json.load(open('.agent/tasks.json'))["tasks"]
for x in t: print(f"  [{x.get('status','todo'):7}] {x['id']}: {x['title']}")
PY
tail -n 5 .agent/logs/supervisor.log 2>/dev/null
