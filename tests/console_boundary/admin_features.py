"""Independent file, recipient, hook and database observations for FEATURE-1."""
import contextlib
import json
from pathlib import Path
import subprocess
import tempfile
import time
from urllib.parse import quote
import uuid

import runtime
from admin import inventory
from admin_common import MissingPeer, case, http, private, require


DOCS = "/v1/memory/documents"
SEARCH = "/v1/memory/search?q=orchard"
CARD = "/.well-known/agent.json"


def trace(h):
    path = h.root / "upstream-observations.jsonl"
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line]


def direct(h, message, **options):
    return http(h,"/v1/chat/completions","POST",{"model":"gpt-5.5","stream":False,
                "messages":[{"role":"user","content":message}]},**options)


def installed_features(h):
    settings = inventory(h)
    events = trace(h)
    for trigger, selected, ignored in [("amberorchard","amber","blue"),("blueorchard","blue","amber")]:
        marker="kbd-feature-"+uuid.uuid4().hex
        start=len(trace(h))
        value=direct(h,marker+" "+trigger)
        require("kbd-fixture-reply" in json.dumps(value), "Direct packaged endpoint did not return fixture text")
        deadline=time.monotonic()+5
        observed=[]
        while time.monotonic()<deadline:
            observed=trace(h)[start:]
            if any(row["path"]=="/hook" for row in observed):
                break
            time.sleep(.05)
        model=[row for row in observed if marker in row["markers"] and row["path"]!="/hook"]
        hooks=[row for row in observed if row["path"]=="/hook"]
        require(len(model)==1 and len(hooks)==1, "Independent recipient did not see one model call and one hook")
        require("kbd-skill-"+selected in model[0]["markers"] and "kbd-skill-"+ignored not in model[0]["markers"],
                "Recipient saw wrong skill, multiple skills, or empty skill injection")
        require(model[0]["toolNames"].count("boundary_schema")==1 and
                not settings["tools"]["value"]["executor"], "Request lacked the fixture schema or reported an executor")
        require(marker not in hooks[0]["markers"], "Lifecycle count hook included conversation text")
        h.note(trigger+" recipient",{"modelSequence":model[0]["sequence"],"hookSequence":hooks[0]["sequence"],
                                     "skill":"kbd-skill-"+selected,"schemaCount":1})
    card=http(h,CARD)
    require(card["url"]==h.base and {s["id"] for s in card["skills"]}=={"chat_completion","list_models"}
            and settings["a2a"]["value"]["enabled"]["effective"],"Agent Card differs from reachable fixture service")
    return {"requests":2,"observedHooks":sum(r["path"]=="/hook" for r in trace(h)[len(events):]),"card":card["url"]}


def mem_h(h):
    if not getattr(h,"memory_base",None):
        raise MissingPeer("Isolated shared memory-enabled service missing")
    return h.memory_base


@contextlib.contextmanager
def owned_document(h, actor, base, text, scope):
    result=http(h,DOCS,"POST",{"scope":scope,"text":text},actor=actor,base=base)
    ident=str(uuid.UUID(result["id"]))
    try:
        yield ident
    finally:
        http(h,DOCS+"/"+ident,"DELETE",actor=actor,base=base,statuses=(204,))
        rows=http(h,DOCS+"?scope="+quote(scope),actor=actor,base=base)
        require(all(r["id"]!=ident for r in rows),"Cleanup left the suite's document in the store")


def shared_docs(h,negative):
    base=mem_h(h)
    value=inventory(h,"admin",base=base)["memory"]
    require(value["availability"]=="ready" and value["value"]["scope"]=="assigned by authenticated principal"
            and value["value"]["embeddings"]=="disabled; local document retrieval",
            "Shared DB status, scoped retrieval or API billing descriptor differs")
    tags={actor:"kbd-"+actor+"-"+uuid.uuid4().hex for actor in ("chat","chat2")}
    with contextlib.ExitStack() as cleanup:
        ids={actor:cleanup.enter_context(owned_document(h,actor,base,"orchard "+tag,"fixture"))
             for actor,tag in tags.items()}
        for actor in tags:
            other=next(name for name in tags if name!=actor)
            params="&scope="+other if negative else ""
            listed=http(h,DOCS+("?scope="+other if negative else ""),actor=actor,base=base)
            searched=http(h,SEARCH+params,actor=actor,base=base)
            for response in (listed,searched):
                require(any(v["id"]==ids[actor] and tags[actor] in v["text"] for v in response)
                        and all(v["id"]!=ids[other] and tags[other] not in v["text"] for v in response),
                        "Scoped store returned wrong document inventory/retrieval")
            if negative:
                http(h,DOCS+"/"+ids[other],"DELETE",actor=actor,base=base,statuses=(204,403,404))
                owner=http(h,DOCS,actor=other,base=base)
                require(any(v["id"]==ids[other] and tags[other] in v["text"] for v in owner),
                        "Scoped delete removed a foreign fixture document")
    return {"actors":2,"ownedCreates":2,"ownDeletes":2,"foreignReads":0,"foreignDeletes":0}


@contextlib.contextmanager
def variant(h, enabled, degraded=False):
    if not h.memory_binary.is_file():
        raise MissingPeer("Packaged memory-feature binary required for this subcase")
    with tempfile.TemporaryDirectory(prefix="feature-service-",dir=h.root) as directory:
        root=Path(directory)
        cfg=root/"config.toml"
        db=root/"database"
        if degraded:
            # Independent file where a directory is required makes DB unavailable.
            db.write_bytes(b"owned fixture occupying a directory component")
            db=db/"memory"
        cfg.write_text("[memory]\nenabled = "+str(enabled).lower()+"\ndb_path = "+json.dumps(str(db))+"\n")
        env=dict(h.env)
        for key in ("PROXY_CONSOLE_PRINCIPALS","MCP_HTTP_PORT","PROXY_HOOKS_CONFIG","PROXY_HOOK_DESTINATIONS","PROXY_EMBEDDINGS_API_KEY"):
            env.pop(key,None)
        port=runtime.port()
        base=runtime.origin(port)
        for key in ("PROXY_CONSOLE_DATA","PROXY_CONSOLE_WORKSPACE"):
            path=root/key
            path.mkdir(mode=0o700)
            env[key]=str(path)
        env.update(OPROXY_CONFIG=str(cfg),PROXY_CONSOLE_ORIGIN=base,PORT=str(port))
        private(h,env["CODEX_HOME"])
        proc=subprocess.Popen([h.memory_binary,"serve"],env=env,cwd=root,stdin=subprocess.DEVNULL,
                              stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
        try:
            runtime.wait_http(base,proc)
            h.note("memory variant "+str((enabled,degraded)),{"pid":proc.pid,"enabled":enabled,"degraded":degraded,
                         "port":port,"binary":"openai-proxy-memory","hooks":False,"embeddingsCredentialProvided":False})
            yield base
        finally:
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=4)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait(timeout=2)


def local_rag(h):
    with variant(h,True) as base:
        settings=inventory(h,base=base)["memory"]
        require(settings["availability"]=="ready" and settings["value"]["compiled"] and
                settings["value"]["embeddings"]=="disabled; local document retrieval", "Local memory or billing differs")
        tag="kbd-rag-"+uuid.uuid4().hex
        marker="kbd-prompt-"+uuid.uuid4().hex
        with owned_document(h,"local",base,"orchard "+tag,"session"):
            start=len(trace(h))
            direct(h,marker+" orchard",base=base)
            model=[r for r in trace(h)[start:] if marker in r["markers"] and r["path"]!="/hook"]
            require(len(model)==1 and tag in model[0]["markers"],"Upstream received no owned retrieval context")
            require(not any(r["path"]=="/hook" for r in trace(h)[start:]),"Disabled hook delivered a request")
        return {"ownedMarker":tag,"recipientSequence":model[0]["sequence"],"hookDeliveries":0,
                "documentCleanup":True,"embeddingsCredentialProvided":False}


def disabled(h,degraded):
    with variant(h,degraded,degraded) as base:
        cfg=inventory(h,base=base)
        expected="database_unavailable" if degraded else "disabled"
        require(cfg["memory"]["availability"]==expected and cfg["webhooks"]["availability"]=="unconfigured",
                "Settings did not surface disabled or degraded database/hooks")
        http(h,DOCS,base=base,statuses=(503,))
        http(h,CARD,base=base,statuses=(404,))
        start=len(trace(h))
        marker="kbd-disabled-"+uuid.uuid4().hex
        direct(h,marker,base=base)
        time.sleep(.3)
        rows=trace(h)[start:]
        require(len([r for r in rows if marker in r["markers"]])==1 and all(r["path"]!="/hook" for r in rows),
                "Independent observer could not distinguish disabled hook from a missing model call")
        http(h,"/api/v1/admin/config/preview","POST",{"field":"hooks.destination","value":"unregistered-fixture"},
             base=base,statuses=(400,))
        require(cfg["webhooks"]==inventory(h,base=base)["webhooks"],"Unapproved hook alias changed effective settings")
    return {"database":expected,"hookDeliveries":0,"modelCalls":1}


def denied_injection(h):
    base=mem_h(h)
    with owned_document(h,"chat2",base,"orchard kbd-foreign-doc","fixture"):
        marker="kbd-isolated-"+uuid.uuid4().hex
        start=len(trace(h))
        reply=direct(h,marker+" orchard",actor="chat",base=base,headers={"X-Memory-Scope":"chat2"},statuses=(401,403,500,503))
        require(isinstance(reply,dict) and reply.get("error",{}).get("type") in ("auth_unavailable","unauthorized","authentication_error"),
                "Shared direct request failed for an unrelated reason")
        require(not any(marker in r["markers"] or "kbd-foreign-doc" in r["markers"] for r in trace(h)[start:]),
                "Signed-out client reached the fixture recipient")
    return {"deniedCalls":1,"outboundCalls":0}


def run(h,negative):
    if negative:
        case(h,"two-user scoped list, search and delete preservation",lambda:shared_docs(h,True))
        case(h,"signed-out request cannot inject a foreign document",lambda:denied_injection(h))
        for broken in (False,True):
            case(h,("degraded" if broken else "disabled")+" DB, disabled hooks and unregistered egress",lambda:disabled(h,broken))
    else:
        case(h,"selected skills, imported schemas, hooks and agent card",lambda:installed_features(h))
        case(h,"two-principal database retrieval and cleanup",lambda:shared_docs(h,False))
        case(h,"actual local packaged memory retrieval, injection and billing configuration",lambda:local_rag(h))
    h.block("authenticated shared upstream memory injection",
            "Dedicated shared profiles have no live grants or independently routable shared direct-backend recipient")
