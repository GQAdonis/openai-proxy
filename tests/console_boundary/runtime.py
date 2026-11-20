"""Start the installed artifacts in isolated profiles for the approved boundary."""
import hashlib
import json
import os
import secrets
import shutil
import signal
import socket
import socketserver
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from pathlib import Path


def port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


def directory(path):
    path.mkdir(parents=True, exist_ok=True, mode=0o700)
    return path


def origin(number):
    return 'http://127.0.0.1:' + str(number)


def write(path, value):
    directory(path.parent)
    tmp = path.with_name(path.name + '.tmp')
    tmp.write_text(json.dumps(value, indent=2) + '\n')
    tmp.replace(path)


def sanitized(root):
    home = directory(root / 'home')
    env = {'PATH': os.pathsep.join(d for d in os.get_exec_path() if os.path.isabs(d)),
           'HOME': str(home), 'LANG': 'C', 'NO_COLOR': '1', 'CI': '1',
           'XDG_CONFIG_HOME': str(directory(home / 'config')), 'XDG_DATA_HOME': str(directory(home / 'data')),
           'XDG_CACHE_HOME': str(directory(home / 'cache')), 'TMPDIR': str(directory(root / 'tmp')),
           'CODEX_HOME': str(directory(root / 'codex')), 'GIT_CONFIG_NOSYSTEM': '1',
           'GIT_CONFIG_GLOBAL': os.devnull, 'RUST_LOG': 'warn', 'DO_NOT_TRACK': '1'}
    return env


def docker_env(env):
    result = dict(env)
    # Use the installed local engine. Do not select remote contexts or credentials.
    for path in (Path('/var/run/docker.sock'), Path.home() / '.docker/run/docker.sock'):
        if path.is_socket():
            result['DOCKER_HOST'] = 'unix://' + str(path)
            break
    return result


def ps_stamp(pid):
    result = subprocess.run(['ps', '-p', str(pid), '-o', 'lstart='], capture_output=True, text=True)
    return result.stdout.strip() if result.returncode == 0 else ''


def spawn(h, argv, env, label, cwd):
    with (h.root / (label + '.log')).open('ab') as output:
        proc = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                stdout=output, stderr=output, start_new_session=True)
    h.state.setdefault('children', []).append({'pid': proc.pid, 'name': label, 'stamp': ps_stamp(proc.pid)})
    h.save()
    return proc


def wait_http(base, process=None):
    deadline = time.monotonic() + 20
    client = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    while time.monotonic() < deadline:
        if process and process.poll() is not None:
            raise AssertionError('owned service exited during startup')
        try:
            with client.open(base + '/health', timeout=1) as reply:
                if reply.status == 200:
                    return
        except (OSError, urllib.error.URLError):
            time.sleep(.1)
    raise AssertionError('loopback service failed to become ready')


def install_env(h, kind):
    root = directory(h.root / kind)
    env = sanitized(root)
    workspace = directory(root / 'workspace')
    data = directory(root / 'state')
    sdk_dir = h.root / 'install/lib/openai-proxy/workers'
    env.update(PROXY_CONSOLE_WORKSPACE=str(workspace), PROXY_CONSOLE_DATA=str(data),
               PROXY_SDK_WORKER=str(sdk_dir / 'main.mjs'), OPENAI_PROXY_PLUGIN_DIR=str(sdk_dir.parent/'plugin'),
               PROXY_OPENCODE_PROVIDER=json.dumps({'id': 'fixture', 'baseURL': h.upstream+'/v1', 'model':'fixture-chat'}),
               HOST='127.0.0.1', PORT=str(port()), PROXY_CONSOLE_TOKEN=h.keys['local'],
               CODEX_BACKEND_URL=h.upstream+'/responses', PROXY_BACKEND_CREDENTIAL_CLASS='subscription',
               HTTP_PROXY='http://127.0.0.1:1', HTTPS_PROXY='http://127.0.0.1:1', ALL_PROXY='http://127.0.0.1:1',
               NO_PROXY='127.0.0.1,localhost,::1', MCP_HTTP_PORT=str(port()), PROXY_MCP_TOKEN=secrets.token_hex(32))
    env['PROXY_CONSOLE_ORIGIN'] = origin(env['PORT'])
    env['PATH'] = str(h.binary.parent) + os.pathsep + env['PATH']
    bun = shutil.which('bun')
    if bun:
        env['PROXY_SDK_BUN'] = bun
    choices = [p for p in (sdk_dir / 'node_modules/@openai').glob('codex-*/vendor/**/codex') if p.is_file() and os.access(p, os.X_OK)]
    # An absent pinned SDK binary stays unavailable, even if the developer has another CLI installed.
    env['PROXY_CODEX_BINARY'] = str(choices[0] if len(choices) == 1 else root/'unavailable-cli')
    (Path(env['CODEX_HOME'])/'config.toml').write_text('cli_auth_credentials_store = "file"\n')
    write(data/'defaults.json', {'defaultDriver':'opencode','defaultModel':'fixture-chat'})
    if kind=='local':
        write(Path(env['CODEX_HOME'])/'auth.json', {'auth_mode':'chatgptAuthTokens','tokens':{
            'access_token':secrets.token_urlsafe(44),'account_id':'kbd-local-owner'}})
    if kind != 'local':
        env.pop('MCP_HTTP_PORT')
        env.pop('PROXY_MCP_TOKEN')
        entries = []
        for who in ('admin','chat','chat2'):
            subject = directory(root / who)
            chome = directory(subject / 'codex')
            (chome/'config.toml').write_text('cli_auth_credentials_store = "file"\n')
            entries.append(dict(id=who, role='admin' if who=='admin' else 'chat',
                                workspace=str(directory(subject/'workspace')), codexHome=str(chome),
                                tokenSha256=hashlib.sha256(h.keys[who].encode()).hexdigest(),
                                openCodeProvider=dict(json.loads(env['PROXY_OPENCODE_PROVIDER']),model='fixture-chat-two' if who=='chat2' else 'fixture-chat')))
        path=root/'principals.json'
        write(path, entries)
        env['PROXY_CONSOLE_PRINCIPALS']=str(path)
    material = directory(root/'inputs')
    tool = material/'tools.toml'
    tool.write_text('[[tool]]\nname = "boundary_schema"\ndescription = "A schema-only fixture"\n')
    hook = material/'hooks.toml'
    hook.write_text('[on_request_received]\nurl = '+json.dumps(h.upstream+'/hook')+'\n')
    skills = directory(material/'skills')
    for color in ('amber','blue'):
        path=directory(skills/color)/'SKILL.md'
        path.write_text(f'---\nname: {color}orchard\ndescription: {color}orchard\ntriggers:\n  keywords:\n    - {color}orchard\n---\n\nkbd-skill-{color}\n')
    cfg = material/'config.toml'
    cfg.write_text('[skills]\ndirs = [' + json.dumps(str(skills)) + ']\nmax_injected = 1\n[mcp]\nconfig_path = ' +
                   json.dumps(str(tool)) + '\n[hooks]\nconfig_path = ' + json.dumps(str(hook)) +
                   '\n[modes]\na2a = ' + ('true' if kind=='local' else 'false') +
                   '\n[memory]\nenabled = ' + ('true' if kind=='memory' else 'false') +
                   '\ndb_path = '+json.dumps(str(root/'memory.db'))+'\n')
    env['OPROXY_CONFIG'] = str(cfg)
    return env


def start(h):
    h.state['keys'] = {k:secrets.token_hex(32) for k in ['local','admin','chat','chat2','image']}
    h.keys=h.state['keys']
    upstream_port=port()
    h.state['upstream']=h.upstream=origin(upstream_port)
    spawn(h,[sys.executable,'-B',str(h.source/'tests/console_boundary/recipient.py'),str(upstream_port),str(h.root)],
          sanitized(directory(h.root/'recipient')), 'upstream',h.root)
    for kind in ('local','shared','memory'):
        if kind=='memory' and not h.memory_binary.exists():
            continue
        env=install_env(h,kind)
        proc=spawn(h,[str(h.memory_binary if kind=='memory' else h.binary),'serve'],env,kind,h.root/kind)
        h.state[kind]=dict(env=env,pid=proc.pid,base=origin(env['PORT']))
        h.save()
        if kind=='memory':
            try:
                wait_http(origin(env['PORT']),proc)
            except AssertionError:
                h.calls.append({'name':'optional memory service startup','status':'FAIL','observed':'isolated feature-enabled service did not become ready'})
                h.state.pop('memory',None)
                h.save()
        else:
            wait_http(origin(env['PORT']),proc)
    # Host lanes are independent of the image and remain available after an
    # isolated image readiness failure.
    h.state['hostReady'] = True
    h.save()
    if 'image' in h.state:
        start_image(h)
    return {'local':True,'shared':True,'memory':bool(h.state.get('memory')),
            'image':bool(h.state.get('image_base'))}


def start_image(h):
    import image_model_fixture
    env = docker_env(sanitized(h.root/'docker'))

    def docker(*args):
        h.count()
        return subprocess.run(['docker', *args], env=env, capture_output=True,
                              text=True, timeout=15)

    tag = h.state['tag']
    p = port()
    container = docker('run', '-d', '--pull=never', '--network', 'none',
                       '--name', tag, '-e', 'PROXY_CONSOLE_TOKEN='+h.keys['image'],
                       '-e', 'PROXY_CONSOLE_ORIGIN='+origin(p), '-e', image_model_fixture.provider_env(),
                       h.state['image'])
    if container.returncode:
        h.calls.append({'name':'image start', 'status':'FAIL', 'observed':'docker run exited unsuccessfully'})
        h.note('image launch', container.stderr[-1500:])
        return
    h.state['container'] = container.stdout.strip()
    h.save()
    network = docker('inspect', '--format', '{{.HostConfig.NetworkMode}}', h.state['container'])
    if network.returncode != 0 or network.stdout.strip() != 'none':
        raise AssertionError('container egress isolation could not be verified')
    h.check('in-container synthetic model recipient', lambda: image_model_fixture.start(h, docker))
    relay = spawn(h, [sys.executable, '-B', str(Path(__file__)), str(p), h.state['container']],
                  env, 'image-loopback-relay', h.root)
    try:
        wait_http(origin(p), relay)
        h.state['image_base'] = origin(p)
        h.save()
    except AssertionError:
        h.calls.append({'name':'image health from host', 'status':'FAIL',
                        'observed':'fixture loopback relay did not reach container health'})
    finally:
        identity = docker('inspect', '--format', '{{.State.Status}} {{.State.ExitCode}}', h.state['container'])
        inside = docker('exec', h.state['container'], 'curl', '--max-time', '3', '-sS', '-o', '/dev/null',
                        '-w', '%{http_code}', 'http://127.0.0.1:8181/health')
        logs = docker('logs', '--tail', '60', h.state['container'])
        h.note('image readiness', {'hostReady':bool(h.state.get('image_base')),
                                   'transport':'host loopback relay to network-disabled container',
                                   'state':identity.stdout.strip(), 'insideStatus':inside.stdout.strip(),
                                   'insideExit':inside.returncode, 'logs':(logs.stdout+logs.stderr)[-5000:]})
        if inside.returncode or inside.stdout.strip() != '200':
            h.calls.append({'name':'image health inside container', 'status':'FAIL',
                            'observed':'container loopback health did not respond successfully'})


def stop(h):
    results=[]
    for child in reversed(h.state.get('children',[])):
        pid=child['pid']
        if child['stamp'] and ps_stamp(pid)==child['stamp']:
            os.killpg(pid,signal.SIGTERM)
            deadline=time.monotonic()+3
            while time.monotonic()<deadline and ps_stamp(pid)==child['stamp']:
                time.sleep(.1)
            if ps_stamp(pid)==child['stamp']:
                os.killpg(pid,signal.SIGKILL)
        results.append(child['name'])
    env=docker_env(sanitized(h.root/'docker'))
    for key,argv in [('container',['docker','rm','-f']),('dockerNetwork',['docker','network','rm']),('image',['docker','image','rm'])]:
        if h.state.get(key):
            object_id=h.state[key] if key!='image' else h.state['tag']
            subprocess.run(argv+[object_id],env=env,stdout=subprocess.DEVNULL,check=True,timeout=30)
    return results


# Stdio carries one raw HTTP connection through Docker exec. The container
# keeps network=none, while the browser sees a loopback-only HTTP origin.
IMAGE_SOCKET = '''
import { connect } from 'node:net';
const target = connect(8181, '127.0.0.1');
target.on('error', () => process.exit(1));
process.stdin.pipe(target);
target.pipe(process.stdout);
'''


def image_relay(port_number, container):
    if len(container) != 64 or any(ch not in '0123456789abcdef' for ch in container):
        raise ValueError('expected the fixture container ID')

    class Handler(socketserver.BaseRequestHandler):
        def handle(self):
            args = ['docker', 'exec', '-i', container, '/usr/local/bin/bun', '-e', IMAGE_SOCKET]
            with subprocess.Popen(args, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                  stderr=subprocess.DEVNULL, bufsize=-1) as peer:
                def forward_request():
                    try:
                        while data := self.request.recv(65536):
                            peer.stdin.write(data)
                            peer.stdin.flush()
                    except OSError:
                        pass
                    finally:
                        peer.stdin.close()

                writer = threading.Thread(target=forward_request, daemon=True)
                writer.start()
                try:
                    while data := peer.stdout.read1(65536):
                        self.request.sendall(data)
                except OSError:
                    pass
                finally:
                    try:
                        self.request.shutdown(socket.SHUT_RDWR)
                    except OSError:
                        pass
                    if peer.poll() is None:
                        peer.terminate()
                    writer.join(timeout=2)

    with socketserver.ThreadingTCPServer(('127.0.0.1', port_number), Handler) as relay:
        relay.serve_forever()


if __name__ == '__main__':
    image_relay(int(sys.argv[1]), sys.argv[2])
