"""Deterministic loopback OpenAI-compatible replies used by the packaged native worker."""
import copy
import hashlib
import json
import re

CATALOG = "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
NAME = "Confirm fixture selection"
DESCRIPTION = "Records one synthetic choice in this conversation."
SURFACE = "kbd-choice"
TEMP = "kbd-transient"
BAD = ("old", "wrong", "catalog", "reserved", "rpc", "oversized", "partial")
ACTION_ACK = "Fixture receiver accepted the selected item."
PLAIN = "Plain assistant output survives."
PARTIAL = "Keep this partial reply."
STOP = "The native fixture stream has started."


def op(kind, body):
    return {"version": "1.0", kind: body}


def create(sid, components, data):
    return op("createSurface", {"surfaceId": sid, "catalogId": CATALOG,
                                "components": components, "dataModel": data})


def snapshots(marker):
    button = {"id": "root", "component": "Button", "child": "caption",
              "@accessibility": {"label": NAME, "description": DESCRIPTION},
              "action": {"event": {"name": "choose", "context": {"item": {"@path": "/@item"}}}}}
    components = [button, {"id": "caption", "component": "Text", "text": {"@path": "/@caption"}}]
    start = [create(SURFACE, components, {"@item": marker, "@caption": "Initial choice"}),
             create(TEMP, [dict(button, **{"@accessibility": {"label": "Transient selection"}}),
                           {"id": "caption", "component": "Text", "text": "Temporary choice"}], {"@item": marker})]
    updated = start + [op("updateDataModel", {"surfaceId": SURFACE,
                                              "path": "/@caption", "value": "Final choice"})]
    return [start, updated, copy.deepcopy(updated), updated + [op("deleteSurface", {"surfaceId": TEMP})]]


def bad_arguments(kind):
    fixture = snapshots("kbd-irrelevant")[0][:1]
    body = fixture[0]["createSurface"]
    body["surfaceId"] = "kbd-rejected"
    body["components"][0]["@accessibility"]["label"] = "Untrusted fixture control"
    if kind == "old":
        fixture[0]["version"] = "0.9"
    elif kind == "wrong":
        fixture[0]["version"] = "v1.0"
    elif kind == "catalog":
        body["catalogId"] = "urn:kbd:unknown"
    elif kind == "reserved":
        body["components"][0]["@unknown"] = "unregistered"
    elif kind == "rpc":
        fixture.append(op("callRendererFunction", {"surfaceId": "kbd-rejected",
                          "functionCallId": "fixture", "callFunction": {"name": "unregistered"}}))
    elif kind == "oversized":
        body["dataModel"]["long"] = "x" * 34000
    raw = json.dumps({"a2ui_operations": fixture}, separators=(",", ":"))
    return raw[:-5] if kind == "partial" else raw


def completion(payload, text="", arguments=(), parts=None, fail=False):
    model = payload.get("model", "fixture")
    seed = hashlib.sha256(json.dumps(payload.get("messages", [])).encode()).hexdigest()[:14]
    calls = [{"index": i, "id": f"call_kbd_{seed}_{i}", "type": "function",
              "function": {"name": "render_surface", "arguments": a}}
             for i, a in enumerate(arguments)]
    base = {"id": "chatcmpl-kbd-fixture", "created": 1, "model": model}
    stop = "tool_calls" if calls else "stop"
    if not payload.get("stream"):
        msg = {"role": "assistant", "content": text}
        if calls:
            msg["tool_calls"] = [{k: v for k, v in c.items() if k != "index"} for c in calls]
        return {**base, "object": "chat.completion", "choices": [
            {"index": 0, "message": msg, "finish_reason": stop}]}

    def chunk(delta, finish=None):
        return {**base, "object": "chat.completion.chunk", "choices": [
            {"index": 0, "delta": delta, "finish_reason": finish}]}

    result = [chunk({"role": "assistant"})]
    for value in parts if parts is not None else [text]:
        if value:
            result.append(chunk({"content": value}))
    if calls:
        result.append(chunk({"tool_calls": calls}))
    if fail:
        # The installed OpenCode provider classifies numeric 400 as InvalidRequest.
        # Unknown string codes ask it to retry and never prove a terminal partial failure.
        result.append({"error": {"message": "Synthetic reply rejected after partial output",
                                 "type": "invalid_request_error", "code": 400}})
    else:
        result.append(chunk({}, stop))
    return result


def reply(payload):
    messages = payload.get("messages", [])
    users = [i for i, m in enumerate(messages) if m.get("role") == "user"]
    if not users:
        return None
    pos = users[-1]
    prompt = json.dumps(messages[pos].get("content", ""))
    if "The user activated a control in this conversation" in prompt and "kbd-surface-" in prompt:
        return completion(payload, ACTION_ACK)
    match = re.search(r"kbd-(surface|ui)-[a-z0-9_-]+", prompt)
    if not match:
        return None
    marker = match.group()
    if any(m.get("role") == "tool" for m in messages[pos + 1:]):
        return completion(payload, PLAIN)
    if marker.startswith("kbd-ui-stop"):
        return completion(payload, parts=[STOP] + [" ."] * 500)
    if marker.startswith("kbd-ui-partial"):
        return completion(payload, parts=[PARTIAL] + [" "] * 12, fail=True)
    if marker == "kbd-surface-text":
        return completion(payload, json.dumps({"type": "ACTIVITY_SNAPSHOT", "activityType": "a2ui-surface",
                          "content": {"a2ui_operations": json.loads(bad_arguments("valid"))["a2ui_operations"]}}))
    if marker.startswith("kbd-surface-guard-"):
        safe = create("kbd-guard", [{"id": "root", "component": "Text", "text": "Preserved sibling surface"}], {})
        return completion(payload, arguments=[json.dumps({"a2ui_operations": [safe]})])
    if marker.startswith("kbd-surface-bad-"):
        return completion(payload, arguments=[bad_arguments(marker.removeprefix("kbd-surface-bad-"))])
    if marker.startswith("kbd-surface-"):
        return completion(payload, arguments=[json.dumps({"a2ui_operations": ops}, separators=(",", ":"))
                                             for ops in snapshots(marker)])
    return None
