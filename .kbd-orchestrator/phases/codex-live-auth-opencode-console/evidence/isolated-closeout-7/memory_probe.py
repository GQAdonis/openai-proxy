"""Exercise shared memory and embedding isolation with a private synthetic recipient."""
import argparse
import contextlib
import hashlib
import json
from pathlib import Path
import secrets
import ssl
import subprocess
import sys
import tempfile
import threading
import urllib.error
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, str(Path.cwd() / "tests/console_boundary"))
import recipient
import runtime

class Observer(recipient.Handler):
    def do_POST(self):
        role = self.server.roles.get(self.headers.get("Authorization"), "unknown")
        if role == "unknown":
            self.send_error(403)
            return
        self.server.calls.append((self.path, role))
        if self.path == "/v1/embeddings":
            self.rfile.read(int(self.headers["Content-Length"]))
            body = b'{"data":[{"embedding":[]}]}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        elif self.path == "/v1/responses":
            request_body=json.loads(self.rfile.read(int(self.headers["Content-Length"])))
            recipient.observe(self.server.root, {"path":self.path,"markers":sorted(set(recipient.payload_markers(request_body)))})
            ref={"id":"resp-fixture","model":request_body["model"]}
            events=[{"type":"response.created","response":ref},
                    {"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"kbd-fixture-reply"},
                    {"type":"response.completed","response":dict(ref,status="completed",usage={"input_tokens":2,"output_tokens":1,"total_tokens":3})}]
            body=("".join("data: "+json.dumps(event)+"\n\n" for event in events)+"data: [DONE]\n\n").encode()
            self.send_response(200)
            self.send_header("Content-Type","text/event-stream")
            self.send_header("Content-Length",str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            self.send_error(404)

class PrivateEndpoint(BaseHTTPRequestHandler):
    """Answer one fixed test authority in-process. This fixture has no upstream connector."""
    def log_message(self, *_):
        pass

    def do_CONNECT(self):
        if self.path != "api.openai.com:443":
            self.send_error(403)
            return
        self.send_response_only(200)
        self.end_headers()
        self.wfile.flush()
        with self.server.tls_context.wrap_socket(self.connection, server_side=True) as sock:
            Observer(sock, self.client_address, self.server)

def request(base, path, auth, data=None, *, method=None, scope=None):
    headers = {"Content-Type":"application/json", "Authorization":"Bearer "+auth}
    if scope:
        headers["X-Memory-Scope"] = scope
    req = urllib.request.Request(base+path, data=json.dumps(data).encode() if data is not None else None,
                                 headers=headers, method=method)
    try:
        with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req, timeout=8) as resp:
            body = resp.read()
            return resp.status, json.loads(body) if body else None
    except urllib.error.HTTPError as err:
        return err.code, json.loads(err.read() or b"null")

def authfile(path, key):
    path.mkdir(exist_ok=True, mode=0o700)
    (path/"auth.json").write_text(json.dumps({"auth_mode":"apikey","OPENAI_API_KEY":key}))

@contextlib.contextmanager
def service(binary, directory, shared, embeddings, peer, ca, keys):
    directory.mkdir(mode=0o700)
    env = runtime.sanitized(directory)
    authfile(Path(env["CODEX_HOME"]),keys["solo"])
    cfg = directory/"memory.toml"
    cfg.write_text('[memory]\nenabled = true\ndb_path = '+json.dumps(str(directory/"db"))+'\n')
    port = runtime.port()
    base = f"http://127.0.0.1:{port}"
    state = directory/"state"
    state.mkdir(mode=0o700)
    workspace=directory/"workspace"
    workspace.mkdir(mode=0o700)
    env.update(OPROXY_CONFIG=str(cfg), HOST="127.0.0.1", PORT=str(port), PROXY_CHAT_AUTH="codex",
               PROXY_CONSOLE_ORIGIN=base, PROXY_CONSOLE_DATA=str(state), PROXY_CONSOLE_WORKSPACE=str(workspace),
               SSL_CERT_FILE=str(ca), HTTPS_PROXY=f"http://127.0.0.1:{peer.server_port}", NO_PROXY="127.0.0.1,localhost")
    tokens = {}
    if embeddings:
        env["PROXY_EMBEDDINGS_API_KEY"] = keys["embedding"]
    if shared:
        entries=[]
        for actor in ("first","second"):
            home = directory/(actor+"-home")
            space = directory/(actor+"-space")
            space.mkdir(mode=0o700)
            authfile(home,keys[actor])
            tokens[actor] = secrets.token_hex(24)
            entries.append(dict(id=actor,role="chat",tokenSha256=hashlib.sha256(tokens[actor].encode()).hexdigest(),
                                workspace=str(space),codexHome=str(home)))
        path=directory/"principals.json"
        path.write_text(json.dumps(entries))
        env["PROXY_CONSOLE_PRINCIPALS"]=str(path)
    with (directory/"service.log").open("w") as log:
        policy='(version 1)(allow default)(deny network-outbound)(allow network-outbound (remote ip "localhost:*"))'
        argv=["/usr/bin/sandbox-exec","-p",policy,str(binary),"serve"]
        proc=subprocess.Popen(argv,cwd=directory,env=env,stdin=subprocess.DEVNULL,
                              stdout=log,stderr=subprocess.STDOUT)
        try:
            runtime.wait_http(base,proc)
            if not shared: tokens["solo"]=(state/"service-token").read_text().strip()
            yield base,tokens
        finally:
            if proc.poll() is None:
                proc.terminate()
                try: proc.wait(timeout=4)
                except subprocess.TimeoutExpired: proc.kill();proc.wait()

def exercise(binary, output, openssl):
    result={"feature":"NOT_EXERCISED","embedding":"NOT_EXERCISED","execution":"frozen production source; memory,reqwest/rustls-tls-native-roots","binarySha256":hashlib.sha256(binary.read_bytes()).hexdigest()}
    with tempfile.TemporaryDirectory(prefix="kbd-owned-memory-") as temp:
        root=Path(temp)
        ca=root/"peer.pem"
        key=root/"peer.key"
        subprocess.run([openssl,"req","-x509","-newkey","rsa:2048","-noenc","-days","1","-subj","/CN=fixture",
                        "-addext","subjectAltName=DNS:api.openai.com","-addext","basicConstraints=critical,CA:FALSE",
                        "-keyout",str(key),"-out",str(ca)],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
        peer=ThreadingHTTPServer(("127.0.0.1",0), PrivateEndpoint)
        ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.load_cert_chain(ca,key)
        peer.tls_context=ctx
        keys={actor:secrets.token_hex(24) for actor in ("solo","embedding","first","second")}
        peer.root=root
        peer.calls=[]
        peer.roles={"Bearer "+v:k for k,v in keys.items()}
        thread=threading.Thread(target=peer.serve_forever,daemon=True);thread.start()
        try:
            docs="/v1/memory/documents"
            with service(binary,root/"shared",True,True,peer,ca,keys) as (base,tokens):
                ids={}
                for actor in tokens:
                    status,row=request(base,docs,tokens[actor],{"text":"orchard kbd-doc-"+actor,"scope":"session"})
                    assert status in (200,201), ("create",actor,status)
                    ids[actor]=row["id"]
                observations=[]
                for actor,other in (("first","second"),("second","first")):
                    query={"model":"gpt-5.5","stream":False,"messages":[{"role":"user","content":"orchard"}]}
                    status,reply=request(base,"/v1/chat/completions",tokens[actor],query,scope=other)
                    assert status==200 and "kbd-fixture-reply" in json.dumps(reply),("chat",actor,status,peer.calls,reply.get("error",{}).get("type"))
                    rows=[json.loads(x) for x in (root/"upstream-observations.jsonl").read_text().splitlines()]
                    observation=rows[-1]
                    assert "kbd-doc-"+actor in observation["markers"] and "kbd-doc-"+other not in observation["markers"],("injection",actor,observation["markers"])
                    assert peer.calls[-1]==("/v1/responses",actor)
                    observations.append(dict(actor=actor,ownMarker=True,foreignMarker=False,spoofedScope=other))
                before=len(peer.calls)
                denied,_=request(base,"/v1/chat/completions",secrets.token_hex(24),query)
                assert denied in (401,403) and len(peer.calls)==before
                for actor,ident in ids.items():
                    status,_=request(base,docs+"/"+ident,tokens[actor],method="DELETE")
                    assert status==204
                assert len(peer.calls)==2
                result.update(feature="PASS",shared=observations,unauthorizedStatus=denied,
                              sharedEmbeddingCalls=0,sharedModelCalls=2)
            embed=[]
            for enabled in (False,True):
                start=len(peer.calls)
                with service(binary,root/("enabled" if enabled else "disabled"),False,enabled,peer,ca,keys) as (base,tokens):
                    status,_=request(base,docs,tokens["solo"],{"text":"orchard synthetic","scope":"session"})
                    assert status in (201,200),("embedding doc",status)
                    seen=peer.calls[start:]
                    expected=[("/v1/embeddings","embedding")] if enabled else []
                    assert seen==expected,("embedding recipient",enabled,seen)
                    embed.append({"explicitEmbeddingCredential":enabled,"recipientCalls":len(seen),"chatCredentialCalls":0})
            result.update(embedding="PASS",embeddingCases=embed)
        finally:
            peer.shutdown();peer.server_close();thread.join(timeout=2)
    result["privateRunRootRemoved"]=not root.exists()
    output.write_text(json.dumps(result,indent=2)+"\n")

if __name__=="__main__":
    parser=argparse.ArgumentParser()
    parser.add_argument("--binary",type=Path,required=True)
    parser.add_argument("--output",type=Path,required=True)
    parser.add_argument("--openssl",default="openssl")
    args=parser.parse_args()
    exercise(args.binary,args.output,args.openssl)
