"""Deterministic model collaborator for permission-controlled reads and interruption."""
import json
import re
import threading

GUARD = threading.Lock()


def latest(payload):
    messages = payload.get("messages", [])
    users = [i for i, msg in enumerate(messages) if msg.get("role") == "user"]
    if not users:
        return None, []
    index = users[-1]
    match = re.search(r"\bkbd-(?:chat|tool)-(?:answer|hold|read|delegate)-[a-f0-9]{12}\b",
                      json.dumps(messages[index].get("content")))
    return (match.group() if match else None), messages[index + 1:]


def observe(root, payload):
    marker, later = latest(payload)
    if marker is None:
        return
    replies = [m for m in later if m.get("role") == "tool"]
    value = {"marker": marker, "tool_replies": len(replies),
             "canary_returns": sum(("value-" + marker) in json.dumps(m.get("content")) for m in replies),
             "delegated_returns": sum("fixture reply" in json.dumps(m.get("content")) for m in replies)}
    with GUARD, (root / "native-model-observer.jsonl").open("a") as journal:
        print(json.dumps(value), file=journal)


def reply(payload):
    marker, later = latest(payload)
    if marker is None:
        return None
    tool = None
    if not any(m.get("role") == "tool" for m in later):
        if "-tool-read-" in marker:
            tool = ("workspace_read", {"path": marker + ".txt"})
        if "-tool-delegate-" in marker:
            tool = ("delegate_codex", {"prompt": "fixture delegate " + marker})
    base = {"id": "chatcmpl-" + marker, "created": 1, "model": payload.get("model", "fixture-chat")}
    content = "reply-" + marker
    call = ({"id": "call_" + marker, "type": "function",
             "function": {"name": tool[0], "arguments": json.dumps(tool[1])}} if tool else None)
    finish = "tool_calls" if tool else "stop"
    if not payload.get("stream"):
        message = {"role": "assistant", "content": None if tool else content}
        if call:
            message["tool_calls"] = [call]
        return dict(base, object="chat.completion", choices=[dict(index=0, message=message, finish_reason=finish)])

    def chunk(delta, reason=None):
        return dict(base, object="chat.completion.chunk",
                    choices=[dict(index=0, delta=delta, finish_reason=reason)])

    result = [chunk({"role": "assistant"})]
    if call:
        result.append(chunk({"tool_calls": [dict(call, index=0)]}))
    else:
        result.append(chunk({"content": content}))
        if "-chat-hold-" in marker:
            result.extend(chunk({"content": "."}) for _ in range(650))
    result.append(chunk({}, finish))
    return result
