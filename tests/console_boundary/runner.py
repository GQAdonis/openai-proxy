"""Execute one command in the phase's signed, ordered packet.

The packet freezes application, fixture and harness inputs. Its private run home,
build logs and redacted evidence index survive across the separate KBD calls.
"""
import fcntl
import hashlib
import importlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

import runtime

REPO = Path(__file__).resolve().parents[2]
PHASE = 'codex-live-auth-opencode-console'
PHASE_DIR = REPO/'.kbd-orchestrator/phases'/PHASE
# A repair has its own frozen inputs, attempt pointer and predecessor report.
# The original B4 packet remains independently addressable.
repair_arg = os.environ.get('KBD_REPAIR_ATTEMPT', '0')
if not re.fullmatch(r'0|[1-9][0-9]*', repair_arg):
    raise ValueError('unsupported repair attempt')
REPAIR = int(repair_arg)
BOUNDARY = 'B5' if REPAIR else 'B4'
PACKET_DIR = PHASE_DIR/'repairs'/f'b5-{REPAIR}' if REPAIR else PHASE_DIR
FROZEN = PACKET_DIR/'verification-candidate.json'
ATTEMPT = PACKET_DIR/'boundary-run.json'
IDS = ['AUTH-1','AUTH-2','AUTH-3','PLUGIN-1','CHAT-1','TOOL-1','UI-1','SURFACE-1',
       'ADMIN-1','SETUP-1','ACP-1','MCP-1','FEATURE-1']
def affected_rows():
    if not REPAIR:
        return IDS
    selected = read(FROZEN)['affectedScenarios']
    if not selected or selected != [row for row in IDS if row in selected]:
        raise ValueError('repair scenarios must follow the adopted packet order')
    return selected


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def read(path):
    return json.loads(path.read_text())


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *_):
        return None


class Harness:
    def __init__(self,state):
        self.state=state
        budget = read(PHASE_DIR/'adopted-contract.json')['budgets']
        self.command_ceiling = budget['toolCalls']
        self.deadline = state['started'] + 60 * budget['timeMinutes']
        self.root=Path(state['root'])
        self.repo=REPO
        self.source=self.root/'source'
        self.local_root=self.root/'local'
        self.binary=self.root/'install/bin/openai-proxy'
        self.memory_binary=self.root/'install/bin/openai-proxy-memory'
        self.keys=state.get('keys',{})
        self.upstream=state.get('upstream')
        self.env=state.get('local',{}).get('env',{})
        self.base=state.get('local',{}).get('base')
        self.shared=state.get('shared',{}).get('base')
        self.memory_base=state.get('memory',{}).get('base')
        self.image_base=state.get('image_base')
        self.local_pid=state.get('local',{}).get('pid')
        self.evidence=PHASE_DIR/'evidence'/state['label']
        self.calls=[]
        self.notes={}

    def save(self):
        runtime.write(self.root/'private-state.json', self.state)

    @property
    def browser_peer(self):
        return read(FROZEN).get('browserPeer')

    @property
    def image_pairing(self):
        if not self.state.get('container'):
            return None
        return subprocess.check_output(['docker','exec',self.state['container'],'cat',
             '/home/oproxy/.local/share/oproxy/console/pairing-token'],
             env=runtime.docker_env(runtime.sanitized(self.root/'docker')),text=True,timeout=10).strip()

    def count(self):
        if self.state['calls'] >= self.command_ceiling or time.time() >= self.deadline:
            self.state['stopReason'] = 'boundary budget reached'
            self.save()
            raise TimeoutError('boundary budget reached')
        self.state['calls'] += 1
        self.save()

    def clean(self,value):
        text=json.dumps(value,default=str,ensure_ascii=True)
        for secret in [*self.keys.values(),self.env.get('PROXY_MCP_TOKEN')]:
            if secret:
                text=text.replace(secret,'[redacted]')
        text=re.sub(r'(?i)Bearer [\w.=-]+', 'Bearer [redacted]',text)
        return json.loads(text)

    def note(self,name,value):
        self.notes[name]=self.clean(value)

    def block(self,name,reason):
        self.calls.append({'name':name,'status':'BLOCKED','observed':str(reason)[:500]})

    def check(self,name,fn):
        self.count()
        started=time.monotonic()
        try:
            detail=fn()
            self.calls.append({'name':name,'status':'PASS','observed':self.clean(detail) if detail is not None else 'assertions satisfied',
                               'seconds':round(time.monotonic()-started,3)})
        except (FileNotFoundError,TimeoutError) as err:
            self.block(name,type(err).__name__+': '+str(err)[:300])
        except Exception as err:
            self.calls.append({'name':name,'status':'FAIL','observed':self.clean(str(err)[:600] or type(err).__name__)})

    def http(self,path,method='GET',body=None,actor='local',headers=None,base=None):
        base=base or (self.base if actor=='local' else self.shared)
        target=urllib.parse.urlsplit(base)
        if target.scheme!='http' or target.hostname not in ('127.0.0.1','localhost','::1') or not path.startswith('/') or path.startswith('//'):
            raise ValueError('fixture HTTP requires an explicit loopback path')
        hdr={}
        if actor in self.keys:
            hdr['Authorization']='Bearer '+self.keys[actor]
        if body is not None:
            hdr['Content-Type']='application/json'
        hdr.update(headers or {})
        content=body if isinstance(body, bytes) else None if body is None else json.dumps(body).encode()
        req=urllib.request.Request(base+path,method=method,headers=hdr,data=content)
        opener=urllib.request.build_opener(urllib.request.ProxyHandler({}),NoRedirect())
        try:
            response=opener.open(req,timeout=25)
        except urllib.error.HTTPError as err:
            response=err
        with response:
            result=response.read(8_388_609)
            if len(result)>8_388_608:
                raise ValueError('fixture response exceeded size limit')
            return response.status,{k.lower():v for k,v in response.headers.items()},result

    def command(self,label,args,env,cwd,limit=2400):
        self.count()
        path=self.evidence/(label+'.log')
        remaining=max(1,self.deadline-time.time())
        with path.open('xb') as record:
            proc=subprocess.Popen(args,cwd=cwd,env=env,stdin=subprocess.DEVNULL,stdout=record,
                                  stderr=subprocess.STDOUT,start_new_session=True)
            try:
                code=proc.wait(timeout=min(remaining,limit))
            except (subprocess.TimeoutExpired,KeyboardInterrupt):
                os.killpg(proc.pid,9)
                proc.wait()
                raise
        self.note(label,{'argv':args,'exit':code,'sha256':sha(path)})
        if code:
            raise AssertionError(f'{label} exited {code}; see {path.name}')


def inputs(h, copy=False):
    for name, expected in read(FROZEN)['files'].items():
        source=REPO/name
        link=expected.get('symlink')
        if link is not None:
            if not source.is_symlink() or os.readlink(source)!=link or not source.resolve().is_relative_to(REPO):
                raise ValueError('frozen symlink changed: '+name)
            if copy:
                dest=h.source/name
                dest.parent.mkdir(parents=True,exist_ok=True)
                dest.symlink_to(link)
            continue
        if source.is_symlink() or sha(source)!=expected['sha256'] or (source.stat().st_mode & 0o111)!=expected['executableBits']:
            raise ValueError('frozen input changed: '+name)
        if copy:
            dest=h.source/name
            dest.parent.mkdir(parents=True,exist_ok=True)
            shutil.copy2(source,dest)


def build(h):
    inputs(h,copy=True)
    env=runtime.sanitized(runtime.directory(h.root/'build'))
    env.update(CARGO_HOME=str(runtime.directory(h.root/'cargo')),CARGO_TARGET_DIR=str(h.root/'target'),
               RUSTUP_HOME=os.environ.get('RUSTUP_HOME',str(Path.home()/'.rustup')),
               CARGO_TERM_COLOR='never',NPM_CONFIG_USERCONFIG=os.devnull, npm_config_audit='false',npm_config_fund='false')
    for name,args in [('rustc',['rustc','--version']),('cargo',['cargo','--version']),('node',['node','--version']),
                      ('npm',['npm','--version']),('bun',['bun','--version'])]:
        h.command('version-'+name,args,env,h.source,10)
    failed_steps=set()
    def build_step(name,args,cwd,required=()):
        if failed_steps.intersection(required):
            failed_steps.add(name)
            h.block(name,'prerequisite build step failed')
            return False
        try:
            h.command(name,args,env,cwd)
            return True
        except (FileNotFoundError,TimeoutError,subprocess.TimeoutExpired):
            h.block(name,'dependency unavailable or timeout')
        except AssertionError as err:
            h.calls.append({'name':name,'status':'FAIL','observed':str(err)})
        failed_steps.add(name)
        return False
    dependencies={'plugin-typecheck':['plugin-install'],'plugin-build':['plugin-install'],'plugin-pack':['plugin-build'],
                  'cargo-build':['frontend-install'],'cargo-clippy':['cargo-build'],'cargo-unit':['cargo-build'],'cargo-release':['cargo-build']}
    for name,args,cwd in [
        ('frontend-install',['npm','ci','--include=dev','--ignore-scripts'],h.source/'frontend'),
        ('worker-install',['bun','install','--frozen-lockfile','--ignore-scripts'],h.source/'workers'),
        ('plugin-install',['bun','install','--frozen-lockfile','--ignore-scripts'],h.source/'plugin'),
        ('plugin-typecheck',['bun','run','typecheck'],h.source/'plugin'),
        ('plugin-build',['bun','run','build'],h.source/'plugin'),
        ('plugin-pack',['npm','pack','--ignore-scripts','--pack-destination',str(h.root)],h.source/'plugin'),
        ('cargo-build',['cargo','build','--locked'],h.source),
        ('cargo-clippy',['cargo','clippy','--locked','--','-D','warnings'],h.source),
        ('cargo-unit',['cargo','test','--locked','--lib'],h.source),
        ('cargo-release',['cargo','build','--release','--locked'],h.source)]:
        available=build_step(name,args,cwd,dependencies.get(name,[]))
        if available and name=='cargo-build' and re.search(rb'(?m)^warning:',(h.evidence/'cargo-build.log').read_bytes()):
            h.calls.append({'name':'zero-warning Cargo build','status':'FAIL','observed':'Cargo emitted a warning; see cargo-build.log'})
    if 'cargo-release' in failed_steps:
        raise AssertionError('installed release unavailable; see build receipts')
    runtime.directory(h.binary.parent)
    shutil.copy2(h.root/'target/release/openai-proxy',h.binary)
    for package in ('workers','plugin'):
        shutil.copytree(h.source/package,h.root/'install/lib/openai-proxy'/package,symlinks=True)
    h.note('installedRelease',{'path':str(h.binary),'sha256':sha(h.binary)})
    if 'FEATURE-1' in affected_rows() and build_step('cargo-memory',['cargo','build','--locked','--features','memory'],h.source):
        shutil.copy2(h.root/'target/debug/openai-proxy',h.memory_binary)
        h.note('memoryBuild',sha(h.memory_binary))
    engine=runtime.docker_env(env)
    if not shutil.which('docker',path=env['PATH']):
        h.block('final image','local Docker engine CLI unavailable')
        return
    available=subprocess.run(['docker','info','--format','{{.ServerVersion}}'],env=engine,
                  capture_output=True,text=True,timeout=15)
    if available.returncode:
        h.block('final image','local Docker engine unavailable')
        return
    h.note('engine',available.stdout.strip())
    tag='kbd-console-'+h.state['label']
    h.state['tag']=tag
    h.save()
    h.command('image-build',['docker','build','--force-rm','--target','sdk-runtime','--iidfile',str(h.root/'image.id'),
                              '-t',tag,'.'],engine,h.source)
    h.state['image']=(h.root/'image.id').read_text().strip()
    h.note('imageId',h.state['image'])
    h.save()


def verify_authority(contract):
    latest=read(PHASE_DIR/'boundary-authorization.json')['events'][-1]
    candidate=sha(FROZEN)
    bound=contract['identity']['contractDigest']
    snap=PHASE_DIR/'adopted'/(bound+'.json')
    normalized=dict(contract,identity={k:v for k,v in contract['identity'].items() if k not in ('contractDigest','planRevision')})
    if not snap.is_file() or sha(snap)!=bound or read(snap)!=normalized:
        raise RuntimeError('contract bytes differ from the adopted snapshot')
    for name,expected in read(FROZEN).get('toolFiles',{}).items():
        selected=shutil.which(name)
        if bool(selected)!=bool(expected) or (selected and sha(Path(selected).resolve())!=expected):
            raise RuntimeError('installed tool identity changed: '+name)
    if REPAIR:
        previous_file = PACKET_DIR/'previous-results.json'
        previous = read(previous_file)
        manifest = read(FROZEN)
        if (REPAIR > contract['budgets']['repairAttempts']
                or sha(previous_file) != manifest['previousResultsSha256']
                or previous['budgetConsumption']['values']['repairAttempts']['value'] + 1 != REPAIR):
            raise RuntimeError('repair exceeds the adopted limit or does not match the preceding attempt')
        if not any(r['status'] == 'FAIL' for r in previous['scenarios']):
            path = REPO / previous['evidence'] / 'run.json'
            if (not path.is_file() or sha(path) != manifest.get('previousStartupReceiptSha256')):
                raise RuntimeError('repair lacks the preceding startup failure receipt')
            startup = read(path)
            if startup['status'] != 'FAIL' or startup['candidate'] != previous['candidateDigest']:
                raise RuntimeError('preceding startup receipt does not identify a failed candidate')
    if not (latest['boundary']==BOUNDARY and contract['status']['implementationComplete']
            and latest['contractDigest']==contract['identity']['contractDigest']==read(FROZEN)['contractDigest']
            and candidate in latest.get('reason','')):
        raise RuntimeError(f'{BOUNDARY} requires current owner authorization bound to this frozen candidate')


def summary(h):
    outcomes=[]
    previous = read(PACKET_DIR/'previous-results.json') if REPAIR else {}
    prior_rows = {row['id']: row for row in previous.get('scenarios', [])}
    selected = affected_rows()
    for row in IDS:
        if row not in selected:
            outcomes.append(prior_rows[row])
            continue
        reports=[]
        for side in ['scenario','negative']:
            file=h.evidence/(side+'-'+row+'.json')
            if file.exists():
                reports.append(read(file))
        states=[rec['status'] for rec in reports]
        status='FAIL' if 'FAIL' in states or (h.state.get('buildFailed') and row=='UI-1') else (
            'NOT_EXERCISED' if not reports else 'BLOCKED' if len(reports)!=2 or 'BLOCKED' in states else 'PASS')
        outcomes.append({'id':row,'status':status,'attempted':', '.join(rec['command'] for rec in reports) or 'packet stopped before this row',
                         'observed':states or [h.state.get('stopReason','not attempted')],
                         'evidence':str(h.evidence.relative_to(REPO)),'executedBy':h.state['executor']})
    contract=read(PHASE_DIR/'adopted-contract.json')
    values={k:{'value':v,'class':'measured'} for k,v in {
        'timeMinutes':round((time.time()-h.state['started'])/60,2),'toolCalls':h.state['calls'],
        'repairAttempts':REPAIR,'noProgress':h.state.get('previousNoProgress',0)}.items()}
    result={
        'phase':PHASE,'boundary':BOUNDARY,'affectedScenarios':selected,**{k:v for k,v in contract['identity'].items() if k!='phaseId'},
        'candidateDigest':sha(FROZEN),'scenarios':outcomes,'budgetConsumption':{
        'source':'clock and packet command/probe counter','values':values},
        'evidence':str(h.evidence.relative_to(REPO)),'cleanup':h.state.get('cleanup'),'executedBy':h.state['executor']}
    if REPAIR:
        result['previousResultsSha256']=read(FROZEN)['previousResultsSha256']
        runtime.write(PACKET_DIR/'boundary-results.json',result)
    runtime.write(PHASE_DIR/'boundary-results.json',result)


def main(argv):
    command=' '.join(argv)
    commands=['build','run',*[name+' '+row for row in affected_rows() for name in ('scenario','negative')],'cleanup']
    if command not in commands:
        raise SystemExit('expected build, run, scenario ID, negative ID, or cleanup')
    os.umask(0o077)
    contract=read(PHASE_DIR/'adopted-contract.json')
    if command!='cleanup':
        verify_authority(contract)
    with (PHASE_DIR/'boundary.lock').open('a') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        if command=='build':
            if ATTEMPT.exists():
                raise RuntimeError(f'{BOUNDARY} packet already started; keep the original receipts')
            prior = {}
            if REPAIR:
                if sha(PHASE_DIR/'boundary-results.json') != read(FROZEN)['previousResultsSha256']:
                    raise RuntimeError('newer results already exist for this phase')
                prior = read(PACKET_DIR/'previous-results.json')['budgetConsumption']['values']
            diagnostics = read(FROZEN).get('recoveryDiagnosticsBudget', {})
            calls = prior.get('toolCalls', {}).get('value', 0) + diagnostics.get('toolCalls', 0)
            minutes = prior.get('timeMinutes', {}).get('value', 0) + diagnostics.get('timeMinutes', 0)
            root=Path(tempfile.mkdtemp(prefix='kbd-console-')).resolve()
            state={'label':secrets_label(),'root':str(root),'next':0,
                   'calls':calls,
                   'started':time.time()-60*minutes,
                   'previousNoProgress':prior.get('noProgress',{}).get('value',0),
                   'executor':{'harness':os.environ.get('KBD_EXECUTOR_HARNESS','Bash/Python KBD boundary runner'),
                               'model':os.environ.get('KBD_EXECUTOR_MODEL','none (scripted observations)'),
                               'python':platform.python_version()},'candidate':sha(FROZEN)}
            h=Harness(state)
            runtime.directory(h.evidence)
            h.save()
            runtime.write(ATTEMPT,{'root':str(root),'label':state['label'],'candidate':state['candidate']})
        else:
            pointer=read(ATTEMPT)
            root=Path(pointer['root']).resolve()
            if root.parent!=Path(tempfile.gettempdir()).resolve() or not root.name.startswith('kbd-console-'):
                raise RuntimeError('scratch path does not belong to this packet')
            state=read(root/'private-state.json')
            h=Harness(state)
            if sha(FROZEN)!=state['candidate']:
                raise RuntimeError('candidate identity changed; retain this attempt')
        if command!='cleanup' and (state.get('stopReason') or commands[state['next']]!=command):
            raise RuntimeError('packet order or preceding result prevents this step')
        if command!='cleanup':
            inputs(h)
        failed=False
        try:
            if command!='cleanup':
                h.count()
            if command=='build':
                build(h)
            elif command=='run':
                h.note('deployments',runtime.start(h))
            elif command=='cleanup':
                h.note('stopped',runtime.stop(h))
            else:
                side,row=argv
                which='identity' if row in IDS[:6] else 'interface' if row in IDS[6:8] else 'admin'
                importlib.import_module(which).run(h,row,side=='negative')
                if not h.calls:
                    h.block('no verified observations','case returned without an executable assertion')
        except (FileNotFoundError,TimeoutError,subprocess.TimeoutExpired) as err:
            h.block(command,str(err)[:300])
            if command == 'build' or (command == 'run' and not state.get('hostReady')):
                state['stopReason']='required dependency or environment unavailable'
                failed=True
        except Exception as err:
            h.calls.append({'name':command,'status':'FAIL','observed':h.clean(str(err)[:600])})
        status='FAIL' if any(x['status']=='FAIL' for x in h.calls) else (
                'BLOCKED' if any(x['status']=='BLOCKED' for x in h.calls) else 'PASS')
        if status=='FAIL':
            failed=True
            # Preserve the rest of the ordered packet on ordinary product failures.
            # Abort dependent work if build/start fails or an authority/side-effect control fails.
            observations=' '.join(str(x.get('name',''))+' '+str(x.get('observed','')) for x in h.calls if x.get('status')=='FAIL')
            unsafe=bool(re.search(r'foreign|unauthori[sz]ed|egress|disclos|duplicat.*effect|other.owner|charge',observations,re.I))
            if (command=='run' and not state.get('hostReady')) or (command=='build' and not h.binary.is_file()) or unsafe:
                state['stopReason']='failed prerequisite or isolation control at '+command
            if command=='build' and not h.binary.is_file():
                state['buildFailed']=True
        rec={'command':command,'status':status,'observations':h.calls,'notes':h.notes,'candidate':state['candidate'],
             'executedBy':state['executor'],'completedAt':time.time()}
        runtime.write(h.evidence/(command.replace(' ','-')+'.json'),rec)
        if command=='cleanup':
            state['cleanup']=status
            # Retain the original fixture receiver trace; it contains only redacted effect labels.
            trace=h.root/'upstream-observations.jsonl'
            if trace.exists():
                shutil.copy2(trace,h.evidence/trace.name)
            originals=h.root/'admin-originals'
            if originals.is_dir():
                shutil.copytree(originals,h.evidence/'admin-originals',dirs_exist_ok=True)
            summary(h)
            if status=='PASS':
                shutil.rmtree(h.root)
            else:
                h.save()
        else:
            state['next']+=1
            h.save()
        print(command,status,str(h.evidence.relative_to(REPO)),flush=True)
        if failed:
            raise SystemExit(2)


def secrets_label():
    import secrets
    return time.strftime('%Y%m%d-%H%M%S',time.gmtime())+'-'+secrets.token_hex(4)


if __name__=='__main__':
    main(sys.argv[1:])
