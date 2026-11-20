"""Deterministic local model peer for one real MCP-client turn.

It can ask only for set_model with a nonsecret, random task marker. The expected
answer is read back from the real client's subsequent tool-result message.
"""
import contextlib
import json
import threading
import uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from admin_common import require
from admin import TOOLS


@contextlib.contextmanager
def model_peer():
    marker = "kbd-client-" + uuid.uuid4().hex
    observations = {"marker":marker,"names":set(),"requested":0,"completed":0}
    lock = threading.Lock()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self,*_):
            pass

        def do_POST(self):
            size=int(self.headers.get("Content-Length",0))
            if not (0<size<=4_000_000):
                self.send_error(400)
                return
            if self.path.split("?")[0] not in ("/v1/messages","/v1/chat/completions"):
                self.send_error(404)
                return
            data=json.loads(self.rfile.read(size))
            anthropic=self.path.startswith("/v1/messages")
            tools=[item.get("function",item)["name"] for item in data.get("tools",[])]
            public={name for name in TOOLS if any(t.endswith(name) and "proxy" in t for t in tools)}
            selected=next((t for t in tools if t.endswith("set_model") and "proxy" in t),None)
            answered=False
            for message in data.get("messages",[]):
                if message.get("role")=="tool":
                    results=[message.get("content")]
                elif isinstance(message.get("content"),list):
                    results=[b.get("content") for b in message["content"] if b.get("type")=="tool_result"]
                else:
                    results=[]
                if any(marker in json.dumps(v) and "codex-mini" in json.dumps(v) for v in results):
                    answered=True
            with lock:
                observations["names"].update(public)
                ask=bool(selected) and not answered and observations["requested"]==0
                if ask:
                    observations["requested"]+=1
                if answered:
                    observations["completed"]+=1
            args={"task":"quick "+marker}
            model=data.get("model","fixture")
            final_text="correlated MCP advice received" if answered else "MCP tool result unavailable"
            frames=[]
            if anthropic:
                block=({"type":"tool_use","id":"toolu_boundary","name":selected,"input":args} if ask else
                       {"type":"text","text":final_text})
                response={"id":"msg_boundary","type":"message","role":"assistant","model":model,"content":[block],
                          "stop_reason":"tool_use" if ask else "end_turn","stop_sequence":None,
                          "usage":{"input_tokens":5,"output_tokens":4}}
                starting=dict(response,content=[],stop_reason=None)
                start_block=dict(block,input={}) if ask else {"type":"text","text":""}
                delta={"type":"input_json_delta","partial_json":json.dumps(args)} if ask else {"type":"text_delta","text":final_text}
                for kind,rest in [("message_start",{"message":starting}),
                                  ("content_block_start",{"index":0,"content_block":start_block}),
                                  ("content_block_delta",{"index":0,"delta":delta}),
                                  ("content_block_stop",{"index":0}),
                                  ("message_delta",{"delta":{"stop_reason":response["stop_reason"],"stop_sequence":None},
                                                    "usage":{"output_tokens":4}}),("message_stop",{})]:
                    frames.append("event: "+kind+"\ndata: "+json.dumps(dict(type=kind,**rest))+"\n\n")
            else:
                tool={"id":"call_boundary","type":"function","function":{"name":selected,"arguments":json.dumps(args)}}
                message={"role":"assistant","content":None,"tool_calls":[tool]} if ask else {"role":"assistant","content":final_text}
                stop="tool_calls" if ask else "stop"
                response={"id":"chatcmpl_boundary","object":"chat.completion","model":model,"created":1,
                          "choices":[{"index":0,"message":message,"finish_reason":stop}]}
                change={"tool_calls":[dict(tool,index=0)]} if ask else {"content":final_text}
                for delta,reason in [({"role":"assistant"},None),(change,None),({},stop)]:
                    frames.append("data: "+json.dumps(dict(response,object="chat.completion.chunk",
                                  choices=[{"index":0,"delta":delta,"finish_reason":reason}]))+"\n\n")
                frames.append("data: [DONE]\n\n")
            streaming=data.get("stream")
            blob=("".join(frames) if streaming else json.dumps(response)).encode()
            self.send_response(200)
            self.send_header("Content-Type","text/event-stream" if streaming else "application/json")
            self.send_header("Content-Length",str(len(blob)))
            self.end_headers()
            self.wfile.write(blob)

    server=ThreadingHTTPServer(("127.0.0.1",0),Handler)
    thread=threading.Thread(target=server.serve_forever,daemon=True)
    thread.start()
    try:
        yield "http://127.0.0.1:"+str(server.server_port),observations
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=3)


def result(observed):
    require(observed["names"]==TOOLS, "Real client did not offer exactly the four proxy tools to its model")
    require(observed["requested"]==1 and observed["completed"]==1,
            "Real client did not return a correlated advisory tool result to the independent model peer")
    return {"tools":sorted(observed["names"]),"nativeToolCalls":observed["requested"],
            "nativeToolResults":observed["completed"],"marker":observed["marker"]}
