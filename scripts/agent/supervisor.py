#!/usr/bin/env python3
"""Zamin OS avtonom supervisori (Claude Code headless CLI ustida).

Sikl: ANALYZE -> PLAN -> EXECUTE -> TEST -> FIX -> VERIFY -> NEXT
  * har sikldan oldin billing himoyasi (guard.check) — xato bo'lsa to'xtaydi;
  * obuna limiti belgisi chiqsa — darhol to'xtaydi, HALT fayl yoziladi,
    qayta urinmaydi (faqat foydalanuvchi `start.sh --clear-halt` qiladi);
  * holat .agent/state.json da; to'xtab qolgan sikl qayta tiklanadi;
  * vazifa bajarilganini supervisor o'zi tekshiradi (verify.sh full).

Ishlatish: scripts/agent/start.sh | stop.sh | status.sh
"""
import argparse
import datetime
import json
import os
import signal
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
import guard  # noqa: E402

AG = os.path.join(REPO, ".agent")
LOGS = os.path.join(AG, "logs")
STATE = os.path.join(AG, "state.json")
TASKS = os.path.join(AG, "tasks.json")
LOCK = os.path.join(AG, "supervisor.lock")
STOP = os.path.join(AG, "STOP")
HALT = os.path.join(AG, "HALT")
CONTEXT = os.path.join(AG, "CONTEXT.md")

DEFAULT_CFG = {
    "claude_bin": "claude",
    "max_cycles": 20,
    "max_consecutive_failures": 3,
    "cycle_timeout_s": 3600,
    # obuna ichida hisoblangan (taxminiy) foydalanish chegarasi — pul emas,
    # lekin ortiqcha sarfni cheklaydi
    "max_est_usd_per_cycle": 15.0,
    "max_est_usd_total": 150.0,
    "permission_mode": "acceptEdits",
    "allowed_tools": "Bash Read Edit Write Glob Grep TodoWrite",
    "disallowed_tools": "WebSearch Bash(git push:*) Bash(git commit:*) Bash(git reset:*) Bash(claude:*)",
    "push": True,
    "branch": "main",
    "verify_mode": "full",
}

RUNNING = {"child": None, "stop": False}


def now():
    return datetime.datetime.now().strftime("%Y-%m-%d %H:%M:%S")


def log(msg):
    os.makedirs(LOGS, exist_ok=True)
    line = f"[{now()}] {msg}"
    print(line, flush=True)
    with open(os.path.join(LOGS, "supervisor.log"), "a") as f:
        f.write(line + "\n")


def jload(p, default):
    try:
        with open(p) as f:
            return json.load(f)
    except Exception:
        return default


def jsave(p, d):
    tmp = p + ".tmp"
    with open(tmp, "w") as f:
        json.dump(d, f, indent=2, ensure_ascii=False)
    os.replace(tmp, p)


def load_cfg(path):
    cfg = dict(DEFAULT_CFG)
    cfg.update(jload(path, {}))
    if os.environ.get("ZAMIN_CLAUDE_BIN"):
        cfg["claude_bin"] = os.environ["ZAMIN_CLAUDE_BIN"]
    if os.environ.get("ZAMIN_NO_PUSH"):
        cfg["push"] = False
    return cfg


def git(*a, check=False):
    r = subprocess.run(["git", *a], cwd=REPO, capture_output=True, text=True)
    if check and r.returncode:
        raise RuntimeError(f"git {' '.join(a)}: {r.stderr.strip()}")
    return r.stdout.strip()


def acquire_lock():
    if os.path.exists(LOCK):
        try:
            pid = int(open(LOCK).read().strip())
            os.kill(pid, 0)
            # zombi yoki PID qayta ishlatilgan bo'lsa — o'lik deb hisoblaymiz
            stat = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()[0]
            cmdl = open(f"/proc/{pid}/cmdline").read()
            if stat == "Z" or "supervisor.py" not in cmdl:
                raise ProcessLookupError
            log(f"boshqa supervisor ishlayapti (pid {pid}) — chiqaman")
            sys.exit(3)
        except (ProcessLookupError, ValueError, FileNotFoundError):
            log("eski lock topildi (jarayon o'lgan) — olib tashlayman")
    with open(LOCK, "w") as f:
        f.write(str(os.getpid()))


def release_lock():
    try:
        if open(LOCK).read().strip() == str(os.getpid()):
            os.remove(LOCK)
    except Exception:
        pass


def on_signal(sig, _frm):
    RUNNING["stop"] = True
    log(f"signal {sig}: muloyim to'xtash so'raldi")
    ch = RUNNING["child"]
    if ch and ch.poll() is None and sig == signal.SIGINT:
        ch.terminate()


def halt(state, reason):
    """Obuna limiti / billing noaniqligi: hamma narsa to'xtaydi."""
    state["status"] = "halted"
    state["halt_reason"] = reason
    state["halted_at"] = now()
    jsave(STATE, state)
    with open(HALT, "w") as f:
        f.write(f"{now()} {reason}\n"
                "Qayta boshlash faqat qo'lda: scripts/agent/start.sh --clear-halt\n")
    log(f"HALT: {reason} — Claude so'rovlari to'xtatildi, qayta urinish YO'Q")


def next_task(tasks):
    for t in tasks.get("tasks", []):
        if t.get("status") != "done" and t.get("status") != "blocked":
            return t
    return None


def build_prompt(task, state, recovering, fail_log):
    ctx = open(CONTEXT).read() if os.path.exists(CONTEXT) else ""
    p = [ctx, "", f"## JORIY VAZIFA ({task['id']}): {task['title']}", task.get("detail", "")]
    if recovering:
        p += ["", "DIQQAT: oldingi sikl uzilib qoldi. Avval `git status` va `git diff` bilan "
              "qisman ishni ko'rib chiq, davom ettir yoki tozalab qaytadan qil."]
    if fail_log:
        p += ["", "Oldingi urinishda tekshiruv XATO berdi. Log oxiri:", "```", fail_log[-3000:], "```",
              "Avval shu xatoni tuzat."]
    p += ["", "Tugatgach: `scripts/agent/verify.sh full` o'tishi shart. Commit qilma va push qilma — "
          "supervisor o'zi tekshirib commit qiladi. Oxirgi javobing qisqa hisobot bo'lsin "
          "(o'zbek tilida), oxirgi qatorda TASK_DONE yoki TASK_BLOCKED: <sabab>."]
    return "\n".join(p)


def run_claude(cfg, prompt, state, cyc):
    cmd = [cfg["claude_bin"], "-p", prompt, "--output-format", "json",
           "--permission-mode", cfg["permission_mode"],
           "--allowedTools", cfg["allowed_tools"],
           "--disallowedTools", cfg["disallowed_tools"],
           "--max-budget-usd", str(cfg["max_est_usd_per_cycle"])]
    if state.get("session_id") and state.get("resume_session"):
        cmd += ["--resume", state["session_id"]]
    errs = guard.check(REPO, cfg["claude_bin"], cmd)
    if errs:
        return None, "guard:" + "; ".join(errs)
    out_p = os.path.join(LOGS, f"cycle-{cyc:04d}.json")
    err_p = os.path.join(LOGS, f"cycle-{cyc:04d}.stderr")
    with open(out_p, "w") as fo, open(err_p, "w") as fe:
        ch = subprocess.Popen(cmd, cwd=REPO, stdout=fo, stderr=fe, env=guard.sanitized_env(),
                              start_new_session=True)
        RUNNING["child"] = ch
        try:
            ch.wait(timeout=cfg["cycle_timeout_s"])
        except subprocess.TimeoutExpired:
            os.killpg(ch.pid, signal.SIGTERM)
            time.sleep(5)
            if ch.poll() is None:
                os.killpg(ch.pid, signal.SIGKILL)
            ch.wait()
            RUNNING["child"] = None
            return {"_timeout": True}, open(err_p).read()
        RUNNING["child"] = None
    raw = open(out_p).read()
    stderr = open(err_p).read()
    try:
        res = json.loads(raw.strip().splitlines()[-1]) if raw.strip() else {}
    except Exception:
        res = {"_unparsed": raw[-2000:]}
    res["_rc"] = ch.returncode
    return res, stderr


def verify(cfg):
    r = subprocess.run([os.path.join(HERE, "verify.sh"), cfg["verify_mode"]], cwd=REPO,
                       capture_output=True, text=True, timeout=900)
    tail = r.stdout[-400:]
    blog = os.path.join(LOGS, "verify-build.log")
    if r.returncode and os.path.exists(blog):
        tail += "\n" + open(blog).read()[-2500:]
    return r.returncode == 0, tail


def commit_and_push(cfg, task, cyc):
    if not git("status", "--porcelain"):
        return False
    git("add", "-A")
    msg = (f"Agent sikl {cyc}: {task['title']}\n\n"
           f"Avtonom supervisor ({task['id']}); verify.sh {cfg['verify_mode']} o'tdi.\n\n"
           "Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>\n"
           "Claude-Session: https://claude.ai/code/session_01XP37bWy7eR4Ndmx7yhzNeM")
    r = subprocess.run(["git", "commit", "-q", "-m", msg], cwd=REPO, capture_output=True, text=True)
    if r.returncode:
        log("commit bo'lmadi: " + r.stderr.strip()[:300])
        return False
    if cfg["push"]:
        for d in (0, 2, 4, 8, 16):
            time.sleep(d)
            p = subprocess.run(["git", "push", "-u", "origin", cfg["branch"]], cwd=REPO,
                               capture_output=True, text=True)
            if p.returncode == 0:
                log("push: OK")
                break
            log("push xato: " + p.stderr.strip()[:200])
    return True


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--config", default=os.path.join(AG, "config.json"))
    ap.add_argument("--max-cycles", type=int)
    a = ap.parse_args()
    os.makedirs(LOGS, exist_ok=True)
    cfg = load_cfg(a.config)
    if a.max_cycles:
        cfg["max_cycles"] = a.max_cycles
    if os.path.exists(HALT):
        log("HALT fayli bor — ishga tushmayman. Sabab: " + open(HALT).read().strip())
        return 4
    acquire_lock()
    signal.signal(signal.SIGTERM, on_signal)
    signal.signal(signal.SIGINT, on_signal)
    state = jload(STATE, {})
    try:
        return loop(cfg, state)
    finally:
        release_lock()


def loop(cfg, state):
    state.setdefault("cycle", 0)
    state.setdefault("consecutive_failures", 0)
    state.setdefault("est_usd_total", 0.0)
    recovering = state.get("status") == "running"
    if recovering:
        log(f"uzilgan sikl topildi (#{state['cycle']}) — tiklash rejimi")
    done_this_run = 0
    while True:
        if RUNNING["stop"] or os.path.exists(STOP):
            log("STOP — muloyim to'xtadim")
            state["status"] = "stopped"
            jsave(STATE, state)
            return 0
        if done_this_run >= cfg["max_cycles"]:
            log(f"max_cycles={cfg['max_cycles']} yetdi")
            state["status"] = "idle"
            jsave(STATE, state)
            return 0
        if state["consecutive_failures"] >= cfg["max_consecutive_failures"]:
            log("ketma-ket xatolar chegarasi — to'xtadim")
            state["status"] = "failed"
            jsave(STATE, state)
            return 5
        if state["est_usd_total"] >= cfg["max_est_usd_total"]:
            log("umumiy foydalanish chegarasi (taxminiy) yetdi — to'xtadim")
            state["status"] = "budget"
            jsave(STATE, state)
            return 6
        tasks = jload(TASKS, {"tasks": []})
        task = next_task(tasks)
        if not task:
            log("barcha vazifalar bajarildi")
            state["status"] = "complete"
            jsave(STATE, state)
            return 0
        # Har sikl oldidan billing himoyasi
        errs = guard.check(REPO, cfg["claude_bin"])
        if errs:
            halt(state, "billing himoyasi: " + "; ".join(errs))
            return 2
        if not recovering:
            state["cycle"] += 1
        cyc = state["cycle"]
        state.update(status="running", task=task["id"], started=now(),
                     resume_session=bool(recovering and state.get("session_id")))
        jsave(STATE, state)
        log(f"sikl {cyc}: {task['id']} — {task['title']}" + (" (tiklash)" if recovering else ""))
        head0 = git("rev-parse", "HEAD")
        prompt = build_prompt(task, state, recovering, state.get("fail_log"))
        res, stderr = run_claude(cfg, prompt, state, cyc)
        if res is None:
            halt(state, stderr)
            return 2
        if RUNNING["stop"] and res.get("_rc") not in (0, None):
            log("signal bilan uzildi — holat 'running' qoldi, keyingi startda tiklanadi")
            return 0
        lim = guard.limit_hit(stderr + "\n" + str(res.get("result", "")) + str(res.get("_unparsed", "")), res)
        if lim:
            if res.get("session_id"):
                state["session_id"] = res["session_id"]
            halt(state, f"obuna limiti / to'lov belgisi: '{lim}'")
            return 7
        est = float(res.get("total_cost_usd") or 0)
        state["est_usd_total"] = round(state["est_usd_total"] + est, 4)
        if res.get("session_id"):
            state["session_id"] = res["session_id"]
        recovering = False
        text = str(res.get("result", ""))
        ok, vlog = verify(cfg)
        claimed = "TASK_DONE" in text
        blocked = "TASK_BLOCKED" in text
        log(f"sikl {cyc} tugadi: rc={res.get('_rc')} timeout={res.get('_timeout', False)} "
            f"taxminiy={est:.3f} verify={'OK' if ok else 'XATO'} da'vo={'DONE' if claimed else ('BLOCKED' if blocked else '-')}")
        if ok:
            changed = bool(git("status", "--porcelain")) or git("rev-parse", "HEAD") != head0
            state["consecutive_failures"] = 0
            state.pop("fail_log", None)
            if claimed and changed:
                task["status"] = "done"
                task["done_at"] = now()
            elif blocked:
                task["status"] = "blocked"
                task["note"] = text[-300:]
            else:
                task["attempts"] = task.get("attempts", 0) + 1
                if task["attempts"] >= 4:
                    task["status"] = "blocked"
                    task["note"] = "4 urinishda tugallanmadi"
            jsave(TASKS, tasks)
            pushed = commit_and_push(cfg, task, cyc) if changed else False
            if changed and not pushed and cfg["push"] and git("rev-parse", "HEAD") != head0:
                subprocess.run(["git", "push", "-u", "origin", cfg["branch"]], cwd=REPO, capture_output=True)
            state["last_commit"] = git("rev-parse", "--short", "HEAD")
        else:
            state["consecutive_failures"] += 1
            state["fail_log"] = vlog
        state["status"] = "between"
        state["resume_session"] = False
        jsave(STATE, state)
        with open(os.path.join(AG, "history.jsonl"), "a") as f:
            f.write(json.dumps({"cycle": cyc, "task": task["id"], "verify": ok, "claimed": claimed,
                                "est_usd": est, "at": now()}, ensure_ascii=False) + "\n")
        done_this_run += 1


if __name__ == "__main__":
    sys.exit(main())
