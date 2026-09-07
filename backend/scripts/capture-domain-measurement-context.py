#!/usr/bin/env python3
"""Read-only macOS measurement context. Never runs a Cargo build or judges comparability."""
from __future__ import annotations
import argparse
import hashlib
import json
import os
import platform
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

VERSION = 2
PROFILE_KEYS = {'inherits','opt-level','debug','split-debuginfo','strip','debug-assertions','overflow-checks','lto','panic','incremental','codegen-units','rpath','codegen-backend'}
BUILD_KEYS = {'jobs','incremental','rustflags','rustdocflags','target','rustc','rustc-wrapper','rustc-workspace-wrapper','rustdoc','target-dir','build-dir'}
TARGET_KEYS = {'linker','runner','rustflags','rustdocflags'}
ENV_KEYS = {'CARGO_BUILD_JOBS','CARGO_INCREMENTAL','CARGO_TERM_COLOR','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','RUSTDOCFLAGS','CARGO_ENCODED_RUSTDOCFLAGS','RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','RUSTUP_TOOLCHAIN','CARGO_BUILD_TARGET','CARGO_BUILD_RUSTFLAGS','CARGO_BUILD_RUSTDOCFLAGS','CARGO_BUILD_INCREMENTAL','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER','RUSTDOC','CARGO_BUILD_RUSTDOC','RUSTC_BOOTSTRAP','CC','CXX','AR','CFLAGS','CXXFLAGS','CPPFLAGS','LDFLAGS','MAKEFLAGS','CARGO_MAKEFLAGS'}
PROCESS_NAMES = {'cargo','rustc','rustdoc','rust-lld','ld','ld64','lld','ld.lld','ld64.lld','clang','clang++','cc','c++','gcc','g++','collect2','mold','sccache','rustc_codegen_cranelift'}
SAFE_WORDS = {'dev','release','test','bench','unwind','abort','none','line-directives-only','line-tables-only','limited','full','packed','unpacked','off','fat','thin','debuginfo','symbols','llvm','cranelift','always','never','auto','default','s','z'}
class CaptureError(Exception): pass

def canonical(value): return json.dumps(value, sort_keys=True, separators=(',',':'), ensure_ascii=False).encode()
def digest(value): return hashlib.sha256(value if isinstance(value,bytes) else canonical(value)).hexdigest()
def now(): return datetime.now(timezone.utc).isoformat()
def identity(kind,value): return digest({'namespace':'erp.measurement.context.v1.'+kind,'value':value})

def run(argv, cwd):
    """Do not persist argv, environment or stderr; failures expose only fixed labels."""
    try:
        p=subprocess.run(argv,cwd=cwd,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=15,check=False)
    except (OSError,subprocess.TimeoutExpired): raise CaptureError('command_unavailable_or_timeout') from None
    if p.returncode: raise CaptureError('command_failed')
    return p.stdout

def safe_value(value):
    """Never dump arbitrary config/env strings, even under allowed build keys."""
    if isinstance(value,bool) or type(value) is int or value is None:return value
    if isinstance(value,str) and value in SAFE_WORDS:return value
    if isinstance(value,list):return [safe_value(v) for v in value]
    return {'value_sha256':digest(value),'value_type':type(value).__name__}

def profile_projection(value):
    if not isinstance(value,dict):return {}
    out={k:safe_value(v) for k,v in value.items() if k in PROFILE_KEYS}
    if isinstance(value.get('build-override'),dict):out['build-override']=profile_projection(value['build-override'])
    if isinstance(value.get('package'),dict):
        out['package']={identity('package-key',k):profile_projection(v) for k,v in sorted(value['package'].items())}
    return out

def configuration_projection(doc):
    out={}
    if isinstance(doc.get('build'),dict):out['build']={k:safe_value(v) for k,v in doc['build'].items() if k in BUILD_KEYS}
    if isinstance(doc.get('profile'),dict):out['profile']={k:profile_projection(v) for k,v in doc['profile'].items() if k in ('dev','release','test','bench')}
    if isinstance(doc.get('target'),dict):
        out['target']={identity('target-key',k):{f:safe_value(v) for f,v in values.items() if f in TARGET_KEYS} for k,values in doc['target'].items() if isinstance(values,dict)}
    if isinstance(doc.get('unstable'),dict):out['unstable']={k:safe_value(v) for k,v in doc['unstable'].items() if k in ('codegen-backend','profile-rustflags','config-include','build-std','build-std-features')}
    # Config [env] is not dumped. Record only its presence: it can affect builds.
    out['has_env_table']=bool(doc.get('env'))
    out['has_include']=bool(doc.get('include'))
    return out

def load_toml(path):
    try:
        raw=path.read_bytes();doc=tomllib.loads(raw.decode('utf-8'))
    except (OSError,UnicodeError,tomllib.TOMLDecodeError):raise CaptureError('config_unreadable_or_invalid') from None
    return raw,doc

def config_candidates(backend,env):
    """Cargo home lowest priority, then root toward the actual invocation cwd."""
    home=Path(env.get('CARGO_HOME') or str(Path.home()/'.cargo')).expanduser()
    dirs=[home]+[p/'.cargo' for p in reversed([backend,*backend.parents])]
    paths=[]
    for d in dirs:
        choices=[d/'config',d/'config.toml']
        chosen=next((p for p in choices if p.is_file()),None)
        if chosen and chosen.resolve() not in paths:paths.append(chosen.resolve())
    return paths

def jobs_value(value,cpu):
    if value=='default':return cpu
    if isinstance(value,bool):raise CaptureError('invalid_build_jobs')
    if isinstance(value,str) and re.fullmatch(r'-?\d+',value):value=int(value)
    if type(value) is not int or value==0:raise CaptureError('invalid_build_jobs')
    effective=value if value>0 else cpu+value
    if effective<=0:raise CaptureError('nonpositive_negative_jobs_result_unresolved')
    return effective

def build_configuration(backend,env,cpu):
    docs=[];jobs=None;source='default:logical_cpu_count';problems=[]
    for p in config_candidates(backend,env):
        raw,doc=load_toml(p)
        docs.append({'path':str(p),'sha256':digest(raw),'allowed_build_configuration':configuration_projection(doc)})
        if 'jobs' in doc.get('build',{}):jobs=doc['build']['jobs'];source='config:'+str(p)
        if doc.get('include'):problems.append('cargo_config_include_requires_explicit_resolution')
        if doc.get('env'):problems.append('cargo_config_env_table_requires_whitelisted_effect_review')
        if any(doc.get('build',{}).get(k) for k in ('rustc','rustc-wrapper','rustc-workspace-wrapper')):problems.append('compiler_or_wrapper_override_requires_explicit_toolchain_review')
    raw,manifest=load_toml(backend/'Cargo.toml')
    manifest_profiles=configuration_projection({'profile':manifest.get('profile',{})})
    selected_env={k:safe_value(v) for k,v in sorted(env.items()) if k in ENV_KEYS or re.fullmatch(r'CARGO_PROFILE_(DEV|RELEASE|TEST|BENCH)_(OPT_LEVEL|DEBUG|SPLIT_DEBUGINFO|STRIP|DEBUG_ASSERTIONS|OVERFLOW_CHECKS|LTO|PANIC|INCREMENTAL|CODEGEN_UNITS|RPATH|CODEGEN_BACKEND)',k) or re.fullmatch(r'CARGO_TARGET_[A-Z0-9_]+_(LINKER|RUSTFLAGS|RUSTDOCFLAGS)',k)}
    if 'CARGO_BUILD_JOBS' in env:jobs=env['CARGO_BUILD_JOBS'];source='env:CARGO_BUILD_JOBS'
    if env.get('CARGO_MAKEFLAGS') or env.get('MAKEFLAGS'):problems.append('inherited_jobserver_requires_explicit_review')
    if any(env.get(k) for k in ('RUSTC','RUSTC_WRAPPER','RUSTC_WORKSPACE_WRAPPER','CARGO_BUILD_RUSTC','CARGO_BUILD_RUSTC_WRAPPER','CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')):problems.append('compiler_or_wrapper_override_requires_explicit_toolchain_review')
    count=jobs_value(jobs,cpu) if jobs is not None else cpu
    # Absolute config paths/full-file hashes do not enter the comparable build
    # projection: credentials and repository locations are not build settings.
    material={'config_precedence_low_to_high':[d['allowed_build_configuration'] for d in docs], 'workspace_profiles':manifest_profiles,'build_environment':selected_env,'profile':'dev','features':'default (no --features)','build_jobs':count}
    def backends(value):
        found=[]
        if isinstance(value,dict):
            for k,v in value.items():
                if k=='codegen-backend':found.append(v)
                else:found.extend(backends(v))
        elif isinstance(value,list):
            for v in value:found.extend(backends(v))
        return found
    observed=backends(material)
    return {'selected_profile':'dev','codegen_backend_declarations':observed,'codegen_note':'Declarations only; flags are fingerprinted without exposing their arbitrary values. Actual unit codegen is not inferred.','files':docs,'workspace_manifest':{'path':str(backend/'Cargo.toml'),'sha256':digest(raw),'profile':manifest_profiles},'allowed_environment':selected_env,'effective_build_jobs':count,'build_jobs_source':source,'build_jobs_configured_value':safe_value(jobs),'build_configuration_sha256':digest(material),'fingerprint_material':material,'unresolved':sorted(set(problems)),'invocation_contract':'measure-incremental.py cargo check/build commands without --jobs/--config/--profile/--features overrides; command-line overrides are not inferred from process argv.'}

def parse_df(raw):
    text=raw.decode('utf-8');lines=text.strip().splitlines()
    if len(lines)!=2 or 'Mounted on' not in lines[0]:raise CaptureError('df_unresolved')
    parts=lines[1].split(None,5)
    if len(parts)!=6 or not re.fullmatch(r'/dev/disk\d+(?:s\d+)*',parts[0]):raise CaptureError('non_local_or_unresolved_target_device')
    try:blocks,used,available=map(int,parts[1:4])
    except ValueError:raise CaptureError('df_invalid_numbers') from None
    return {'device_node':parts[0],'blocks_512':blocks,'used_blocks_512':used,'available_blocks_512':available,'capacity':parts[4],'mount_point_sha256':identity('mount-point',parts[5])}

def media(target,host_id,runner=run):
    if not host_id:raise CaptureError('host_identity_missing')
    df=parse_df(runner(['/bin/df','-P',str(target)],target))
    def info(node):
        try:d=plistlib.loads(runner(['/usr/sbin/diskutil','info','-plist',node],target))
        except (ValueError,plistlib.InvalidFileException):raise CaptureError('diskutil_invalid_plist') from None
        if not isinstance(d,dict) or d.get('Error'):raise CaptureError('diskutil_unresolved')
        return d
    volume=info(df['device_node']);volume_uuid=volume.get('VolumeUUID') or volume.get('DiskUUID')
    if not volume_uuid:raise CaptureError('volume_identity_unresolved')
    if volume.get('APFSContainerReference'):
        stores=[s.get('APFSPhysicalStore') for s in volume.get('APFSPhysicalStores',[]) if isinstance(s,dict)]
        if not stores:raise CaptureError('apfs_physical_stores_unresolved')
    else:stores=[volume.get('DeviceIdentifier')]
    records=[];stable=[]
    for store in stores:
        if not isinstance(store,str) or not re.fullmatch(r'disk\d+(?:s\d+)*',store):raise CaptureError('physical_store_invalid')
        part=info('/dev/'+store);whole=part.get('ParentWholeDisk') or part.get('DeviceIdentifier')
        if not isinstance(whole,str) or not re.fullmatch(r'disk\d+',whole):raise CaptureError('physical_whole_disk_unresolved')
        disk=info('/dev/'+whole)
        if disk.get('Virtual') is True or disk.get('VirtualOrPhysical')=='Virtual' or disk.get('BusProtocol') in ('Disk Image','Virtual Interface'):raise CaptureError('virtual_media_not_physical')
        store_uuid=part.get('DiskUUID') or part.get('VolumeUUID')
        if not store_uuid:raise CaptureError('physical_store_identity_unresolved')
        facts={k:disk[k] for k in ('Internal','SolidState','TotalSize','DeviceBlockSize') if k in disk}
        protocol=disk.get('BusProtocol');facts['bus_protocol']=protocol if protocol in ('USB','PCI-Express','SATA','Apple Fabric','Thunderbolt','NVMe','FireWire','SD','SAS') else {'value_sha256':digest(protocol)}
        record={'physical_store_device':store,'whole_disk_device':whole,'store_identity_sha256':identity('store-uuid',store_uuid),'facts':facts}
        records.append(record)
        stable.append({'store_identity_sha256':record['store_identity_sha256'],'facts':facts})
    if not records:raise CaptureError('physical_media_unresolved')
    return {'df':df,'volume_id':identity('volume-uuid',volume_uuid),'physical_device_id':identity('physical-stores',{'host_id':host_id,'stores':sorted(stable,key=lambda x:x['store_identity_sha256'])}),'physical_stores':records,'identity_basis':'actual df device -> diskutil volume -> APFSPhysicalStores (or partition) -> ParentWholeDisk; persistent store UUIDs plus observed medium facts, hashed and host-scoped. Not a disk serial-number claim.'}

def process_observation(raw):
    names=Counter(line.strip() for line in raw.decode('utf-8',errors='replace').splitlines() if line.strip() in PROCESS_NAMES)
    return {'counts_by_comm':dict(sorted(names.items())),'matching_process_count':sum(names.values()),'cargo_count':names['cargo'],'rustc_count':names['rustc'],'linker_or_compiler_driver_count':sum(n for name,n in names.items() if name in PROCESS_NAMES-{'cargo','rustc','rustdoc','sccache'}),'collection':'/bin/ps -A -o ucomm=; exact allowlisted comm only; no PIDs, argv, unrelated names or environment'}

def lock_fingerprint(path):
    raw,doc=load_toml(path)
    packages=[{k:p.get(k) for k in ('name','version','source','checksum')} for p in doc.get('package',[]) if p.get('source')]
    if not packages:raise CaptureError('third_party_lock_entries_missing')
    packages.sort(key=lambda p:(p['name'],p['version'],p['source']))
    return {'path':str(path),'sha256':digest(raw),'third_party_versions_sha256':digest(packages),'third_party_package_count':len(packages),'basis':'canonical sorted name/version/source/checksum records for Cargo.lock packages with a source; path-only workspace packages excluded; source URLs are hashed, never emitted'}

def tools_observation(backend,runner=run):
    result={}
    rustup=shutil.which('rustup')
    for name in ('rustc','cargo'):
        found=shutil.which(name)
        if not found:raise CaptureError('toolchain_binary_missing')
        # Resolve proxies without invoking a toolchain installer; versions are
        # queried from existing concrete executables, not rustup proxies.
        if rustup:
            found=runner([rustup,'which',name],backend).decode().strip()
        if not Path(found).is_file():raise CaptureError('toolchain_binary_missing')
        raw=runner([found,'-vV'],backend).decode('utf-8')
        # Official -vV has bounded version labels; don't persist arbitrary output.
        allowed=('rustc ','cargo ','binary:','commit-hash:','commit-date:','host:','release:','LLVM version:','libgit2:','libcurl:','ssl:','os:')
        lines=[line for line in raw.splitlines() if line.startswith(allowed)]
        if not lines or not lines[0].startswith(name+' '):raise CaptureError('version_output_unrecognized')
        result[name+'_verbose']='\n'.join(lines)
        result[name+'_binary_sha256']=digest(Path(found).read_bytes())
    toolfile=next((p for p in [backend/'rust-toolchain.toml',backend/'rust-toolchain',backend.parent/'rust-toolchain.toml',backend.parent/'rust-toolchain'] if p.is_file()),None)
    if toolfile:
        raw=toolfile.read_bytes();result['toolchain_file']={'path':str(toolfile),'sha256':digest(raw)}
        if toolfile.suffix=='.toml':channel=tomllib.loads(raw.decode()).get('toolchain',{}).get('channel')
        else:channel=raw.decode().strip()
        result['toolchain_channel']=channel if isinstance(channel,str) and re.fullmatch(r'[A-Za-z0-9_.-]+',channel) else safe_value(channel)
    else:result['toolchain_channel']=None
    return result

MEASUREMENT_INPUTS = {
    'measurement_script': ('--measurement-script', 'scripts/measure-incremental.py'),
    'probe_spec': ('--measurement-spec', 'docs/superpowers/plans/domain-crate-migration/compile-probes.json'),
    'measurement_contract': ('--measurement-contract', 'docs/superpowers/plans/domain-crate-migration/compile-measurement.md'),
}

def select_measurement_inputs(repo,measurement_script=None,measurement_spec=None,measurement_contract=None):
    """Bind selected files without inventing a historical in-repo tool or execution."""
    supplied={'measurement_script':measurement_script,'probe_spec':measurement_spec,'measurement_contract':measurement_contract}
    selected={}
    for label,(flag,rel) in MEASUREMENT_INPUTS.items():
        default=repo/'backend'/rel
        requested=Path(supplied[label]).expanduser() if supplied[label] is not None else default
        path=requested.resolve(strict=supplied[label] is not None)
        if supplied[label] is not None and not path.is_file():raise CaptureError('explicit_measurement_input_requires_file')
        selected[label]={'path':str(path),'requested_path':str(requested.absolute()),
                         'selection':'explicit_cli' if supplied[label] is not None else 'repo_default',
                         'provided_by':flag if supplied[label] is not None else 'repo_default',
                         'location':'measured_repo' if path.is_relative_to(repo.resolve()) else 'external_to_measured_repo',
                         'repo_default_path':str(default),'repo_default_is_file':default.is_file()}
    return selected

def observe_measurement_input(selection):
    path=Path(selection['path'])
    if not path.is_file():raise CaptureError('selected_measurement_input_missing')
    raw=path.read_bytes()
    return {**selection,'sha256':digest(raw),'size_bytes':len(raw)}

def capture(repo,target,stage,runner=run,env=None,measurement_script=None,measurement_spec=None,measurement_contract=None):
    env=dict(os.environ if env is None else env);backend=repo/'backend'
    selected_inputs=select_measurement_inputs(repo,measurement_script,measurement_spec,measurement_contract)
    issues=[];result={'schema_version':VERSION,'evidence_kind':'live_read_only_measurement_context','captured_at_utc':now(),'stage':stage,'repo':str(repo),'invocation_cwd':str(backend),'target_dir':str(target),'comparable':None,'background_load_review':'requires human review across before/after observations; no threshold or comparable=true is generated'}
    def part(label,fn):
        try:return fn()
        except (CaptureError,OSError,ValueError,KeyError,TypeError):issues.append({'component':label,'error':'observation_failed_or_unresolved','detail_not_emitted':True});return None
    host_id=None
    def hardware():
        numeric={}
        for key in ['hw.ncpu','hw.physicalcpu','hw.logicalcpu','hw.memsize']:
            numeric[key]=int(runner(['/usr/sbin/sysctl','-n',key],backend).strip())
        model=runner(['/usr/sbin/sysctl','-n','hw.model'],backend).decode().strip()
        data=plistlib.loads(runner(['/usr/sbin/ioreg','-rd1','-c','IOPlatformExpertDevice','-a'],backend))
        uid=next((d.get('IOPlatformUUID') for d in data if isinstance(d,dict) and d.get('IOPlatformUUID')),None)
        if not uid:raise CaptureError('hardware_identity_missing')
        if any(v<=0 for v in numeric.values()):raise CaptureError('hardware_numbers_invalid')
        return {'host_id':identity('host-platform-uuid',uid),'hardware_id':identity('hardware',{'platform_uuid':uid,'model':model,**numeric}),'model':model if re.fullmatch(r'[A-Za-z0-9,._-]+',model) else safe_value(model),'cpu_count':numeric['hw.ncpu'],'physical_cpu_count':numeric['hw.physicalcpu'],'logical_cpu_count':numeric['hw.logicalcpu'],'memory_bytes':numeric['hw.memsize'],'os':{'system':platform.system(),'release':platform.release(),'machine':platform.machine()},'privacy':'hostname, IOPlatformUUID and serial values are not persisted'}
    result['hardware']=part('hardware',hardware)
    host_id=(result['hardware'] or {}).get('host_id');result['host_id']=host_id
    result['media']=part('physical_media',lambda:media(target,host_id,runner))
    result['toolchain']=part('toolchain',lambda:tools_observation(backend,runner))
    result['build_configuration']=part('build_configuration',lambda:build_configuration(backend,env,result['hardware']['cpu_count']))
    if result['build_configuration']:
        issues.extend({'component':'build_configuration','error':v} for v in result['build_configuration']['unresolved'])
    result['third_party_lock']=part('third_party_lock',lambda:lock_fingerprint(backend/'Cargo.lock'))
    files={label:part(label,lambda selection=selection:observe_measurement_input(selection)) for label,selection in selected_inputs.items()}
    result['input_selection']=selected_inputs
    result['inputs']=files
    result['measurement_input_claim']='Actual bytes of the explicitly selected or repo-default files at capture time. Explicit paths are operator-supplied invocation inputs; this collector does not execute the measurement script or attest that a historical measurement used it. Match the real measurement CLI and preserve the selected files across both endpoints.'
    result['collector']={'path':str(Path(__file__).resolve()),'sha256':digest(Path(__file__).read_bytes())}
    result['commit']=part('source_commit',lambda:runner(['git','rev-parse','HEAD'],repo).decode().strip())
    if result['commit'] and not re.fullmatch(r'[0-9a-f]{40}',result['commit']):result['commit']=None;issues.append({'component':'source_commit','error':'invalid_commit'})
    # --porcelain output is never persisted: filenames could contain secrets.
    dirty=part('source_dirty',lambda:bool(runner(['git','--no-optional-locks','status','--porcelain','--untracked-files=normal'],repo).strip()))
    result['source_dirty']=dirty
    result['load']=part('load',lambda:{'observed_at_utc':now(),'loadavg_1_5_15':list(os.getloadavg()),'processes':process_observation(runner(['/bin/ps','-A','-o','ucomm='],repo))})
    def get(section,key):return (result.get(section) or {}).get(key)
    result['proof_values']={'host_id':host_id,'hardware_id':get('hardware','hardware_id'),'physical_device_id':get('media','physical_device_id'),'build_jobs':get('build_configuration','effective_build_jobs'),'measurement_script_sha256':(files['measurement_script'] or {}).get('sha256'),'build_configuration_sha256':get('build_configuration','build_configuration_sha256'),'third_party_versions_sha256':get('third_party_lock','third_party_versions_sha256')}
    if result['build_configuration'] and result['build_configuration']['unresolved']:
        result['proof_values']['build_jobs']=None
        result['proof_values']['build_configuration_sha256']=None
    result['captured_end_utc']=now();result['issues']=issues;result['capture_complete']=not issues
    result['limitations']=['This observes the collector process context; it does not inspect secret command argv or prove CLI overrides absent. Match the contracted measurement invocation explicitly.','No --jobs/--config overrides are represented. The configured jobs count is only effective for the recorded invocation contract.','Configuration includes or [env] require explicit review and make capture incomplete, rather than guessing effective values.','Profile settings/package overrides are recorded; Cargo feature resolution and rustc unit profiles are proved by actual measurement artifacts, not this collector.','Physical identity uses actual store UUIDs and device facts, not raw hardware serials. Device UUID cloning cannot be ruled out by this observation.','Beginning/end load observations do not prove load throughout a run. Preserve every snapshot and review original durations without filtering.','No historical environment is reconstructed. No Cargo build/check/metadata, cache cleanup, database or application action is performed.']
    return result

def validate_paths(repo,target,output):
    repo=repo.expanduser().resolve(strict=True);target=target.expanduser().resolve(strict=True)
    if not repo.is_dir() or not (repo/'backend/Cargo.toml').is_file():raise CaptureError('repo_root_requires_backend_manifest')
    if not target.is_dir():raise CaptureError('target_dir_missing')
    output=output.expanduser().absolute()
    if output.exists() or output.is_symlink():raise CaptureError('output_already_exists')
    if not output.parent.is_dir():raise CaptureError('output_parent_missing')
    return repo,target,output

def write_output(path,obj):
    raw=json.dumps(obj,ensure_ascii=False,indent=2).encode()+b'\n'
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    with os.fdopen(fd,'wb') as f:f.write(raw)
    return {'path':str(path.resolve()),'sha256':digest(raw),'capture_complete':obj['capture_complete'],'comparable':None}

class SelfTests(unittest.TestCase):
    def test_safe_values_and_config_exclude_secrets(self):
        secret='SENTINEL_DO_NOT_PERSIST_123'
        doc={'registry':{'token':secret},'http':{'proxy':secret},'env':{'SECRET':secret},'build':{'jobs':4,'rustflags':['--cfg',secret],'secret':secret},'profile':{'dev':{'opt-level':1,'codegen-backend':'cranelift','password':secret,'package':{secret:{'opt-level':2}}}}}
        out=json.dumps(configuration_projection(doc));self.assertNotIn(secret,out);self.assertNotIn('token',out);self.assertIn('cranelift',out)
    def test_jobs(self):
        for raw,want in [(None,None),(4,4),('3',3),(-2,10),('default',12)]:
            if raw is not None:self.assertEqual(jobs_value(raw,12),want)
        for raw in [0,'0','oops',True,-20]:
            with self.assertRaises(CaptureError):jobs_value(raw,12)
    def test_config_precedence_and_environment(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);backend=root/'repo/backend';backend.mkdir(parents=True);(backend/'Cargo.toml').write_text('[workspace]\n')
            home=root/'cargo-home';home.mkdir();(home/'config.toml').write_text('[build]\njobs=2\n')
            conf=backend/'.cargo';conf.mkdir();(conf/'config.toml').write_text('[build]\njobs=5\n');(conf/'config').write_text('[build]\njobs=6\n')
            env={'CARGO_HOME':str(home),'UNRELATED_SECRET':'SENTINEL_UNRELATED_SECRET_719'}
            x=build_configuration(backend,env,12);self.assertEqual(x['effective_build_jobs'],6);self.assertTrue(x['build_jobs_source'].endswith('/config'))
            x=build_configuration(backend,{**env,'CARGO_BUILD_JOBS':'7'},12);self.assertEqual(x['effective_build_jobs'],7);self.assertEqual(x['build_jobs_source'],'env:CARGO_BUILD_JOBS');self.assertNotIn('SENTINEL_UNRELATED_SECRET_719',json.dumps(x))
    def test_effective_jobs_does_not_ignore_config_env(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'Cargo.toml').write_text('[workspace]\n');(root/'.cargo').mkdir();(root/'.cargo/config.toml').write_text('[env]\nCARGO_BUILD_JOBS="8"\n')
            x=build_configuration(root,{'CARGO_HOME':str(root/'none')},9);self.assertIn('cargo_config_env_table_requires_whitelisted_effect_review',x['unresolved'])
    def test_incomplete_capture_does_not_leak_command_errors(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as d:
            repo=Path(d);(repo/'backend').mkdir();(repo/'backend/Cargo.toml').write_text('[workspace]\n')
            def fail(*args):raise CaptureError('SENTINEL_SECRET_COMMAND_STDERR_814')
            with patch(__name__+'.build_configuration', return_value={'effective_build_jobs':8,'build_configuration_sha256':'c'*64,'unresolved':['cargo_config_include_requires_explicit_resolution']}):
                x=capture(repo,repo,'before',runner=fail,env={})
            self.assertFalse(x['capture_complete']);self.assertIsNone(x['proof_values']['physical_device_id']);self.assertIsNone(x['proof_values']['build_jobs']);self.assertIsNone(x['proof_values']['build_configuration_sha256']);self.assertNotIn('SENTINEL_SECRET',json.dumps(x));self.assertIsNone(x['comparable'])
    def test_cli_missing_paths_returns_two_without_files(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);out=root/'out.json'
            proc=subprocess.run([sys.executable,__file__,'--repo',str(root/'missing'),'--target-dir',str(root/'missing-target'),'--output',str(out)],capture_output=True,text=True)
            self.assertEqual(proc.returncode,2);self.assertFalse(out.exists());self.assertNotIn('Traceback',proc.stderr)
    def test_default_jobs(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'Cargo.toml').write_text('[workspace]\n')
            x=build_configuration(root,{'CARGO_HOME':str(root/'none')},9);self.assertEqual(x['effective_build_jobs'],9)
    def test_include_is_incomplete(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/'Cargo.toml').write_text('[workspace]\n');(root/'.cargo').mkdir();(root/'.cargo/config.toml').write_text('include=["secret.toml"]\n')
            x=build_configuration(root,{'CARGO_HOME':str(root/'none')},8);self.assertIn('cargo_config_include_requires_explicit_resolution',x['unresolved']);self.assertNotIn('secret.toml',json.dumps(x))
    def test_missing_repo_target_and_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);repo=root/'repo';(repo/'backend').mkdir(parents=True);(repo/'backend/Cargo.toml').write_text('[workspace]');target=root/'target';target.mkdir();out=root/'out.json'
            with self.assertRaises((CaptureError,FileNotFoundError)):validate_paths(repo,root/'missing',out)
            with self.assertRaises((CaptureError,FileNotFoundError)):validate_paths(root/'missing',target,out)
            out.write_text('keep')
            with self.assertRaises(CaptureError):validate_paths(repo,target,out)
            self.assertEqual(out.read_text(),'keep')
    def test_df_rejects_network_and_failure(self):
        for raw in [b'',b'Filesystem 512-blocks Used Available Capacity Mounted on\nserver:/share 1 1 0 100% /x\n']:
            with self.assertRaises(CaptureError):parse_df(raw)
        def fail(*a):raise CaptureError('secret stderr omitted')
        with self.assertRaises(CaptureError):media(Path('/tmp'),'host',fail)
    def test_apfs_physical_chain_and_no_serial(self):
        calls=[]
        records={'/dev/disk3s5':{'VolumeUUID':'private-volume','APFSContainerReference':'disk3','APFSPhysicalStores':[{'APFSPhysicalStore':'disk0s2'}],'SerialNumber':'SECRET'},'/dev/disk0s2':{'DiskUUID':'private-store','ParentWholeDisk':'disk0'},'/dev/disk0':{'WholeDisk':True,'BusProtocol':'Apple Fabric','SolidState':True,'TotalSize':100,'SerialNumber':'SECRET'}}
        def fake(cmd,cwd):
            calls.append(cmd)
            if cmd[0]=='/bin/df':return b'Filesystem 512-blocks Used Available Capacity Mounted on\n/dev/disk3s5 100 20 80 20% /private-name\n'
            return plistlib.dumps(records[cmd[-1]])
        x=media(Path('/tmp'),'host',fake);text=json.dumps(x)
        for forbidden in ['SECRET','private-volume','private-store','private-name']:self.assertNotIn(forbidden,text)
        self.assertEqual(x['physical_stores'][0]['whole_disk_device'],'disk0');self.assertEqual(calls[0][0],'/bin/df')
        records['/dev/disk3s5']['APFSPhysicalStores']=[]
        with self.assertRaises(CaptureError):media(Path('/tmp'),'host',fake)
    def test_processes_only_exact_comm(self):
        x=process_observation(b'cargo\nrustc\nrustc\nld\nnode --token secret\ncargo --secret nope\n')
        self.assertEqual(x['cargo_count'],1);self.assertEqual(x['rustc_count'],2);self.assertNotIn('secret',json.dumps(x))
    def test_lock_ignores_workspace_and_hides_source(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'Cargo.lock';p.write_text('version=4\n[[package]]\nname="local"\nversion="0.1"\n[[package]]\nname="dep"\nversion="1.0"\nsource="git+https://secret@example.test/x"\n')
            a=lock_fingerprint(p);p.write_text(p.read_text().replace('name="local"','name="changed-local"'));b=lock_fingerprint(p)
            self.assertEqual(a['third_party_versions_sha256'],b['third_party_versions_sha256']);self.assertNotIn('secret',json.dumps(a))
    def test_exclusive_private_output_and_no_comparability(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/'x.json';ref=write_output(p,{'capture_complete':False,'comparable':None})
            self.assertEqual(ref['sha256'],digest(p.read_bytes()));self.assertEqual(p.stat().st_mode & 0o777,0o600)
            with self.assertRaises(FileExistsError):write_output(p,{'capture_complete':True})
            self.assertIsNone(json.loads(p.read_text())['comparable'])

    def input_fixture(self,repo,**overrides):
        from unittest.mock import patch
        backend=repo/'backend';backend.mkdir(parents=True,exist_ok=True)
        (backend/'Cargo.toml').write_text('[workspace]\n')
        (backend/'Cargo.lock').write_text('version=4\n[[package]]\nname="dep"\nversion="1"\nsource="registry+https://example.invalid"\n')
        def fake(cmd,cwd):
            if cmd[0]=='/usr/sbin/sysctl':return b'TestModel1,1\n' if cmd[-1]=='hw.model' else b'12\n'
            if cmd[0]=='/usr/sbin/ioreg':return plistlib.dumps([{'IOPlatformUUID':'PRIVATE_TEST_UUID'}])
            if cmd==['git','rev-parse','HEAD']:return b'0123456789012345678901234567890123456789\n'
            if cmd[0]=='git' or cmd[0]=='/bin/ps':return b''
            raise AssertionError('unexpected command')
        with patch(__name__+'.tools_observation',return_value={'fixture':True}),patch(__name__+'.media',return_value={'physical_device_id':'fixture-physical-id'}):
            return capture(repo,repo,'before',runner=fake,env={'CARGO_HOME':str(repo/'empty-cargo-home')},**overrides)

    def external_inputs_fixture(self,root):
        files={}
        for label,filename in [('measurement_script','measure.py'),('measurement_spec','probes.json'),('measurement_contract','contract.md')]:
            p=root/filename;p.write_text('input-'+label+'\n');files[label]=p
        return files

    def test_baseline_missing_defaults_with_explicit_external_inputs(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);repo=root/'baseline';files=self.external_inputs_fixture(root)
            x=self.input_fixture(repo,**files)
            self.assertTrue(x['capture_complete']);self.assertIsNone(x['comparable'])
            self.assertFalse((repo/'backend/scripts/measure-incremental.py').exists())
            for value in x['inputs'].values():
                self.assertEqual(value['selection'],'explicit_cli');self.assertEqual(value['location'],'external_to_measured_repo');self.assertFalse(value['repo_default_is_file']);self.assertEqual(value['sha256'],digest(Path(value['path']).read_bytes()))
            self.assertEqual(x['proof_values']['measurement_script_sha256'],digest(files['measurement_script'].read_bytes()))

    def test_missing_default_stays_missing_without_external_override(self):
        with tempfile.TemporaryDirectory() as d:
            repo=Path(d)/'baseline';x=self.input_fixture(repo)
            self.assertFalse(x['capture_complete']);self.assertIsNone(x['inputs']['measurement_script']);self.assertIsNone(x['proof_values']['measurement_script_sha256'])
            self.assertEqual(x['input_selection']['measurement_script']['selection'],'repo_default');self.assertFalse(x['input_selection']['measurement_script']['repo_default_is_file'])
            self.assertIn('measurement_script',[i['component'] for i in x['issues']]);self.assertFalse((repo/'backend/scripts').exists())

    def test_explicit_same_inputs_bind_same_sha_across_baseline_and_candidate(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);files=self.external_inputs_fixture(root);baseline=root/'baseline';candidate=root/'candidate'
            default=candidate/'backend/scripts/measure-incremental.py';default.parent.mkdir(parents=True);default.write_text('different candidate default')
            a=self.input_fixture(baseline,**files);b=self.input_fixture(candidate,**files)
            for label in MEASUREMENT_INPUTS:self.assertEqual(a['inputs'][label]['sha256'],b['inputs'][label]['sha256'])
            self.assertEqual(a['proof_values']['measurement_script_sha256'],b['proof_values']['measurement_script_sha256'])
            self.assertNotEqual(b['proof_values']['measurement_script_sha256'],digest(default.read_bytes()))
            files['measurement_script'].write_text('changed selected script')
            c=self.input_fixture(candidate,**files)
            self.assertNotEqual(b['proof_values']['measurement_script_sha256'],c['proof_values']['measurement_script_sha256'])

    def test_existing_default_and_explicit_symlink_real_location(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);repo=root/'repo';default=repo/'backend/scripts/measure-incremental.py';default.parent.mkdir(parents=True);default.write_text('real default')
            selected=select_measurement_inputs(repo);value=observe_measurement_input(selected['measurement_script'])
            self.assertEqual(value['selection'],'repo_default');self.assertEqual(value['location'],'measured_repo');self.assertTrue(value['repo_default_is_file'])
            external=root/'external.py';external.write_text('real external');link=repo/'tool-link.py';link.symlink_to(external)
            x=observe_measurement_input(select_measurement_inputs(repo,measurement_script=link)['measurement_script'])
            self.assertEqual(x['path'],str(external.resolve()));self.assertEqual(x['requested_path'],str(link));self.assertEqual(x['location'],'external_to_measured_repo')

    def test_cli_explicit_missing_input_returns_two_and_does_not_fallback(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);repo=root/'repo';default=repo/'backend/scripts/measure-incremental.py';default.parent.mkdir(parents=True);default.write_text('valid default');(repo/'backend/Cargo.toml').write_text('[workspace]\n')
            out=root/'out.json';missing=root/'SENTINEL_SECRET_MISSING_FILENAME'
            proc=subprocess.run([sys.executable,__file__,'--repo',str(repo),'--target-dir',str(root),'--output',str(out),'--measurement-script',str(missing)],capture_output=True,text=True)
            self.assertEqual(proc.returncode,2);self.assertFalse(out.exists());self.assertNotIn('SENTINEL_SECRET',proc.stderr);self.assertNotIn('Traceback',proc.stderr)
            with self.assertRaises(CaptureError):select_measurement_inputs(repo,measurement_script=root)

    def test_cli_forwards_all_three_selected_inputs(self):
        from unittest.mock import patch
        import contextlib,io
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);repo=root/'repo';(repo/'backend').mkdir(parents=True);(repo/'backend/Cargo.toml').write_text('[workspace]\n');files=self.external_inputs_fixture(root);out=root/'out.json'
            args=['--repo',str(repo),'--target-dir',str(root),'--output',str(out)]
            for flag,key in [('--measurement-script','measurement_script'),('--measurement-spec','measurement_spec'),('--measurement-contract','measurement_contract')]:args.extend([flag,str(files[key])])
            with patch(__name__+'.platform.system',return_value='Darwin'),patch(__name__+'.capture',return_value={'capture_complete':True,'comparable':None}) as mocked,contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(main(args),0)
            self.assertEqual(mocked.call_args.kwargs,files)

def main(argv=None):
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repo',type=Path);parser.add_argument('--target-dir',type=Path);parser.add_argument('--output',type=Path)
    parser.add_argument('--stage',choices=('before','after'));parser.add_argument('--self-test',action='store_true')
    parser.add_argument('--measurement-script',type=Path,help='Actual measurement script selected for the measurement CLI; defaults to repo/backend/scripts/measure-incremental.py')
    parser.add_argument('--measurement-spec',type=Path,help='Actual --spec input selected for the measurement CLI; defaults to the repo compile-probes.json')
    parser.add_argument('--measurement-contract',type=Path,help='Actual governing compile-measurement.md; defaults to the repo contract')
    args=parser.parse_args(argv)
    if args.self_test:
        result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SelfTests));return 0 if result.wasSuccessful() else 1
    if any(v is None for v in [args.repo,args.target_dir,args.output]):parser.error('--repo, --target-dir and --output are required')
    try:
        repo,target,output=validate_paths(args.repo,args.target_dir,args.output)
        if platform.system()!='Darwin':raise CaptureError('macos_required_for_physical_media_capture')
        obj=capture(repo,target,args.stage,measurement_script=args.measurement_script,measurement_spec=args.measurement_spec,measurement_contract=args.measurement_contract);ref=write_output(output,obj)
    except (CaptureError,OSError):
        print(json.dumps({'error':'invalid_input_or_output','details_not_emitted':True}),file=sys.stderr);return 2
    print(json.dumps(ref,ensure_ascii=False));return 0 if obj['capture_complete'] else 1
if __name__=='__main__':raise SystemExit(main())
