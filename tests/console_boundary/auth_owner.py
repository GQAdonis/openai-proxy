"""Disposable Codex App Server / exec protocol peer. Never open a user's auth home."""
import json
import os
import signal
import sys
import time
import uuid
from pathlib import Path


def emit(value):
    print(json.dumps(value), flush=True)


def main():
    if sys.argv[1:] == ["--version"]:
        print("codex-cli 0.169.0")
        return
    home = Path(os.environ["CODEX_HOME"])
    if not (home / ".synthetic-owner").is_file():
        raise SystemExit(64)
    journal = home / "observer.jsonl"

    def observed(kind, **fields):
        with journal.open("a", encoding="utf-8") as output:
            output.write(json.dumps(dict(kind=kind, pid=os.getpid(), **fields)) + "\n")

    def interrupted(_sig, _frame):
        observed("interrupt")
        raise SystemExit(0)

    if "exec" in sys.argv[1:]:
        prompt = sys.stdin.read()
        if "resume" in sys.argv:
            native = sys.argv[sys.argv.index("resume") + 1]
        else:
            native = str(uuid.uuid4())
        signal.signal(signal.SIGTERM, interrupted)
        observed("exec", native=native, resume="resume" in sys.argv)
        emit({"type": "thread.started", "thread_id": native})
        emit({"type": "turn.started"})
        if "KBD_DENIED" in prompt or "KBD_QUOTA" in prompt:
            code = "insufficient_entitlement" if "KBD_DENIED" in prompt else "insufficient_quota"
            emit({"type": "turn.failed", "error": {"message": code}})
            return
        if "KBD_HOLD" in prompt:
            time.sleep(25)
        for phase, content in [("updated", "fixture"), ("completed", "fixture reply")]:
            emit({"type": "item." + phase, "item": {"id": "item1", "type": "agent_message", "text": content}})
        emit({"type": "turn.completed", "usage": {"input_tokens": 1, "output_tokens": 2, "cached_input_tokens": 0}})
        return
    if "app-server" not in sys.argv:
        raise SystemExit(64)
    for line in sys.stdin:
        req = json.loads(line)
        method = req.get("method")
        if method == "initialize":
            result = {"codexHome": str(home.resolve())}
        elif method == "initialized":
            continue
        elif method == "model/list":
            model = (home / "catalogue").read_text().strip() if (home / "catalogue").exists() else "gpt-5.5"
            result = {"data": [{"id": model, "model": model, "hidden": False, "displayName": "Fixture"}],
                      "nextCursor": None}
        elif method == "account/read":
            result = {"account": {"type": "chatgpt"}, "requiresOpenaiAuth": True}
            if req["params"].get("refreshToken"):
                mode = (home / "exchange-mode").read_text().strip()
                observed("exchange")
                if mode == "interrupted":
                    return
                if mode == "revoked":
                    emit({"id": req["id"], "error": {"code": -32000, "message": "invalid_grant"}})
                    continue
                if mode.startswith("hold"):
                    for _ in range(200):
                        if (home / "release").exists():
                            break
                        time.sleep(.05)
                    else:
                        return
                if mode in ("rotate", "hold-rotate"):
                    (home / "next-generation").replace(home / "auth.json")
        else:
            raise SystemExit(65)
        emit({"id": req["id"], "result": result})


if __name__ == "__main__":
    main()
