#!/usr/bin/env python3
"""Read-only stage-15 scope audit; never invokes Cargo or changes a source tree.

Usage:
  python3 audit-commerce15-scope.py --tree before00=/path/to/tree \
    --tree current=/path/to/tree --metadata before00=/tmp/metadata.json \
    --out /private/tmp/commerce15-scope.json

Rust lexical evidence separates nested comments, normal/raw/byte strings, character
literals, cfg(test) item ranges and out-of-line test modules. This is not a Rust
macro expander or type checker: unknown production references remain review items.
The JSON keeps every subprocess argv, exit code, stdout/stderr, per-file SHA256,
per-family classifications and Cargo manifest/optional supplied metadata evidence.
"""
from __future__ import annotations
import argparse
import bisect
import collections
import csv
import datetime
import hashlib
import json
import re
import shlex
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path

FAMILIES = {
    'CardInstance': r'card[_-]?instance',
    'MallOrder': r'mall[_-]?order',
    'MallAfterSales': r'mall[_-]?after[_-]?sales',
    'MallBackfill': r'mall[_-]?backfill',
    'ProductPublication': r'product[_-]?publication',
}
NARROW = re.compile('|'.join(FAMILIES.values()), re.I)
BROAD = re.compile(r'mall|commerce|card[_-]?instance|publication|card[_-]?balance[_-]?restored|商城', re.I)
COMMENT_KINDS = {'line_comment', 'block_comment'}
LITERAL_KINDS = {'string', 'raw_string', 'char'}
STABLE_NAMES = {'Mall', 'MallUser', 'MallActionRequest', 'CardBalanceRestored', 'MallConsumption'}

@dataclass
class Token:
    kind: str
    text: str
    start: int
    end: int


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def run(argv: list[str], cwd: Path) -> dict:
    p = subprocess.run(argv, cwd=cwd, capture_output=True, text=True, check=False)
    return {'cwd': str(cwd), 'argv': argv, 'command': shlex.join(argv),
            'exit_code': p.returncode, 'stdout': p.stdout, 'stderr': p.stderr}


def lex(s: str) -> list[Token]:
    """Tokenize enough Rust to keep comments/literals separate without regex masking."""
    out = []
    i, n = 0, len(s)
    while i < n:
        start = i
        if s[i].isspace():
            i += 1
            continue
        if s.startswith('//', i):
            end = s.find('\n', i)
            i = n if end < 0 else end
            out.append(Token('line_comment', s[start:i], start, i))
            continue
        if s.startswith('/*', i):
            i += 2
            depth = 1
            while i < n and depth:
                if s.startswith('/*', i): depth += 1; i += 2
                elif s.startswith('*/', i): depth -= 1; i += 2
                else: i += 1
            if depth: raise ValueError(f'unterminated block comment at {start}')
            out.append(Token('block_comment', s[start:i], start, i))
            continue
        raw = re.match(r'(?:br|cr|r)(#+)?"', s[i:])
        if raw:
            hashes = raw.group(1) or ''
            end = s.find('"' + hashes, i + raw.end())
            if end < 0: raise ValueError(f'unterminated raw string at {start}')
            i = end + 1 + len(hashes)
            out.append(Token('raw_string', s[start:i], start, i))
            continue
        prefix = re.match(r'(?:b|c)?"', s[i:])
        if prefix:
            i += prefix.end()
            while i < n:
                if s[i] == '\\': i += 2
                elif s[i] == '"': i += 1; break
                else: i += 1
            else: raise ValueError(f'unterminated string at {start}')
            out.append(Token('string', s[start:i], start, i))
            continue
        char = re.match(r"(?:b)?'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'", s[i:])
        if char:
            i += char.end()
            out.append(Token('char', s[start:i], start, i))
            continue
        ident = re.match(r'(?:r#)?[A-Za-z_][A-Za-z_0-9]*', s[i:])
        if ident:
            i += ident.end()
            out.append(Token('ident', s[start:i], start, i))
            continue
        i += 1
        out.append(Token('punct', s[start:i], start, i))
    return out


def pairs(tokens: list[Token]) -> dict[int, int]:
    result, stack = {}, []
    close = {')': '(', ']': '[', '}': '{'}
    for i, tok in enumerate(tokens):
        if tok.kind != 'punct': continue
        if tok.text in ('(', '[', '{'): stack.append((tok.text, i))
        elif tok.text in close:
            if not stack or stack[-1][0] != close[tok.text]:
                raise ValueError(f'unmatched {tok.text} at {tok.start}')
            _, begin = stack.pop(); result[begin] = i
    if stack: raise ValueError('unclosed delimiter')
    return result


def literal(tok: Token) -> str:
    x = tok.text
    if tok.kind == 'raw_string':
        first = x.index('"'); hashes = len(x) - len(x.rstrip('#'))
        return x[first+1:len(x)-1-hashes]
    if tok.kind == 'string':
        x = x[x.index('"'):]
        try: return json.loads(x)
        except json.JSONDecodeError: return x[1:-1]
    return x


class RustFile:
    def __init__(self, path: Path, root: Path):
        self.path, self.rel = path, str(path.relative_to(root))
        self.data = path.read_bytes(); self.text = self.data.decode('utf-8')
        self.tokens = lex(self.text)
        self.code = [t for t in self.tokens if t.kind not in COMMENT_KINDS]
        self.pairs = pairs(self.code)
        self.newlines = [m.start() for m in re.finditer('\n', self.text)]
        self.test_ranges = self.find_test_ranges()
        self.test_file = False
        self.external_modules = self.find_modules()

    def line(self, pos): return bisect.bisect_left(self.newlines, pos) + 1
    def is_test(self, pos):
        return self.test_file or any(start <= pos < end for start, end in self.test_ranges)

    def find_test_ranges(self):
        ranges, c = [], self.code
        for i in range(len(c)-2):
            if c[i].text != '#' or c[i+1].text != '[': continue
            end_attr = self.pairs.get(i+1)
            if end_attr is None: continue
            attr = [t.text for t in c[i+2:end_attr]]
            if attr != ['cfg', '(', 'test', ')']: continue
            j = end_attr + 1
            while j < len(c) and c[j].text == '#' and j+1 < len(c) and c[j+1].text == '[':
                j = self.pairs[j+1] + 1
            while j < len(c) and c[j].text not in ('{', ';'):
                if c[j].text in ('(', '['): j = self.pairs[j] + 1
                else: j += 1
            if j < len(c):
                end = self.pairs[j] if c[j].text == '{' else j
                ranges.append((c[i].start, c[end].end))
        return ranges

    def find_modules(self):
        found, c = [], self.code
        for i in range(len(c)-2):
            if c[i].text == 'mod' and c[i+1].kind == 'ident' and c[i+2].text == ';':
                name = c[i+1].text
                bases = [self.path.parent]
                if self.path.stem not in ('lib', 'main', 'mod'):
                    bases.insert(0, self.path.parent/self.path.stem)
                candidates = [p for b in bases for p in (b/(name+'.rs'), b/name/'mod.rs') if p.is_file()]
                # A #[path="..."] may precede the external declaration.
                prev = c[max(0, i-25):i]
                for k in range(len(prev)-2):
                    if prev[k].text == 'path' and prev[k+1].text == '=' and prev[k+2].kind in LITERAL_KINDS:
                        candidate = self.path.parent/literal(prev[k+2])
                        if candidate.is_file(): candidates = [candidate]
                found.append((c[i].start, candidates))
        return found

    def row(self, tok, category, **extra):
        return {'path': self.rel, 'line': self.line(tok.start), 'column': tok.start - (self.newlines[self.line(tok.start)-2] if self.line(tok.start)>1 else -1),
                'token_kind': tok.kind, 'token': tok.text, 'classification': category,
                'source_line': self.text.splitlines()[self.line(tok.start)-1], **extra}


def classify(file: RustFile, tok: Token) -> str:
    historical = '/tests/' in file.rel.split('/src/')[0] + '/' if '/src/' in file.rel else '/tests/' in file.rel
    if historical: return 'historical_tests_archive'
    if tok.kind in COMMENT_KINDS: return 'rust_comment'
    if file.is_test(tok.start): return 'cfg_test_fixture_or_assertion'
    if tok.kind == 'ident' and tok.text in {'find_publication_supplier_offering', 'find_publication_offering_revision', 'list_publication_offering_revisions'} and file.rel.endswith('/repository/supplier_offering/query.rs'):
        return 'existing_supply_publication_query'
    if tok.kind == 'ident' and tok.text == 'find_publication_sku_revision' and file.rel.endswith('/repository/catalog/sku.rs'):
        return 'existing_catalog_publication_query'
    if tok.kind == 'ident' and tok.text == 'WorkItemAllowedAction':
        return 'unrelated_identifier_substring'
    if tok.kind in LITERAL_KINDS and literal(tok) == 'uk_product_publication_revisions_publication_revision' and file.path.name == 'errors.rs':
        return 'legacy_error_index_compatibility'
    if tok.kind == 'ident' and tok.text in STABLE_NAMES: return 'stable_enum_value_or_reference'
    if tok.kind in LITERAL_KINDS:
        value = literal(tok)
        if value in {'MALL', 'mall_user', 'MALL_ACTION_REQUEST', 'CARD_BALANCE_RESTORED', 'mall_consumption'}:
            return 'stable_enum_wire_value'
        if value == 'mall_missing': return 'integration_registered_difference_code'
        if value == '供应已停止，商城在售发布已暂停' and file.path.name == 'presentation.rs':
            return 'existing_supply_workbench_display_text'
        if value in {'商城','商城用户','商城动作请求','商城消费','商城消费成本必填取值基础','商城消费成本范围已停用','卡券销售变更缺少原正式版本冻结的目标商城或应收到期日，禁止创建变更单','这单由商城开单，商业数据同步中，本系统只能查看；改内容请在商城处理。'} and any(x in file.rel for x in ('sales_order', 'sales_center', '/cost', 'source_registry', 'inbox_message')):
            return 'existing_domain_label_or_fail_closed_rule'
    return 'production_reference_requires_review'


def cargo_inventory(root: Path, supplied: Path|None):
    workspace = root/'backend/Cargo.toml'
    w = tomllib.loads(workspace.read_text())
    manifests = []
    for glob in w['workspace']['members']:
        for member in sorted((root/'backend').glob(glob)):
            path = member/'Cargo.toml'
            data = tomllib.loads(path.read_text())
            package = data.get('package', {})
            name = package.get('name')
            targets = []
            if 'lib' in data or (member/'src/lib.rs').is_file():
                lib = data.get('lib', {})
                targets.append({'kind': 'proc-macro' if lib.get('proc-macro',False) else 'lib', 'name': lib.get('name', name.replace('-', '_')), 'path': lib.get('path','src/lib.rs')})
            bins = data.get('bin', [])
            targets += [{'kind':'bin', 'name': b['name'], 'path':b.get('path','src/main.rs')} for b in bins]
            if package.get('autobins', True):
                if (member/'src/main.rs').is_file() and not any(t['path']=='src/main.rs' for t in targets):
                    targets.append({'kind':'bin','name':name,'path':'src/main.rs'})
                for p in sorted((member/'src/bin').glob('*.rs')) if (member/'src/bin').is_dir() else []:
                    if not any(t['name']==p.stem for t in targets): targets.append({'kind':'bin','name':p.stem,'path':str(p.relative_to(member))})
                for p in sorted((member/'src/bin').glob('*/main.rs')) if (member/'src/bin').is_dir() else []:
                    if not any(t['name']==p.parent.name for t in targets): targets.append({'kind':'bin','name':p.parent.name,'path':str(p.relative_to(member))})
            build = package.get('build', 'build.rs' if (member/'build.rs').is_file() else False)
            if build: targets.append({'kind':'custom-build','name':'build-script-build','path':build})
            manifests.append({'name':name,'manifest':str(path.relative_to(root)), 'sha256':sha(path.read_bytes()),
                'production_targets_static':targets,'autotests':package.get('autotests',True),'explicit_tests':data.get('test',[])})
    info = {'mode':'static_manifests_no_cargo_invocation','workspace_manifest_sha256':sha(workspace.read_bytes()),'packages':manifests}
    if supplied:
        doc = json.loads(supplied.read_text())
        meta = {'path':str(supplied),'sha256':sha(supplied.read_bytes()),'workspace_root':doc.get('workspace_root'),'commit':doc.get('commit')}
        meta['workspace_path_matches'] = Path(doc.get('workspace_root','/')).resolve() == (root/'backend').resolve()
        if doc.get('packages') and isinstance(doc['packages'][0],str):
            meta.update({'kind':'measurement_summary_no_target_records','package_names':doc['packages']})
        else:
            members = set(doc.get('workspace_members',[]))
            packages = [p for p in doc.get('packages',[]) if not members or p['id'] in members]
            meta.update({'kind':'cargo_metadata','packages':[{'name':p['name'],'manifest_path':p['manifest_path'],'targets':p['targets']} for p in packages]})
        info['supplied_metadata']=meta
    else: info['supplied_metadata']={'kind':'not_supplied','target_execution_evidence':'not_collected'}
    return info


def audit_tree(label: str, root: Path, metadata: Path|None):
    root = root.resolve()
    commands = []
    def command(argv):
        result=run(argv,root);commands.append(result);return result
    head = command(['git','rev-parse','HEAD'])['stdout'].strip()
    status = command(['git','status','--porcelain=v1','--untracked-files=all'])
    input_files = sorted((root/'backend').glob('**/Cargo.toml'))
    input_files = [p for p in input_files if 'target' not in p.parts and '.git' not in p.parts]
    input_files += [p for p in [root/'backend/Cargo.lock', root/'backend/docs/superpowers/plans/domain-crate-migration/15-commerce-scope.md'] if p.is_file()]
    inputs_at_start = {str(p.relative_to(root)):sha(p.read_bytes()) for p in input_files}
    file_list=command(['rg','--files','--hidden','--no-ignore','backend','-g','*.rs','-g','!**/target/**','-g','!**/.git/**'])
    if file_list['exit_code']!=0: raise RuntimeError(file_list)
    common=['rg','-n','--hidden','--no-ignore','--no-heading','--color','never']
    command(common+['CardInstance|MallOrder|MallAfterSales|MallBackfill|ProductPublication','backend','-g','*.rs','-g','!**/target/**'])
    command(common+['-i',NARROW.pattern,'backend','-g','*.rs','-g','!**/target/**'])
    command(common+['-i',BROAD.pattern,'backend','-g','*.rs','-g','!**/target/**'])
    command(common+['-i',NARROW.pattern,'backend','-g','Cargo.toml','-g','!**/target/**'])
    command(common+['-i',BROAD.pattern,'backend/apps/web-api/src/core/routes','backend/apps/web-api/src/app_state.rs','backend/apps/web-api/src/lib.rs','backend/apps/web-api/src/main.rs'])
    files={}; errors=[]
    for rel in sorted(file_list['stdout'].splitlines()):
        try:
            f=RustFile(root/rel,root);files[f.path.resolve()]=f
        except (OSError,UnicodeError,ValueError) as error:
            errors.append({'path':rel,'error':str(error)})
    # Propagate cfg(test) through Rust out-of-line module declarations, not include_str!.
    changed=True
    while changed:
        changed=False
        for f in files.values():
            for pos,candidates in f.external_modules:
                if f.is_test(pos):
                    for p in candidates:
                        target=files.get(p.resolve())
                        if target and not target.test_file: target.test_file=True;changed=True
    hits=[]; declarations=[]; ids=[]; routes=[]; collections_calls=[]; constants=[]; retained_types=[]
    for f in files.values():
        c=f.code
        for t in f.tokens:
            if BROAD.search(t.text): hits.append(f.row(t,classify(f,t),families=[k for k,v in FAMILIES.items() if re.search(v,t.text,re.I)]))
        for i,t in enumerate(c):
            prod=not f.is_test(t.start) and '/tests/' not in f.rel
            if t.text in ('struct','enum','trait','type','union') and i+1<len(c) and c[i+1].kind=='ident':
                name=c[i+1].text
                if name in {'SalesOrder','BusinessType','OriginSystem','VoucherLineFields','ReceivableFundsReview','ReceivableAccount','SupplierOffering','SupplierFulfillmentOrder','SupplierSettlementStatement','MessageType','SourceSystemType','ExternalObjectType','CostScope','DocumentType'}:
                    retained_types.append(f.row(c[i+1],'existing_domain_type',declaration_kind=t.text))
                if NARROW.search(name): declarations.append(f.row(c[i+1],'production_type_declaration' if prod else 'test_type_declaration',declaration_kind=t.text))
            if t.text=='id_type' and i+3<len(c) and c[i+1].text=='!' and c[i+2].text=='(':
                ids.append(f.row(c[i+3],'production_id_newtype' if prod else 'test_id_newtype'))
            if t.text in ('const','static') and i+1<len(c):
                end=next((j for j in range(i+1,len(c)) if c[j].text==';'),min(i+80,len(c)))
                values=[literal(x) for x in c[i:end] if x.kind in ('string','raw_string')]
                if prod:
                    constants.append({'path':f.rel,'line':f.line(t.start),'name':c[i+1].text,'literal_values':values})
            if prod and i>0 and c[i-1].text=='.' and t.text in ('route','nest','route_service','nest_service','collection','create_collection'):
                j=i+1
                while j<len(c) and j<i+100 and c[j].text not in ('(', ';','{'): j+=1
                if j<len(c) and c[j].text=='(' and j in f.pairs:
                    end=f.pairs[j]
                    args=f.text[c[j].start:c[end].end]
                    event={'path':f.rel,'line':f.line(t.start),'call':t.text,'arguments':args,'families':[k for k,v in FAMILIES.items() if re.search(v,args,re.I)],'broad_commerce_match':bool(BROAD.search(args))}
                    (collections_calls if 'collection' in t.text else routes).append(event)
    cargo=cargo_inventory(root,metadata)
    families={}
    for name in FAMILIES:
        related=[h for h in hits if name in h['families']]
        families[name]={'hits':related,'classification_counts':dict(collections.Counter(h['classification'] for h in related)),
          'type_declarations':[d for d in declarations if re.search(FAMILIES[name],d['token'],re.I)],
          'id_newtypes':[d for d in ids if re.search(FAMILIES[name],d['token'],re.I)],
          'active_collection_calls':[r for r in collections_calls if name in r['families']],
          'route_calls':[r for r in routes if name in r['families']],
          'cargo_package_matches':[p['name'] for p in cargo['packages'] if re.search(FAMILIES[name],p['name'],re.I)],
          'cargo_target_matches':[dict(t,package=p['name']) for p in cargo['packages'] for t in p['production_targets_static'] if re.search(FAMILIES[name],t['name']+' '+t['path'],re.I)],
          'repository_or_factory_references':[h for h in related if h['token_kind']=='ident' and re.search(r'Repository|Service|Factory|Ext',h['token']) and h['classification'] not in ('rust_comment','cfg_test_fixture_or_assertion','historical_tests_archive')]}
    mapping={}
    for filename,key in [('source-map.tsv','phase'),('repository-types.tsv','owner_phase')]:
        p=root/'backend/docs/superpowers/plans/domain-crate-migration'/filename
        if p.exists():
            rows=list(csv.DictReader(p.read_text().splitlines(),delimiter='\t'))
            mapping[filename]={'sha256':sha(p.read_bytes()),'phase15_rows':[r for r in rows if r.get(key)=='15']}
    manifests=[{'path':f.rel,'sha256':sha(f.data),'bytes':len(f.data),'test_file_via_cfg_module':f.test_file} for f in files.values()]
    production_unknown=[h for h in hits if h['classification']=='production_reference_requires_review']
    active=[d for d in declarations if d['classification']=='production_type_declaration']+[r for r in routes+collections_calls if r['broad_commerce_match']]
    file_list_end=command(['rg','--files','--hidden','--no-ignore','backend','-g','*.rs','-g','!**/target/**','-g','!**/.git/**'])
    end_paths=set(file_list_end['stdout'].splitlines());start_paths={f.rel for f in files.values()}
    changed_files=[f.rel for f in files.values() if not f.path.is_file() or sha(f.path.read_bytes())!=sha(f.data)]
    changed_inputs=[path for path,digest in inputs_at_start.items() if not (root/path).is_file() or sha((root/path).read_bytes())!=digest]
    head_at_end=command(['git','rev-parse','HEAD'])['stdout'].strip()
    input_errors=[{'command':c['command'],'exit_code':c['exit_code'],'stderr':c['stderr']} for c in commands if (c['argv'][0]=='rg' and c['exit_code'] not in (0,1)) or (c['argv'][0]=='git' and c['exit_code']!=0)]
    if metadata and not cargo['supplied_metadata']['workspace_path_matches']:
        input_errors.append({'metadata':str(metadata),'error':'supplied metadata workspace_root does not match audited backend'})
    snapshot_changes={'changed_or_removed':changed_files,'added':sorted(end_paths-start_paths),'removed':sorted(start_paths-end_paths),'changed_manifests_lock_or_plan':changed_inputs,'head_changed':[head,head_at_end] if head!=head_at_end else []}
    snapshot_stable=not any(snapshot_changes.values())
    return {'label':label,'tree':str(root),'head_at_start':head,'git_status':status['stdout'],'commands':commands,
       'snapshot_stable':snapshot_stable,'snapshot_changes':snapshot_changes,'input_errors':input_errors,'input_file_sha256':inputs_at_start,'retained_domain_types':retained_types,'lexical_errors':errors,'rust_files':manifests,'source_inventory_sha256':sha(json.dumps(manifests,sort_keys=True).encode()),
       'families':families,'broad_hits':hits,'classification_counts':dict(collections.Counter(h['classification'] for h in hits)),
       'all_id_macros':ids,'all_route_calls':routes,'all_collection_calls':collections_calls,'production_string_constants':constants,
       'production_unclassified':production_unknown,'commerce_implementation_candidates':active,'scope_mapping':mapping,
       'cargo':cargo,'head_at_end':head_at_end,
       'scope_result':'REQUIRES_MANUAL_REVIEW' if active or production_unknown or errors or input_errors or not snapshot_stable else 'NO_MATCHING_IMPLEMENTATION_IN_LEXICAL_SCOPE'}


def self_test():
    src='''// struct MallOrder { }\n/* outer /* enum CardInstance {} */ end */\nconst TEXT: &str = r##"struct MallOrder { // raw }"##;\nconst BYTE: &[u8] = b"/* not comment */";\nfn lifetime<'a>(x: &'a str) { let c = '}'; }\n#[cfg(test)] mod tests { fn fixture() { let s = "mall_order"; } }\npub struct ExistingSales;\npub struct MallOrder;\n#[cfg(test)] struct MallOrderFixture;\nid_type!(MallOrderId);\n'''
    tokens=lex(src); code=[t for t in tokens if t.kind not in COMMENT_KINDS]; pairs(code)
    assert sum(t.kind=='block_comment' for t in tokens)==1
    assert sum(t.kind=='raw_string' for t in tokens)==1
    assert sum(t.kind=='char' for t in tokens)==1
    assert [code[i+1].text for i,t in enumerate(code[:-1]) if t.text=='struct']==['ExistingSales','MallOrder','MallOrderFixture']
    # Test range extraction without filesystem writes.
    f=object.__new__(RustFile); f.code=code; f.pairs=pairs(code)
    ranges=f.find_test_ranges(); assert len(ranges)==2
    fixture=next(t for t in tokens if t.text=='"mall_order"')
    assert ranges[0][0]<=fixture.start<ranges[0][1]
    production_name=next(t for t in code if t.text=='MallOrder')
    test_name=next(t for t in code if t.text=='MallOrderFixture')
    assert not any(a<=production_name.start<b for a,b in ranges)
    assert any(a<=test_name.start<b for a,b in ranges)
    return {'production_type_positive_control':True,'test_type_positive_control':True,'nested_comments':True,'raw_strings':True,'byte_strings':True,'char_vs_lifetime':True,'balanced_cfg_test_scope':True,'no_source_writes':True}


def keypaths(values):
    out={}
    for value in values:
        key, sep, path=value.partition('=')
        if not sep or not key: raise ValueError('expected label=/absolute/path')
        out[key]=Path(path).resolve()
    return out


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tree',action='append',required=True,help='label=/absolute/worktree')
    parser.add_argument('--metadata',action='append',default=[],help='matching-label=/supplied/metadata.json')
    parser.add_argument('--out',type=Path,required=True,help='JSON path under /tmp or /private/tmp')
    args=parser.parse_args();trees=keypaths(args.tree);metadata=keypaths(args.metadata)
    output=args.out.resolve()
    if not output.is_relative_to(Path('/private/tmp')) and not output.is_relative_to(Path('/tmp').resolve()):
        parser.error('--out must be under /tmp; source trees are read-only')
    for tree in trees.values():
        if output.is_relative_to(tree): parser.error('--out cannot be inside an audited source tree')
    result={'schema_version':1,'python_version':sys.version,'generated_at_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'script_path':str(Path(__file__).resolve()),'script_sha256':sha(Path(__file__).read_bytes()),
        'lexer_self_test':self_test(),'limitations':['No Cargo invocation, cfg evaluation, Rust macro expansion or database execution.',
        'Only exact cfg(test) positive attributes and their out-of-line modules are classified as tests; other cfg combinations remain production review candidates.',
        'Collection arguments and all string constants are recorded; dynamic expressions are not evaluated.',
        'Scope evidence does not assert stage acceptance; public quality gates must be recorded separately.'],
        'trees':[audit_tree(label,root,metadata.get(label)) for label,root in trees.items()]}
    output.parent.mkdir(parents=True,exist_ok=True);output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n')
    print(json.dumps({'out':str(output),'sha256':sha(output.read_bytes()),'trees':[{'label':t['label'],'rust_files':len(t['rust_files']),'result':t['scope_result'],'lex_errors':len(t['lexical_errors']),'unclassified':len(t['production_unclassified']),'candidates':len(t['commerce_implementation_candidates'])} for t in result['trees']]},ensure_ascii=False))
    return 2 if any(t['lexical_errors'] or t['input_errors'] or t['commerce_implementation_candidates'] or t['production_unclassified'] or not t['snapshot_stable'] for t in result['trees']) else 0

if __name__=='__main__':
    sys.exit(main())
