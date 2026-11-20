"""Separate real named client evidence from supplemental ACP/MCP wire probes."""
import contextlib
import json
from pathlib import Path
import re
import tempfile
import shutil
import uuid
import urllib.error
import urllib.request

from admin_common import MissingPeer, case, client, command, http, outputs, private, require, seed, snapshot
from admin_rpc import process
from admin_setup import invalid, portable, req
from admin import TOOLS


MCP_VERSION = "2024-11-05"


def subprocess_env(h):
    env = dict(h.env)
    env.pop("MCP_HTTP_PORT", None)
    env.pop("PROXY_CONSOLE_PRINCIPALS", None)
    return env


def workspace(h):
    return private(h,h.env["PROXY_CONSOLE_WORKSPACE"])


def endpoint(h):
    return "http://127.0.0.1:" + str(int(h.env["MCP_HTTP_PORT"])) + "/mcp"


def mcp_request(h, method, params, authorize=True, ident=1, version=MCP_VERSION):
    body = {"jsonrpc":"2.0","method":method,"params":params}
    if ident is not None:
        body["id"] = ident
    headers = {"Content-Type":"application/json", "Accept":"application/json, text/event-stream",
               "MCP-Protocol-Version":version}
    if authorize:
        headers["Authorization"] = "Bearer " + h.env["PROXY_MCP_TOKEN"]
    request = urllib.request.Request(endpoint(h),data=json.dumps(body).encode(),headers=headers)
    client_http = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        response = client_http.open(request, timeout=8)
    except urllib.error.HTTPError as exc:
        response = exc
    with response:
        data = response.read(262145)
        require(len(data) <= 262144, "MCP HTTP reply exceeded fixture limit")
        secrets = [*h.keys.values(),h.env["PROXY_MCP_TOKEN"]]
        require(not any(s.encode() in data for s in secrets), "MCP HTTP disclosed credentials")
        value = json.loads(data) if data and "json" in response.headers.get("Content-Type","") else {}
        return response.status,value


def init_params(version):
    return {"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"boundary-observer","version":"1"}}


def content(result):
    require(not result.get("isError"), "Client returned a tool error")
    return "".join(item["text"] for item in result["content"] if item.get("type") == "text")


def tool_observations(h, call):
    descriptions = call("tools/list",{})
    require({v["name"] for v in descriptions["tools"]} == TOOLS and len(descriptions["tools"]) == 4,
            "MCP inventory differs from four adopted tools")
    def tool(name,args):
        return content(call("tools/call",{"name":name,"arguments":args}))
    catalogue = tool("list_models",{})
    expected = {x["id"] for x in http(h,"/v1/models")["data"]}
    require(catalogue.startswith("Available models") and any(re.search(r"\b"+re.escape(m)+r"\b",catalogue) for m in expected),
            "MCP returned no model from the separately queried profile catalogue")
    state = tool("check_auth",{})
    require("Authenticated" in state, "Dedicated synthetic owner was not authenticated")
    before = http(h,"/api/v1/me/connection")["defaultModel"]
    advice = tool("set_model",{"task":"quick kbd-mcp-advice"})
    require("kbd-mcp-advice" in advice and "codex-mini" in advice and
            http(h,"/api/v1/me/connection")["defaultModel"] == before, "set_model changed the default or returned unrelated advice")
    return {"tools":sorted(TOOLS),"calls":["list_models","check_auth","set_model"],"defaultUnchanged":True}


def wire_mcp(h, transport):
    if transport == "http":
        def call(method,params):
            status, value = mcp_request(h,method,params)
            require(status == 200 and "result" in value, "MCP HTTP failed "+method)
            return value["result"]
        result = call("initialize",init_params(MCP_VERSION))
        require(result["protocolVersion"] == MCP_VERSION, "MCP HTTP protocol version mismatch")
        status,_ = mcp_request(h,"notifications/initialized",{},ident=None)
        require(status in (202,204), "MCP HTTP refused initialized notification")
        return tool_observations(h,call)
    with process(h,[h.binary,"serve","--mcp-stdio"],workspace(h),subprocess_env(h)) as peer:
        result=peer.call("initialize",init_params(MCP_VERSION))
        require(result["protocolVersion"] == MCP_VERSION, "MCP stdio protocol version mismatch")
        peer.send("notifications/initialized",{})
        return tool_observations(h,peer.call)


def codex_client(h, env, cwd, transport):
    binary = client(h, "codex", cwd, "0.169.0")
    with process(h,[binary,"app-server"],cwd,env) as peer:
        peer.call("initialize",{"clientInfo":{"name":"boundary","title":"Boundary","version":"1"},
                                "capabilities":{"experimentalApi":True}})
        peer.send("initialized",{})
        thread = peer.call("thread/start",{"cwd":str(cwd),"approvalPolicy":"never","sandbox":"read-only","ephemeral":True})
        ident = thread["thread"]["id"]
        entries = peer.call("mcpServerStatus/list",{"threadId":ident,"serverName":"openai-proxy"})["data"]
        server = next((s for s in entries if s["name"] == "openai-proxy"),None)
        require(server and not server.get("toolsError") and {v["name"] for v in server["tools"].values()} == TOOLS,
                "Pinned Codex client did not discover the four tools")
        require(bool(server.get("httpOrigin")) == (transport == "http"), "Codex used a different transport")
        result = peer.call("mcpServer/tool/call",{"threadId":ident,"server":"openai-proxy",
                                                 "tool":"set_model","arguments":{"task":"quick kbd-codex-mcp"}})
        require("kbd-codex-mcp" in content(result) and "codex-mini" in content(result),
                "Codex MCP tool result failed correlation")
        return {"client":"Codex 0.169.0","transport":transport,"tools":sorted(TOOLS),"tool":"set_model"}


@contextlib.contextmanager
def registration(h, program, transport, acp=False):
    cwd = workspace(h)
    client(h, program, cwd, {"opencode":"2.3.0","codex":"0.169.0"}.get(program))
    cli_flag = {"idea":"jetbrains","code":"vscode"}.get(program,program)
    with tempfile.TemporaryDirectory(prefix="client-",dir=h.root) as temp:
        root = Path(temp)
        env = subprocess_env(h)
        for name in ("HOME","XDG_CONFIG_HOME","XDG_DATA_HOME","XDG_CACHE_HOME","CODEX_HOME","TMPDIR"):
            target = root / name.lower()
            target.mkdir(mode=0o700)
            env[name] = str(target)
        # The MCP server's dedicated store remains the synthetic fixture's. Never
        # copy the host's credentials into a real client profile.
        env["CODEX_AUTH_PATH"] = str(private(h, Path(h.env["CODEX_HOME"])/"auth.json"))
        env["PROXY_CONSOLE_WORKSPACE"] = str(root)
        scope = "--global" if program in ("codex","idea") else "--project"
        subcommand = "acp" if acp else "mcp"
        args = [h.binary,"setup",subcommand,"--"+cli_flag,scope]
        if not acp:
            args += ["--port",h.env["MCP_HTTP_PORT"]]
            if transport == "stdio":
                args += ["--stdio"]
        if program == "codex":
            path = Path(env["CODEX_HOME"])/"config.toml"
            source = b'# keep client settings\n[mcp_servers.unrelated]\ncommand = "/usr/bin/false"\nenabled = false\n'
        elif program == "idea":
            path = Path(env["HOME"])/".jetbrains/acp.json"
            source = b'{"agent_servers": {}}\n'
        else:
            path = root / {"zed":".zed/settings.json","opencode":"opencode.jsonc",
                           "code":".vscode/mcp.json","claude":".mcp.json"}[program]
            source = {"zed":b'{"tab_size": 2}\n',"opencode":b'{"plugins": []}\n',
                      "code":b'{"inputs": []}\n',"claude":b'{"mcpServers": {}}\n'}[program]
        with seed(h,path,source):
            original = snapshot(h,path,program+" previous settings")
            result = command(h,args,root,env)
            require(result.returncode == 0, "Installed client setup refused certified profile")
            values=outputs(result)
            require(values[0]["status"] == "ready" and Path(values[0]["path"]).resolve() == path.resolve()
                    and values[-1]["status"] == "applied", "Client configuration was not applied at expected path")
            try:
                altered = snapshot(h,path,"client configured")
                require(altered != original and altered["mode"] == original["mode"], "Incorrect config edit or mode")
                again = command(h,args,root,env)
                require(again.returncode == 0 and not outputs(again)[0]["changes"] and
                        snapshot(h,path,"client reapply") == altered,"Repeated client setup wrote again")
                if program == "opencode":
                    require(json.loads(path.read_text())["mcp"]["servers"]["openai-proxy"], "Missing V2 mcp.servers registration")
                yield root,env,path
            finally:
                undo = command(h,[*args,"--undo",values[-1]["id"]],root,env)
                require(undo.returncode == 0 and snapshot(h,path,"client restored") == original,
                        "Client settings failed to restore byte-for-byte")


def vscode_client(h, cwd, env):
    extension=cwd/"test-extension"
    extension.mkdir(mode=0o700)
    (extension/"package.json").write_text(json.dumps({"name":"proxy-boundary-observer","publisher":"boundary",
        "version":"0.0.1","engines":{"vscode":"^1.99.0"},"main":"./extension.cjs","activationEvents":["onStartupFinished"]}))
    (extension/"extension.cjs").write_text("exports.activate = () => {};")
    source=Path(__file__).with_name("admin_vscode.cjs")
    tests=extension/"observe.cjs"
    shutil.copyfile(source,tests)
    receipt=cwd/"observed.json"
    user=cwd/"code-data"
    (user/"User").mkdir(mode=0o700,parents=True)
    (user/"User/settings.json").write_text(json.dumps({"telemetry.telemetryLevel":"off","update.mode":"none",
        "extensions.autoUpdate":False,"extensions.autoCheckUpdates":False}))
    env.update(BOUNDARY_CODE_RECEIPT=str(receipt),BOUNDARY_CODE_MARKER="kbd-code-"+uuid.uuid4().hex)
    result=command(h,["code","--new-window","--user-data-dir",user,"--extensions-dir",cwd/"extensions",
        "--extensionDevelopmentPath="+str(extension),"--extensionTestsPath="+str(tests),cwd],cwd,env)
    if not receipt.is_file():
        raise MissingPeer("Isolated VS Code extension host or active Copilot/MCP session unavailable")
    observed=json.loads(receipt.read_bytes())
    if observed.get("unavailable"):
        h.note("VS Code isolated profile",observed)
        raise MissingPeer(observed["unavailable"])
    require(result.returncode==0 and set(observed["tools"])==TOOLS and observed["markerMatched"]
            and observed["advisoryMatched"],"Actual editor tool call failed marker or advisory comparison")
    return observed


def named_mcp(h,name,transport):
    with registration(h,name,transport) as (cwd,env,path):
        if name == "codex":
            return codex_client(h,env,cwd,transport)
        if name in ("claude","opencode"):
            import admin_model_peer
            with admin_model_peer.model_peer() as (origin,observed):
                if name=="claude":
                    env.update(ANTHROPIC_BASE_URL=origin,ANTHROPIC_API_KEY="synthetic-local-only",
                               ENABLE_TOOL_SEARCH="false",
                               CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC="1",DISABLE_TELEMETRY="1",
                               CLAUDE_CONFIG_DIR=str(cwd/"claude-profile"))
                    args=[name,"--print","--strict-mcp-config","--mcp-config",str(path),
                          "--output-format","json","--max-turns","3","--model","claude-sonnet-4-6",
                          "--allowedTools","mcp__openai-proxy__set_model","--",
                          "Use the configured proxy's advisory model-selection tool once."]
                else:
                    model={"name":"Fixture","capabilities":{"tools":True,"input":["text"],"output":["text"]},
                           "limit":{"context":32768,"output":1024}}
                    provider={"name":"Fixture","package":"@opencode/ai/providers/openai-compatible",
                              "models":{"boundary-chat":model},"settings":{"baseURL":origin+"/v1","apiKey":"fixture"}}
                    # The registered MCP server stays in the project file. A
                    # disposable global V2 config supplies this client's model
                    # and requires direct tools instead of the Code Mode wrapper.
                    user_config = private(h, Path(env["XDG_CONFIG_HOME"]) / "opencode/opencode.json")
                    user_config.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                    rules = [{"action": "execute", "resource": "*", "effect": "deny"},
                             {"action": "openai-proxy_set_model", "resource": "*", "effect": "allow"}]
                    with user_config.open("x") as output:
                        json.dump({"providers": {"boundary": provider}, "permissions": rules}, output)
                    args=[name,"run","--standalone","--model","boundary/boundary-chat",
                          "Use the configured proxy's advisory model-selection tool once."]
                report=command(h,args,cwd,env)
                h.note(name + " independent MCP model observations", {
                    "requestCount": observed["modelRequests"], "offeredProxyTools": sorted(observed["names"]),
                    "advisoryCallsRequested": observed["requested"], "correlatedResults": observed["completed"]})
                require(report.returncode==0,"The isolated real MCP-client turn failed")
                return dict(admin_model_peer.result(observed),client=name,transport=transport)
        if name=="code":
            return vscode_client(h,cwd,env)
        raise MissingPeer("This GUI client needs an isolated, controllable editor session with an enabled MCP integration")


def acp_wire(h, negative):
    env=subprocess_env(h)
    env["CODEX_WIRE_API"]="responses"
    with process(h,[h.binary,"serve","--acp-stdio"],workspace(h),env) as peer:
        init = peer.call("initialize",{"protocolVersion":1,"clientCapabilities":{}})
        require(init["protocolVersion"] == 1 and not init["agentCapabilities"].get("loadSession"),"ACP advertised unsupported resume")
        created = peer.call("session/new",{"cwd":str(workspace(h)),"mcpServers":[]})
        session = created["sessionId"]
        if negative:
            result = peer.call("session/load",{"sessionId":session,"cwd":str(workspace(h)),"mcpServers":[]},error=True)
            require(isinstance(result["code"],int), "ACP missing-capability failure lacks a protocol error")
            return {"capability":"loadSession","errorCode":result["code"]}
        result = peer.call("session/prompt",{"sessionId":session,"prompt":[{"type":"text","text":"kbd-acp-prompt amberorchard"}]})
        text = "".join(e["params"]["update"].get("content",{}).get("text","") for e in peer.notifications
                       if e.get("method") == "session/update" and e["params"].get("sessionId") == session)
        require(result["stopReason"] == "end_turn" and "kbd-fixture-reply" in text,
                "ACP did not stream a correlated update and terminate the prompt")
        return {"session":session,"streamed":"kbd-fixture-reply","stopReason":result["stopReason"]}


def editor(h, name):
    with registration(h,name,"stdio",acp=True) as (_,__,path):
        entry = json.loads(path.read_text())["agent_servers"]["openai-proxy"]
        require(Path(entry["command"]).resolve() == h.binary.resolve() and entry["args"] == ["serve","--acp-stdio"],
                "Editor registration points to a different binary or argument list")
        raise MissingPeer("Registered editor requires an independently controlled disposable UI session to observe child argv, stream and terminal error")


def absent_editor(h):
    with tempfile.TemporaryDirectory(prefix="absent-editor-",dir=h.root) as temp:
        root=Path(temp)
        path=root/".zed/settings.json"
        with seed(h,path,b'{"tab_size": 7}\n'):
            before=snapshot(h,path,"editor before absent executable")
            result=command(h,[h.binary,"setup","acp","--zed","--project"],root,{**h.env,"PATH":str(root)})
            require(result.returncode != 0 and outputs(result)[0]["status"]=="blocked" and
                    snapshot(h,path,"editor after absent executable")==before, "Missing editor changed settings or reported verified")
    return {"writes":0}


def acp(h,negative):
    case(h,"ACP packaged stdio " + ("unsupported capability" if negative else "stream and session"),lambda: acp_wire(h,negative))
    if negative:
        case(h,"missing editor executable",lambda:absent_editor(h))
        def export():
            value=portable(h,"generic_acp")
            require(value == {"command":"/absolute/path/to/openai-proxy","args":["serve","--acp-stdio"]},
                    "Remote setup export contained an incorrect launch descriptor")
            return {"writes":0}
        case(h,"separate browser host ACP export",export)
    else:
        for name in ("zed","idea"):
            case(h,name+" actual ACP editor prompt and terminal error",lambda: editor(h,name))


def mcp(h,negative):
    if negative:
        def negotiation():
            offered="1900-01-01"
            status,reply=mcp_request(h,"initialize",init_params(offered),version=offered)
            require(status==400 or "error" in reply or reply["result"]["protocolVersion"]!=offered,
                    "An unsupported version was accepted")
            return {"offered":offered,"negotiated":None,"callsAfterMismatch":0}
        case(h,"unsupported protocol negotiation",negotiation)
        def bearer():
            status,reply=mcp_request(h,"tools/call",{"name":"check_auth","arguments":{}},authorize=False)
            require(status in (401,403) and "result" not in reply, "Unauthenticated MCP call disclosed scoped status")
            return {"http":status,"disclosures":0}
        case(h,"protected HTTP rejects unauthenticated account discovery",bearer)
        def listener():
            result=req(h,"preview",{"integration":"opencode_mcp","scope":"project","transport":"http"},actor="admin")
            require(result["status"]=="blocked" and "listener" in result["reason"].lower(),"Missing shared listener presented as available")
            return {"verified":False}
        case(h,"unavailable shared HTTP listener",listener)
        case(h,"mixed OpenCode MCP file survives refused setup",
             lambda:invalid(h,"project",b'{"mcp":{"servers":{}, "legacy":{}}}',"legacy","opencode_mcp"))
        return
    for transport in ("stdio","http"):
        case(h,"standalone MCP "+transport+" negotiation and scoped calls",lambda:wire_mcp(h,transport))
    for name,transport in [("codex","stdio"),("codex","http"),("claude","stdio"),("opencode","http"),("zed","stdio"),("code","http")]:
        case(h,name+" real MCP client "+transport,lambda:named_mcp(h,name,transport))
