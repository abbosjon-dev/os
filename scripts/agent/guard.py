#!/usr/bin/env python3
"""Billing / autentifikatsiya himoyasi (fail-closed).

Har bir Claude chaqiruvidan OLDIN ishlaydi. Birorta tekshiruv aniq "OK"
bermasa — xato qaytaradi va supervisor to'xtaydi. Hech qanday billing
sozlamasini O'ZGARTIRMAYDI, faqat o'qiydi.

Ruxsat etilgan yagona holat:
  * Claude Code obuna (OAuth) orqali kirgan: authMethod oauth/claude.ai,
    apiProvider = firstParty;
  * hech qayerda API kalit yo'q (env, apiKeyHelper, primaryApiKey);
  * Bedrock/Vertex/Foundry va boshqa pullik provayderlar o'chiq;
  * ANTHROPIC_BASE_URL yo'q yoki rasmiy https://api.anthropic.com;
  * qo'shimcha foydalanish (extra usage / kreditlar) o'chirilgan.
"""
import json
import os
import subprocess
import sys

HOME = os.path.expanduser("~")

# Bular mavjud bo'lsa — API to'lovi yoki boshqa provayder ishlatilishi mumkin.
FORBIDDEN_ENV = [
    "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "CLAUDE_API_KEY",
    "CLAUDE_CODE_API_KEY_HELPER", "CLAUDE_CODE_API_KEY_HELPER_TTL_MS",
    "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY",
    "AWS_BEARER_TOKEN_BEDROCK", "ANTHROPIC_BEDROCK_BASE_URL",
    "ANTHROPIC_VERTEX_PROJECT_ID", "ANTHROPIC_VERTEX_BASE_URL",
    "ANTHROPIC_FOUNDRY_API_KEY", "ANTHROPIC_FOUNDRY_RESOURCE", "ANTHROPIC_FOUNDRY_BASE_URL",
    "ANTHROPIC_SMALL_FAST_MODEL_AWS_REGION", "CLAUDE_CODE_SKIP_BEDROCK_AUTH",
    "CLAUDE_CODE_SKIP_VERTEX_AUTH", "CLAUDE_CODE_SKIP_FOUNDRY_AUTH",
    "OPENAI_API_KEY", "OPENROUTER_API_KEY",
]
# settings.json ichidagi xavfli kalitlar
FORBIDDEN_SETTINGS = ["apiKeyHelper", "awsAuthRefresh", "awsCredentialExport",
                      "gcpAuthRefresh", "forceLoginMethod_console"]
ALLOWED_AUTH = {"oauth_token", "claude.ai", "claudeai", "oauth"}
OFFICIAL_BASE = {"", "https://api.anthropic.com", "https://api.anthropic.com/"}
# Supervisor buyruq qatorida hech qachon bo'lmasligi kerak bo'lgan bayroqlar
FORBIDDEN_ARGS = ["--fallback-model", "--api-key", "--bare"]


def settings_files(repo):
    return [
        os.path.join(HOME, ".claude", "settings.json"),
        os.path.join(HOME, ".claude", "settings.local.json"),
        os.path.join(HOME, ".claude", "launcher-settings.json"),
        os.path.join(HOME, ".claude", "remote-settings.json"),
        "/etc/claude-code/managed-settings.json",
        "/Library/Application Support/ClaudeCode/managed-settings.json",
        os.path.join(repo, ".claude", "settings.json"),
        os.path.join(repo, ".claude", "settings.local.json"),
    ]


def sanitized_env():
    """Bola jarayon uchun muhit: taqiqlangan o'zgaruvchilar olib tashlanadi."""
    env = dict(os.environ)
    for k in FORBIDDEN_ENV:
        env.pop(k, None)
    env["DISABLE_AUTOUPDATER"] = "1"
    return env


def check(repo, claude_bin, cmd_preview=None):
    """[] — hammasi joyida; aks holda muammolar ro'yxati (fail-closed)."""
    errs = []
    # 1. Muhit o'zgaruvchilari
    for k in FORBIDDEN_ENV:
        if os.environ.get(k):
            errs.append(f"muhitda taqiqlangan o'zgaruvchi bor: {k}")
    base = os.environ.get("ANTHROPIC_BASE_URL", "")
    if base not in OFFICIAL_BASE:
        errs.append(f"ANTHROPIC_BASE_URL rasmiy emas: {base}")

    # 2. Sozlama fayllari
    for f in settings_files(repo):
        if not os.path.exists(f):
            continue
        try:
            d = json.load(open(f))
        except Exception as e:
            errs.append(f"{f} o'qib bo'lmadi ({e}) — noaniq holat")
            continue
        if not isinstance(d, dict):
            continue
        for k in FORBIDDEN_SETTINGS:
            if k in d:
                errs.append(f"{f}: '{k}' sozlamasi bor (API/provayder to'lovi xavfi)")
        env = d.get("env") or {}
        if isinstance(env, dict):
            for k in env:
                if k in FORBIDDEN_ENV:
                    errs.append(f"{f}: env.{k} bor")
                if k == "ANTHROPIC_BASE_URL" and env[k] not in OFFICIAL_BASE:
                    errs.append(f"{f}: env.ANTHROPIC_BASE_URL rasmiy emas")

    # 3. ~/.claude.json — API kalit yo'q, qo'shimcha foydalanish o'chiq
    cj = os.path.join(HOME, ".claude.json")
    try:
        j = json.load(open(cj))
    except Exception as e:
        j = None
        errs.append(f"{cj} o'qib bo'lmadi ({e}) — noaniq holat")
    if j is not None:
        if j.get("primaryApiKey"):
            errs.append("~/.claude.json: primaryApiKey saqlangan (API kalit)")
        if j.get("customApiKeyResponses", {}).get("approved"):
            errs.append("~/.claude.json: tasdiqlangan maxsus API kalit bor")
        if not j.get("oauthAccount"):
            errs.append("~/.claude.json: oauthAccount yo'q — obuna hisobi aniqlanmadi")
        if not j.get("cachedExtraUsageDisabledReason"):
            errs.append("qo'shimcha foydalanish (extra usage) o'chirilgani tasdiqlanmadi")
    cred = os.path.join(HOME, ".claude", ".credentials.json")
    if os.path.exists(cred):
        try:
            c = json.load(open(cred))
            if any(k.lower().find("apikey") >= 0 for k in c):
                errs.append(".credentials.json ichida API kalit yozuvi bor")
        except Exception as e:
            errs.append(f".credentials.json o'qib bo'lmadi ({e})")

    # 4. Claude Code o'zi nima deydi
    try:
        r = subprocess.run([claude_bin, "auth", "status", "--json"], capture_output=True,
                           text=True, timeout=60, env=sanitized_env())
        st = json.loads(r.stdout)
        if st.get("loggedIn") is not True:
            errs.append("claude auth status: kirilmagan")
        if str(st.get("authMethod", "")).lower() not in ALLOWED_AUTH:
            errs.append(f"authMethod obuna emas: {st.get('authMethod')}")
        if st.get("apiProvider") != "firstParty":
            errs.append(f"apiProvider firstParty emas: {st.get('apiProvider')}")
        if st.get("apiKeySource") not in (None, "", "none"):
            errs.append(f"apiKeySource bor: {st.get('apiKeySource')}")
    except Exception as e:
        errs.append(f"claude auth status ishlamadi ({e}) — noaniq holat")

    # 5. Buyruq qatori
    if cmd_preview:
        for a in cmd_preview:
            if any(a.startswith(f) for f in FORBIDDEN_ARGS):
                errs.append(f"taqiqlangan bayroq buyruqda: {a}")
    return errs


# Obuna limiti / pullik takliflar belgilari (kichik harfda qidiriladi)
LIMIT_MARKERS = [
    "usage limit", "limit reached", "limit will reset", "resets at", "rate_limit_error",
    "5-hour limit", "weekly limit", "out of extra usage", "extra usage",
    "credit balance", "purchase", "buy more", "upgrade your plan", "billing",
    "quota", "too many requests", "429",
]


def limit_hit(text, result=None):
    """Natija/log matnida limit yoki to'lov taklifi bormi."""
    if result is not None:
        if result.get("terminal_reason") in ("usage_limit", "rate_limit", "quota_exceeded"):
            return "terminal_reason=" + str(result.get("terminal_reason"))
        if result.get("is_error"):
            t = str(result.get("result", "")).lower()
            for m in LIMIT_MARKERS:
                if m in t:
                    return m
    low = text.lower()
    for m in LIMIT_MARKERS[:9]:  # qattiq belgilar — har qanday joyda
        if m in low:
            return m
    return None


if __name__ == "__main__":
    repo = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
    bin_ = os.environ.get("ZAMIN_CLAUDE_BIN", "claude")
    e = check(repo, bin_)
    if e:
        print("BILLING HIMOYASI: XATO — avtonom sikl ISHGA TUSHMAYDI")
        for x in e:
            print("  -", x)
        sys.exit(2)
    print("BILLING HIMOYASI: OK — faqat obuna (OAuth, firstParty), API kalit yo'q, extra usage o'chiq")
