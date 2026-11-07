#!/usr/bin/env python3
"""Sinov uchun soxta `claude` (pul sarflamaydi). FAKE_MODE: ok | slow | limit | apikey"""
import json, os, sys, time, uuid
mode = os.environ.get("FAKE_MODE", "ok")
a = sys.argv[1:]
if a[:2] == ["auth", "status"]:
    m = "apiKey" if mode == "apikey" else "oauth_token"
    print(json.dumps({"loggedIn": True, "authMethod": m, "apiProvider": "firstParty"})); sys.exit(0)
with open(os.environ.get("FAKE_TRACE", "/dev/null"), "a") as f:
    f.write(json.dumps({"argv": [x[:40] for x in a], "pid": os.getpid()}) + "\n")
prompt = a[a.index("-p") + 1] if "-p" in a else ""
if "rejalashtiruvchisi" in prompt:
    if os.environ.get("FAKE_PLAN") == "bad":
        print(json.dumps({"type": "result", "is_error": False, "result": "kechirasiz, JSON yo'q"})); sys.exit(0)
    plan = {"bosqich": "Sinov bosqichi", "tasks": [{"title": "Yangi A", "detail": "a"}, {"title": "Yangi B", "detail": "b"}]}
    print(json.dumps({"type": "result", "is_error": False, "result": "```json\n" + json.dumps(plan) + "\n```", "total_cost_usd": 0.0}))
    sys.exit(0)
if mode == "slow":
    time.sleep(120)
if mode == "limit":
    print("Claude AI usage limit reached|1793520000", file=sys.stderr)
    print(json.dumps({"type": "result", "is_error": True, "result": "Claude AI usage limit reached", "session_id": str(uuid.uuid4())}))
    sys.exit(1)
if mode == "tamper":
    open(os.path.join(os.getcwd(), "scripts", "agent", "guard.py"), "a").write("\n# buzildi\n")
if mode == "break":
    open(os.path.join(os.getcwd(), "src", "main.rs"), "a").write("\nfn buzilgan( {\n")
# ok: repoda zararsiz o'zgarish qiladi
open(os.path.join(os.getcwd(), ".agent", "fake-progress.txt"), "a").write("x\n")
print(json.dumps({"type": "result", "is_error": False, "result": "Bajarildi.\nTASK_DONE",
                  "session_id": str(uuid.uuid4()), "total_cost_usd": 0.0}))
