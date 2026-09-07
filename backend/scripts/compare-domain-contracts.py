#!/usr/bin/env python3
"""Read-only static contract evidence for domain migrations 09–17 (draft).

Python 3.11+; no third-party modules, Cargo execution, MongoDB, or source writes.
Exit 0: captured facts equal; 1: captured contract drift/missing facts;
2: incomplete/ambiguous capture requires review. Static equality is not runtime
proof. All comparisons ignore source file relocation and retain both origins.
"""
from __future__ import annotations

import argparse
from collections import defaultdict
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import tomllib
import unittest

DISCLAIMER = "真实数据库运行未验证"
LIMITS = [
    "Static source inventory and bounded lexical extraction; not a complete Rust parser.",
    "Workspace members are read from Cargo.toml without resolving features or expanding macros.",
    "Package src trees are inventoried; per-target module reachability is not proven.",
    "DTO inventory includes serialized internal types; tuple/newtype and macro-generated wire shapes may require review.",
    "Collection candidates include literal repository/index constants and literal collection calls; authority/registration needs review.",
    "Qualified type paths are reduced to terminal symbols for relocation comparison; type identity needs review.",
    "Index descriptions are source declarations, not MongoDB catalog inspection or index execution.",
    "Equal source hashes do not prove the same Executor, write ordering, rollback, or external-I/O separation.",
    "HTTP permission artifact is read as-is; this script does not regenerate it or prove freshness.",
]


def digest(value: bytes | str) -> str:
    return hashlib.sha256(value.encode() if isinstance(value, str) else value).hexdigest()


def stable(value) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


@dataclass(frozen=True)
class Token:
    value: str
    line: int


def lex(text: str) -> list[Token]:
    """Tokenize balanced punctuation while preserving literals and dropping comments."""
    out, i, line = [], 0, 1
    while i < len(text):
        start, start_line = i, line
        if text[i].isspace():
            i += 1
        elif text.startswith("//", i):
            end = text.find("\n", i)
            i = len(text) if end < 0 else end
        elif text.startswith("/*", i):
            i, depth = i + 2, 1
            while i < len(text) and depth:
                if text.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif text.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    i += 1
        else:
            raw = re.match(r'(?:br|r)(#*)"', text[i:])
            quoted = re.match(r'''(?:b)?'(?:\\.|[^'\\])'|(?:b)?"(?:\\.|[^"\\])*"''', text[i:], re.S)
            word = re.match(r"[A-Za-z_][A-Za-z_0-9]*|[0-9][A-Za-z_0-9.]*|::|->|=>|&&|\|\|", text[i:])
            if raw:
                close = '"' + raw.group(1)
                end = text.find(close, i + raw.end())
                i = len(text) if end < 0 else end + len(close)
            elif quoted:
                i += quoted.end()
            elif word:
                i += word.end()
            else:
                i += 1
            out.append(Token(text[start:i], start_line))
        line += text[start:i].count("\n")
    return out


def values(tokens) -> list[str]:
    return [token.value for token in tokens]


def canonical(tokens, leaf_paths: bool = False) -> str:
    vals, normalized, i = values(tokens), [], 0
    while i < len(vals):
        if leaf_paths and re.fullmatch(r"[A-Za-z_]\w*", vals[i]):
            while i + 2 < len(vals) and vals[i + 1] == "::" and re.fullmatch(r"[A-Za-z_]\w*", vals[i + 2]):
                i += 2
        normalized.append(vals[i])
        i += 1
    return " ".join(normalized)


def closing(tokens, start: int) -> int:
    pairs = {"{": "}", "[": "]", "(": ")"}
    if tokens[start].value not in pairs:
        raise ValueError("opening delimiter required")
    stack = [pairs[tokens[start].value]]
    for i in range(start + 1, len(tokens)):
        value = tokens[i].value
        if value in pairs:
            stack.append(pairs[value])
        elif value in pairs.values():
            if not stack or value != stack.pop():
                raise ValueError("unbalanced delimiters")
            if not stack:
                return i
    raise ValueError("unclosed delimiter")


def split_top(tokens, delimiter=",", angle=False):
    output, start, stack = [], 0, []
    pairs = {"{": "}", "[": "]", "(": ")"}
    if angle:
        pairs["<"] = ">"
    for i, token in enumerate(tokens):
        value = token.value
        if value in pairs:
            stack.append(pairs[value])
        elif stack and value == stack[-1]:
            stack.pop()
        elif value == delimiter and not stack:
            output.append(tokens[start:i])
            start = i + 1
    if tokens[start:]:
        output.append(tokens[start:])
    return output


def production(tokens):
    """Exclude entire explicit cfg(test) modules and annotated test functions."""
    result, i = [], 0
    while i < len(tokens):
        if values(tokens[i:i + 2]) == ["#", "["]:
            end = closing(tokens, i + 1)
            attr = canonical(tokens[i + 2:end])
            if attr in {"cfg ( test )", "test", "tokio :: test"}:
                j = end + 1
                while j < len(tokens) and tokens[j].value not in {"{", ";"}:
                    j += 1
                i = closing(tokens, j) + 1 if j < len(tokens) and tokens[j].value == "{" else j + 1
                continue
        result.append(tokens[i])
        i += 1
    return result


def attrs_before(tokens, item: int):
    """Collect attributes immediately preceding an item, allowing visibility/modifiers."""
    start = item - 1
    while start >= 0 and tokens[start].value not in {";", "{", "}"}:
        start -= 1
    prefix, attrs, i = tokens[start + 1:item], [], 0
    while i < len(prefix):
        if values(prefix[i:i + 2]) == ["#", "["]:
            end = closing(prefix, i + 1)
            attrs.append(canonical(prefix[i + 2:end]))
            i = end + 1
        else:
            i += 1
    return attrs


def declarations(tokens, kind):
    """Yield named item headers/bodies and ranges, without expanding macros."""
    for i, token in enumerate(tokens):
        if token.value != kind or i + 1 >= len(tokens):
            continue
        if not re.fullmatch(r"[A-Za-z_]\w*", tokens[i + 1].value):
            continue
        j = i + 2
        while j < len(tokens) and tokens[j].value not in {"{", ";"}:
            if tokens[j].value in {"(", "["}:
                j = closing(tokens, j)
            j += 1
        if j < len(tokens) and tokens[j].value == "{":
            end = closing(tokens, j)
            yield {"name": tokens[i + 1].value, "start": i, "end": end,
                   "line": token.line, "header": tokens[i:j], "body": tokens[j + 1:end],
                   "attrs": attrs_before(tokens, i)}


def calls(tokens, name):
    for i in range(len(tokens) - 1):
        if tokens[i].value == name and tokens[i + 1].value == "(":
            if i and tokens[i - 1].value == "fn":
                continue
            end = closing(tokens, i + 1)
            yield i, end, split_top(tokens[i + 2:end])


def literal(tokens):
    if not tokens:
        return None
    value = tokens[0].value
    if value.startswith('"'):
        try:
            return json.loads(value)
        except json.JSONDecodeError:
            return value[1:-1]
    return None


def source(path, repo, line=None):
    item = {"path": path.relative_to(repo).as_posix()}
    if line is not None:
        item["line"] = line
    return item


def add(records, symbol, contract, origin):
    records.setdefault(symbol, []).append({"contract": contract, "source": origin})


def issue(out, category, message, origin=None):
    out.append({"status": "needs_review", "category": category, "message": message, "source": origin})


def workspace_sources(repo: Path, reviews):
    """Read active member manifests only; never invoke Cargo or mutate lockfiles."""
    backend = repo / "backend" if (repo / "backend/Cargo.toml").exists() else repo
    manifest = backend / "Cargo.toml"
    config = tomllib.loads(manifest.read_text())
    workspace = config.get("workspace", {})
    members = []
    for pattern in workspace.get("members", ["."]):
        members.extend(backend.glob(pattern))
    excludes = {p.resolve() for pattern in workspace.get("exclude", []) for p in backend.glob(pattern)}
    packages, paths = [], set()
    for directory in sorted(set(members)):
        cargo = directory / "Cargo.toml"
        if directory.resolve() in excludes or not cargo.is_file():
            continue
        data = tomllib.loads(cargo.read_text())
        package = data.get("package", {})
        packages.append({"name": package.get("name"), "manifest": source(cargo, repo)["path"]})
        if package.get("name") == "test-support":
            continue
        for path in (directory / "src").rglob("*.rs"):
            if "tests" not in path.parts and path.stem not in {"tests", "test_fixture", "test_fixtures"} and not path.stem.endswith("_tests"):
                paths.add(path)
        for target in [data.get("lib", {})] + data.get("bin", []):
            if target.get("path"):
                path = directory / target["path"]
                if path.is_file() and "tests" not in path.parts:
                    paths.add(path)
        build = package.get("build", "build.rs")
        if build is not False and (directory / str(build)).is_file():
            paths.add(directory / str(build))
    if not paths:
        issue(reviews, "workspace", "No active member Rust sources discovered", source(manifest, repo))
    return backend, packages, sorted(paths)


def capture_dtos(tokens, origin, records, reviews):
    for kind in ("struct", "enum"):
        for item in declarations(tokens, kind):
            attrs = item["attrs"]
            is_dto_path = "dto" in Path(origin["path"]).parts or Path(origin["path"]).stem == "dto"
            if not is_dto_path and not any("Serialize" in a or "Deserialize" in a for a in attrs):
                continue
            body = item["body"]
            fields = []
            for part in split_top(body, angle=True):
                if not part:
                    continue
                raw = canonical(part, leaf_paths=True)
                # Strip ordinary docs in lexer; retain serde and field type tokens.
                raw = re.sub(r"# \[ (?!serde\b)[^\]]*\]", "", raw).strip()
                fields.append(raw)
            contract = {"kind": kind, "serde": [a for a in attrs if a.startswith("serde ")], "fields_or_variants": fields}
            if any(a.startswith("cfg") for a in attrs):
                issue(reviews, "dto", f"Conditional serialized item {item['name']}", {**origin, "line": item["line"]})
            add(records, item["name"], contract, {**origin, "line": item["line"]})
    captured_struct_lines = {item["line"] for item in declarations(tokens, "struct")}
    for i, token in enumerate(tokens):
        if token.value == "struct" and token.line not in captured_struct_lines and any("Serialize" in attr or "Deserialize" in attr for attr in attrs_before(tokens, i)):
            issue(reviews, "dto", f"Tuple/unit serialized struct {tokens[i + 1].value} needs review", {**origin, "line": token.line})
    for name in ("serialize", "deserialize"):
        if any(token.value == name for token in tokens) and "dto" in origin["path"] and not any(token.value in {"struct", "enum"} for token in tokens):
            issue(reviews, "dto", "DTO aliases/macros/custom serde need manual coverage review", origin)


def match_arms(tokens):
    for i, token in enumerate(tokens):
        if token.value != "match":
            continue
        j = i + 1
        while j < len(tokens) and tokens[j].value != "{":
            j += 1
        if j == len(tokens):
            return []
        end, result = closing(tokens, j), []
        for arm in split_top(tokens[j + 1:end]):
            vals = values(arm)
            if "=>" in vals:
                arrow = vals.index("=>")
                result.append({"pattern": canonical(arm[:arrow], True), "expression": canonical(arm[arrow + 1:], True)})
        return result
    return []


def capture_errors(tokens, origin, records):
    path = origin["path"]
    if not ("error" in Path(path).stem or "/errors/" in path):
        return
    for item in declarations(tokens, "enum"):
        if "Error" in item["name"]:
            # New domain Error clones are reported, not treated as HTTP runtime behavior.
            for fn in declarations(tokens, "fn"):
                if fn["name"] in {"as_str", "class", "http_status", "error_code", "retryable"}:
                    identity = f"{item['name']}::{fn['name']}"
                    if "/apps/web-api/" in f"/{path}":
                        identity = "http::" + identity
                    body = canonical(fn["body"], leaf_paths=True)
                    add(records, identity, {"body": body, "match_arms": match_arms(fn["body"])}, {**origin, "line": fn["line"]})
            if item["name"].endswith("ErrorCode"):
                add(records, item["name"], {"variants": canonical(item["body"], leaf_paths=True)}, {**origin, "line": item["line"]})


def collect_constants(tokens, origin, constants):
    for i, token in enumerate(tokens):
        if token.value != "const" or i + 2 >= len(tokens):
            continue
        j = i + 2
        while j < len(tokens) and tokens[j].value not in {";", "{"}:
            j += 1
        part = tokens[i:j]
        vals = values(part)
        if "=" in vals and "str" in vals:
            rhs = part[vals.index("=") + 1:]
            value = literal(rhs)
            if value is not None:
                constants.setdefault(tokens[i + 1].value, set()).add(value)


def capture_indexes(tokens, origin, constants, records, collections, reviews):
    if "indexes" not in Path(origin["path"]).parts:
        return
    functions = {item["name"]: item for item in declarations(tokens, "fn")}
    constants = {name: set(vals) for name, vals in constants.items()}
    for i, token in enumerate(tokens):
        if token.value != "const" or i + 1 >= len(tokens):
            continue
        j = i + 2
        while j < len(tokens) and tokens[j].value != ";":
            j += 1
        if j and tokens[j - 1].value in constants:
            constants[tokens[i + 1].value] = set(constants[tokens[j - 1].value])
    fn_collections = defaultdict(set)
    for constant, function in re.findall(r"\. collection (?::: < [^>]+ > )?\( ([A-Z_][A-Z_0-9]*) \) \. create_indexes \( ([A-Za-z_]\w*) \(", canonical(tokens)):
        fn_collections[function].update(constants.get(constant, set()))
    for _, _, args in calls(tokens, "create_indexes"):
        if len(args) == 3 and args[1] and args[2]:
            candidates = constants.get(args[1][-1].value, set())
            fn_collections[args[2][0].value].update(candidates)
    for name, fn in functions.items():
        fn_body = fn["body"]
        # Helpers have dynamic names. Their call sites provide concrete declarations.
        for helper_name, helper in functions.items():
            hbody = canonical(helper["body"])
            if "IndexModel :: builder" not in hbody or ". name ( name" not in hbody:
                continue
            unique = ". unique ( true )" in hbody
            if ". unique (" in hbody and not unique and ". unique ( false )" not in hbody:
                unique = None
            options = []
            for option in ("partial_filter_expression", "collation", "sparse", "expire_after"):
                for _, _, option_args in calls(helper["body"], option):
                    options.append({"name": option, "arguments": [canonical(arg, True) for arg in option_args]})
            for start, _, args in calls(fn_body, helper_name):
                index_name = literal(args[0]) if args else None
                if not index_name or len(args) < 2:
                    continue
                key_tokens = args[1]
                colls = sorted(fn_collections.get(name, []))
                contract = {"name": index_name, "collections": colls,
                            "keys": canonical(key_tokens, leaf_paths=True), "unique": unique,
                            "options": options, "extra_arguments": [canonical(a, True) for a in args[2:]]}
                add(records, index_name, contract, {**origin, "line": fn_body[start].line})
                if not colls or unique is None:
                    issue(reviews, "indexes", f"Unresolved collection/uniqueness for {index_name}", origin)
        # Direct builders, including options supplied through a local zero-arg function.
        for start, token in enumerate(fn_body):
            if values(fn_body[start:start + 4]) != ["IndexModel", "::", "builder", "("]:
                continue
            end = start + 3
            while end < len(fn_body):
                if fn_body[end].value in {"(", "{", "["}:
                    end = closing(fn_body, end) + 1
                elif fn_body[end].value in {",", ";"}:
                    break
                else:
                    end += 1
            chain = fn_body[start:end]
            key_args = list(calls(chain, "keys"))
            option_args = list(calls(chain, "options"))
            if not key_args or not option_args:
                continue
            options_tokens = option_args[0][2][0]
            if options_tokens and options_tokens[0].value in functions:
                options_tokens = functions[options_tokens[0].value]["body"]
            names = list(calls(options_tokens, "name"))
            index_name = literal(names[0][2][0]) if names and names[0][2] else None
            if not index_name:
                # Dynamic helper definitions were resolved at their concrete call sites.
                if name not in {h for h, f in functions.items() if ". name ( name" in canonical(f["body"])}:
                    issue(reviews, "indexes", f"Dynamic index builder in {name}", {**origin, "line": token.line})
                continue
            unique_calls = list(calls(options_tokens, "unique"))
            unique_raw = canonical(unique_calls[0][2][0]) if unique_calls else "false"
            colls = sorted(fn_collections.get(name, []))
            contract = {"name": index_name, "collections": colls,
                        "keys": canonical(key_args[0][2][0], True),
                        "unique": {"true": True, "false": False}.get(unique_raw),
                        "options_source_tokens": canonical(options_tokens, True)}
            add(records, index_name, contract, {**origin, "line": token.line})
            if not colls:
                issue(reviews, "indexes", f"Unresolved collection for direct index {index_name}", origin)
    # Record unresolved literal names, rather than silently dropping unsupported builders.
    for _, _, args in calls(tokens, "name"):
        name = literal(args[0]) if args else None
        if name and name not in records:
            issue(reviews, "indexes", f"Uncaptured named index {name}", origin)


def source_contracts(tokens, origin, raw, records):
    joined = canonical(tokens)
    symbols = []
    for kind in ("struct", "enum", "trait", "fn"):
        symbols.extend((kind, item) for item in declarations(tokens, kind))
    signals = {
        "money": bool(re.search(r"\b(?:Amount|Quantity|UnitPrice|Rate)\b", joined)) and ("Decimal128" in joined or "SCALE" in joined),
        "idempotency": any(s in joined for s in ("fingerprint", "Fingerprint", "sha256-v1", "canonical_payload")),
        "executor": any(kind == "trait" and item["name"] == "Executor" for kind, item in symbols),
        "transaction": any(s in joined for s in ("COMMIT_RETRY_TIMEOUT", "with_transaction", "run_audited")),
    }
    for category, selected in signals.items():
        if not selected:
            continue
        relevant = []
        for kind, item in symbols:
            body = canonical(item["body"], True)
            if category == "money" or category == "executor" or any(s in (item["name"] + " " + body) for s in ("fingerprint", "Fingerprint", "sha256", "with_transaction", "run_audited", "commit_transaction", "abort_transaction")):
                relevant.append({"symbol": f"{kind}::{item['name']}", "token_sha256": digest(canonical(item['header'] + item['body'], True)), "line": item["line"]})
        add(records, category, {"source_sha256": digest(raw), "production_token_sha256": digest(joined), "symbols": relevant}, origin)


def capture(repo: Path):
    reviews = []
    backend, packages, paths = workspace_sources(repo, reviews)
    result = {"repo": str(repo), "packages": packages, "source_count": len(paths),
              "http": {}, "collections": {}, "indexes": {}, "dto": {}, "errors": {},
              "foundations": {}, "needs_review": reviews, "source_inventory": []}
    inventory, constants = [], {}
    for path in paths:
        origin = source(path, repo)
        try:
            raw = path.read_bytes()
            tokens = production(lex(raw.decode("utf-8")))
        except (OSError, ValueError) as exc:
            issue(reviews, "source_parse", f"Source unavailable or unsupported: {exc}", origin)
            continue
        inventory.append((tokens, origin, raw))
        result["source_inventory"].append({**origin, "sha256": digest(raw)})
        for i, token in enumerate(tokens):
            if token.value != "collection":
                continue
            j = i + 1
            if values(tokens[j:j + 2]) == ["::", "<"]:
                j, depth = j + 2, 1
                while j < len(tokens) and depth:
                    depth += (tokens[j].value == "<") - (tokens[j].value == ">")
                    j += 1
            if j < len(tokens) and tokens[j].value == "(":
                args = tokens[j + 1:closing(tokens, j)]
                value = literal(args)
                if value is not None:
                    add(result["collections"], value, {"collection": value}, {**origin, "line": token.line, "role": "literal_collection_call"})
        # Collection literal authorities come from repositories/extensions and index modules.
        if "repository" in path.parts or "indexes" in path.parts:
            local = {}
            collect_constants(tokens, origin, local)
            for name, vals in local.items():
                constants.setdefault(name, set()).update(vals)
                for value in vals:
                    add(result["collections"], value, {"collection": value}, {**origin, "constant": name})
            for token in tokens:
                if token.value == "include" or token.value == "cfg_attr":
                    issue(reviews, "macros", "Conditional/generated repository or index declarations need review", origin)
                    break
    # Constant names with multiple literal values cannot be resolved safely by leaf symbol.
    for name, vals in constants.items():
        if len(vals) > 1:
            issue(reviews, "collections", f"Ambiguous collection constant {name}: {sorted(vals)}")
    for tokens, origin, raw in inventory:
        capture_dtos(tokens, origin, result["dto"], reviews)
        capture_errors(tokens, origin, result["errors"])
        capture_indexes(tokens, origin, constants, result["indexes"], result["collections"], reviews)
        source_contracts(tokens, origin, raw, result["foundations"])
    generated = repo / "erp-client/lib/permissions.generated.ts"
    if generated.is_file():
        text = generated.read_text()
        routes = re.findall(r'method:\s*"([A-Z]+)"\s*,\s*path:\s*"([^"]+)".*?resource:\s*"([^"]+)"\s*,\s*action:\s*"([^"]+)"', text, re.S)
        result["http"] = {"artifact": source(generated, repo), "sha256": digest(generated.read_bytes()), "routes": sorted(set(routes)), "route_count": len(routes)}
        if not routes:
            issue(reviews, "http", "Permission artifact exists but route parser captured zero routes", source(generated, repo))
    else:
        issue(reviews, "http", "Permission generated artifact is missing", {"path": "erp-client/lib/permissions.generated.ts"})
    for category in ("collections", "indexes", "dto", "errors", "foundations"):
        if not result[category]:
            issue(reviews, category, "Required evidence category has no captured facts")
    return result


def semantic_records(records, category):
    out = {}
    for name, rows in records.items():
        if category == "collections":
            out[name] = [stable({"collection": name, "declared_constants": sorted({row["source"]["constant"] for row in rows if "constant" in row["source"]})})]
        else:
            out[name] = sorted({stable(row["contract"]) for row in rows})
    return out


def compare(before, after):
    changes, missing, added = [], [], []
    for category in ("collections", "indexes", "dto", "errors"):
        left = semantic_records(before[category], category)
        right = semantic_records(after[category], category)
        for symbol in sorted(left.keys() | right.keys()):
            item = {"category": category, "symbol": symbol,
                    "before": before[category].get(symbol), "after": after[category].get(symbol)}
            if symbol not in right:
                missing.append(item)
            elif symbol not in left:
                added.append(item)
            elif left[symbol] != right[symbol]:
                changes.append(item)
    http_equal = {k: v for k, v in before["http"].items() if k != "artifact"} == {k: v for k, v in after["http"].items() if k != "artifact"}
    if not http_equal:
        changes.append({"category": "http", "symbol": "generated_permissions", "before": before["http"], "after": after["http"]})
    # Source hashes are documentary evidence; movement/import changes must not imply runtime drift.
    hashes = {}
    for category in ("money", "idempotency", "executor", "transaction"):
        def symbols(snapshot):
            grouped = defaultdict(set)
            for row in snapshot["foundations"].get(category, []):
                for item in row["contract"]["symbols"]:
                    grouped[item["symbol"]].add(item["token_sha256"])
            return {k: sorted(v) for k, v in grouped.items()}
        left, right = symbols(before), symbols(after)
        source_hashes_equal = {row["contract"]["production_token_sha256"] for row in before["foundations"].get(category, [])} == {row["contract"]["production_token_sha256"] for row in after["foundations"].get(category, [])}
        hashes[category] = {"before": before["foundations"].get(category, []), "after": after["foundations"].get(category, []),
                            "symbol_hashes_equal": left == right, "production_source_hashes_equal": source_hashes_equal,
                            "changed_symbols": sorted(k for k in left.keys() | right.keys() if left.get(k) != right.get(k)),
                            "status": "static_equal" if left == right and source_hashes_equal else "needs_review"}
    reviews = [{"side": side, **entry} for side, snapshot in [("before", before), ("after", after)] for entry in snapshot["needs_review"]]
    reviews += [{"status": "needs_review", "category": "foundations", "message": f"{cat} source or symbol hashes changed", "symbols": info["changed_symbols"]} for cat, info in hashes.items() if info["status"] != "static_equal"]
    # Additions are visible review obligations; they can be intentional consumer fact types.
    reviews += [{"status": "needs_review", "category": item["category"], "message": "New captured symbol", "symbol": item["symbol"]} for item in added]
    failed = bool(changes or missing)
    return {"status": "drift_detected" if failed else "needs_review" if reviews else "static_equal",
            "captured_contracts_equal": not failed, "changed": changes, "missing": missing, "added": added,
            "needs_review": reviews, "foundation_comparison": hashes}


def write_outputs(before, after, phase, output):
    comparison = compare(before, after)
    shared = {"schema_version": 1, "phase": phase, "evidence_kind": "static_source_capture", "real_database_verified": False,
              "disclaimer": DISCLAIMER, "limitations": LIMITS, "before_repo": before["repo"], "after_repo": after["repo"]}
    contract = {**shared, "status": comparison["status"], "comparison": comparison, "before": before, "after": after}
    transaction = {**shared, "status": "needs_review", "runtime_verification": {"same_executor": "not_verified", "write_order": "not_verified", "rollback": "not_verified", "external_io_outside_session": "not_verified"},
                   "source_comparison": comparison["foundation_comparison"], "required_evidence": "Attach existing pure inline/Port trace test logs; static hashes do not establish runtime order."}
    output.mkdir(parents=True, exist_ok=True)
    for name, value in [("contract-comparison.json", contract), ("transaction-contract.json", transaction), ("missing-drift-report.json", {**shared, **comparison})]:
        (output / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    return comparison


class SelfTests(unittest.TestCase):
    def snapshot(self, tokens, path="backend/services/src/a/dto.rs"):
        records, indexes, reviews = {}, {}, []
        ts = production(lex(tokens))
        origin = {"path": path}
        capture_dtos(ts, origin, records, reviews)
        capture_indexes(ts, {"path": "backend/database/src/indexes/a.rs"}, {"ITEMS": {"items"}}, indexes, {}, reviews)
        return {"repo": "/fixture", "collections": {"items": []}, "dto": records, "indexes": indexes,
                "errors": {}, "http": {"sha256": "same"}, "foundations": {}, "needs_review": reviews}

    SOURCE = '''#[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
    pub struct Request { #[serde(default)] pub order_id: old::SalesOrderId, pub amount: Amount }
    fn ensure() { create_indexes(db, ITEMS, item_indexes()); }
    fn item_indexes() { unique_index("uk_items", doc! { "order_id": 1 }); }
    fn unique_index(name: &str, keys: Document) { IndexModel::builder().keys(keys)
       .options(IndexOptions::builder().name(name.into()).unique(true).build()).build() }
    #[cfg(test)] mod tests { pub struct Request { pub must_not_capture: bool } }
    '''

    def test_relocation_and_qualified_import_keep_contract(self):
        left = self.snapshot(self.SOURCE)
        right = self.snapshot(self.SOURCE.replace("old::SalesOrderId", "erp_core::ids::SalesOrderId"), "backend/crates/erp-a/src/dto/a.rs")
        self.assertTrue(compare(left, right)["captured_contracts_equal"])
        self.assertEqual(len(left["dto"]["Request"]), 1)

    def test_field_change_fails(self):
        self.assertFalse(compare(self.snapshot(self.SOURCE), self.snapshot(self.SOURCE.replace("pub amount: Amount", "pub amount: String")))["captured_contracts_equal"])

    def test_serde_change_fails(self):
        self.assertFalse(compare(self.snapshot(self.SOURCE), self.snapshot(self.SOURCE.replace("camelCase", "snake_case")))["captured_contracts_equal"])

    def test_index_keys_and_uniqueness_fail(self):
        for old, new in [('"order_id": 1', '"order_id": -1'), (".unique(true)", ".unique(false)")]:
            self.assertFalse(compare(self.snapshot(self.SOURCE), self.snapshot(self.SOURCE.replace(old, new)))["captured_contracts_equal"])

    def test_collection_constant_swap_fails(self):
        before, after = self.snapshot(self.SOURCE), self.snapshot(self.SOURCE)
        before["collections"] = {"left": [{"source": {"constant": "A"}}], "right": [{"source": {"constant": "B"}}]}
        after["collections"] = {"left": [{"source": {"constant": "B"}}], "right": [{"source": {"constant": "A"}}]}
        self.assertFalse(compare(before, after)["captured_contracts_equal"])

    def test_missing_symbol_fails(self):
        after = self.snapshot(self.SOURCE)
        after["dto"] = {}
        self.assertFalse(compare(self.snapshot(self.SOURCE), after)["captured_contracts_equal"])

    def test_direct_builder_and_collection_registration(self):
        code = 'fn ensure() { db.collection::<Document>(ITEMS).create_indexes(item_indexes()); } fn item_indexes() { IndexModel::builder().keys(doc! { "id": 1 }).options(IndexOptions::builder().name("uk_items".into()).unique(true).build()).build() }'
        item = self.snapshot(code)["indexes"]["uk_items"][0]["contract"]
        self.assertEqual(item["collections"], ["items"])
        self.assertTrue(item["unique"])

    def test_path_like_wire_literal_is_not_normalized(self):
        self.assertNotEqual(canonical(lex('"old::Action"'), True), canonical(lex('"new::Action"'), True))

    def test_grouped_error_mapping_preserves_guard_and_status(self):
        body = lex('match self { Error::A(_) | Error::B(_) => StatusCode::BAD_REQUEST, Error::C(x) if x.ok() => StatusCode::CONFLICT, _ => StatusCode::INTERNAL_SERVER_ERROR, }')
        arms = match_arms(body)
        self.assertEqual(len(arms), 3)
        self.assertIn("|", arms[0]["pattern"])
        self.assertIn("if", arms[1]["pattern"])
        self.assertEqual(arms[0]["expression"], "BAD_REQUEST")

    def test_literals_and_nested_comments(self):
        ts = lex('/* outer /* inner */ done */ fn x() { "// not comment }"; r#"{raw}"#; }')
        self.assertEqual(len(list(declarations(ts, "fn"))), 1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before-repo", type=Path)
    parser.add_argument("--after-repo", type=Path)
    parser.add_argument("--phase")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(SelfTests))
        return 0 if result.wasSuccessful() else 1
    for name in ("before_repo", "after_repo", "phase", "output"):
        if getattr(args, name) is None:
            parser.error(f"--{name.replace('_', '-')} is required")
    if args.phase not in {f"{p:02d}" for p in range(9, 18)}:
        parser.error("--phase must be 09 through 17")
    before_repo, after_repo, output = args.before_repo.resolve(), args.after_repo.resolve(), args.output.resolve()
    if output == before_repo or before_repo in output.parents:
        parser.error("--output must not write into the read-only before repository")
    try:
        comparison = write_outputs(capture(before_repo), capture(after_repo), args.phase, output)
    except (OSError, ValueError, tomllib.TOMLDecodeError) as exc:
        parser.exit(2, f"Capture incomplete: {exc}\n")
    summary = {k: len(comparison[k]) for k in ("changed", "missing", "added", "needs_review")}
    print(stable({"status": comparison["status"], **summary, "output": str(output), "disclaimer": DISCLAIMER}))
    return {"drift_detected": 1, "needs_review": 2, "static_equal": 0}[comparison["status"]]


if __name__ == "__main__":
    raise SystemExit(main())
