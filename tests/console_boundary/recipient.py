"""Loopback-only model and webhook recipient. Retain public canaries and counts."""
import importlib
import json
import re
import sys
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

LOCK = threading.Lock()
MARKER = re.compile(r'\bkbd-[a-z0-9_-]{1,80}', re.I)
SEQUENCE = 0


def payload_markers(value):
    if isinstance(value, str):
        yield from (match.lower() for match in MARKER.findall(value))
    elif isinstance(value, dict):
        for child in value.values():
            yield from payload_markers(child)
    elif isinstance(value, list):
        for child in value:
            yield from payload_markers(child)


def observe(root, fields):
    global SEQUENCE
    with LOCK:
        SEQUENCE += 1
        event = dict(fields, sequence=SEQUENCE, monotonic=time.monotonic())
        with (root / 'upstream-observations.jsonl').open('a') as stream:
            print(json.dumps(event, separators=(',', ':')), file=stream, flush=True)


def optional(name):
    try:
        return importlib.import_module(name)
    except ModuleNotFoundError as exc:
        if exc.name == name:
            return None
        raise


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_POST(self):
        length = int(self.headers.get('Content-Length', '0'))
        if self.path not in ('/hook', '/responses', '/v1/responses', '/v1/chat/completions') or not 0 < length <= 2_000_000:
            self.send_error(404)
            return
        data = json.loads(self.rfile.read(length))
        if not isinstance(data, dict):
            self.send_error(400)
            return
        messages = data.get('messages') or []
        account = self.headers.get('chatgpt-account-id', '')
        if not re.fullmatch(r'(?:owner-|kbd-)[a-zA-Z0-9_-]{1,80}', account):
            account = 'other' if account else None
        fields = dict(path=self.path, account=account,
                      markers=sorted(set(payload_markers(data)))[:128],
                      lastRole=messages[-1].get('role') if messages else None,
                      toolNames=[t.get('function', t).get('name') for t in data.get('tools', []) if isinstance(t, dict)],
                      surfaceAction=any('The user activated a control in this conversation' in str(m.get('content', ''))
                                        for m in messages[-2:]))
        observe(self.server.root, fields)
        if self.path == '/hook':
            self.send_response(204)
            self.end_headers()
            return
        model = data.get('model', 'fixture-chat')
        if self.path.endswith('chat/completions'):
            answer = None
            for name in ('auth_upstream', 'ui_upstream'):
                fixture = optional(name)
                if fixture and hasattr(fixture, 'observe'):
                    fixture.observe(self.server.root, data)
                if fixture and answer is None:
                    answer = fixture.reply(data)
            if answer is None:
                if not data.get('stream'):
                    answer = dict(id='fixture', object='chat.completion', model=model, created=1,
                                  choices=[dict(index=0, message=dict(role='assistant', content='kbd-fixture-reply'), finish_reason='stop')])
                else:
                    answer = [dict(id='fixture', object='chat.completion.chunk', created=1, model=model,
                                   choices=[dict(index=0, delta=delta, finish_reason=finish)])
                              for delta, finish in [(dict(role='assistant', content='kbd-fixture-reply'), None), ({}, 'stop')]]
        else:
            ref = dict(id='resp-fixture', model=model)
            answer = [
                dict(type='response.created', response=ref),
                dict(type='response.output_text.delta', output_index=0, content_index=0, delta='kbd-fixture-reply'),
                dict(type='response.completed', response=dict(ref, status='completed', usage=dict(input_tokens=2, output_tokens=1, total_tokens=3))),
            ]
        if isinstance(answer, dict):
            blob = json.dumps(answer).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.send_header('Content-Length', str(len(blob)))
            self.end_headers()
            self.wfile.write(blob)
            return
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream')
        self.end_headers()
        count = 0
        try:
            for event in answer:
                if "kbd-ui-partial-chat" in fields["markers"] and "error" in event:
                    deadline = time.monotonic() + 20
                    gate = self.server.root / "ui-partial-visible"
                    while not gate.exists() and time.monotonic() < deadline:
                        time.sleep(.05)
                    observe(self.server.root, dict(fields, partialVisibleBeforeError=gate.exists()))
                self.wfile.write(('data: ' + json.dumps(event) + '\n\n').encode())
                self.wfile.flush()
                count += 1
                time.sleep(.02)
            self.wfile.write(b'data: [DONE]\n\n')
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            observe(self.server.root, dict(fields, disconnected=True, deliveredChunks=count))


if __name__ == '__main__':
    listener = ThreadingHTTPServer(('127.0.0.1', int(sys.argv[1])), Handler)
    listener.root = Path(sys.argv[2])
    listener.serve_forever()
