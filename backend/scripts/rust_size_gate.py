#!/usr/bin/env python3
"""Fail-closed production Rust size gate.

A file may not exceed 800 physical production lines. A function/method may not
exceed 50 effective body lines. Test code is excluded: `tests/` trees,
`#[cfg(test)]` items, `#[test]` functions, and `#[cfg(test)] mod name;` files.
`build.rs` is checked for file size only. Proc-macro crates skip the method cap.
Examples and benches are excluded.

This is a lexical scan, not a rustc parse. Clippy `too_many_lines` cannot
exclude tests from file size and is allow-by-default.
"""

from __future__ import annotations

import argparse
from dataclasses import asdict, dataclass
import json
from pathlib import Path
import re
import sys
import unittest

from cutover_workspace import _code_and_literals, active_source_paths
from domain_boundaries import cargo_metadata


MAX_FILE_LINES = 800
MAX_FN_LINES = 50
FN_NAME = re.compile(r"\bfn\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)")
MOD_NAME = re.compile(r"mod\s+([A-Za-z_][A-Za-z0-9_]*)")
MACRO_IMPL_PREFIXES = ("crates/entity-macros/", "crates/permission-macros/")
CFG_TEST = re.compile(
    r"#\s*!?\s*\[\s*cfg\s*\(\s*(?:test\s*\)|all\s*\((?:[^()]|\([^()]*\))*\btest\b)",
    re.S,
)
TEST_ATTR = re.compile(r"#\s*\[\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*test\b")
LIB_ENTRY = {"lib.rs", "main.rs", "mod.rs"}


@dataclass(frozen=True)
class Finding:
    """One blocking diagnostic; line offsets refer to the original source."""

    rule: str
    path: str
    line: int
    message: str
    extra: int = 0


def skip_ws(code: str, index: int) -> int:
    """Advance past Unicode whitespace."""
    length = len(code)
    while index < length and code[index].isspace():
        index += 1
    return index


def skip_balanced(code: str, index: int, opener: str, closer: str) -> int:
    """Return the index after a nested opener/closer pair starting at index."""
    if index >= len(code) or code[index] != opener:
        raise ValueError(f"期望 {opener!r}，实际 {code[index:index + 8]!r}")
    depth = 0
    for pos in range(index, len(code)):
        char = code[pos]
        depth += char == opener
        depth -= char == closer
        if depth == 0:
            return pos + 1
    raise ValueError(f"{opener} 未闭合")


def skip_vis(code: str, index: int) -> int:
    """Skip `pub`, `pub(crate)`, `pub(super)` and `pub(in path)`."""
    match = re.match(r"pub\b", code[index:])
    if not match:
        return index
    index = skip_ws(code, index + match.end())
    if index < len(code) and code[index] == "(":
        return skip_ws(code, skip_balanced(code, index, "(", ")"))
    return index


def parse_attr(code: str, index: int) -> tuple[int, int, str, bool] | None:
    """Parse `#[...]` or `#![...]` at index; return start, end, text, inner."""
    if index >= len(code) or code[index] != "#":
        return None
    start = index
    index += 1
    index = skip_ws(code, index)
    inner = False
    if index < len(code) and code[index] == "!":
        inner = True
        index = skip_ws(code, index + 1)
    if index >= len(code) or code[index] != "[":
        return None
    end = skip_balanced(code, index, "[", "]")
    return start, end, code[start:end], inner


def consume_attrs(code: str, index: int) -> tuple[list[str], int]:
    """Consume a run of outer attributes. Inner attributes are not included."""
    attrs: list[str] = []
    while True:
        index = skip_ws(code, index)
        parsed = parse_attr(code, index)
        if parsed is None or parsed[3]:
            return attrs, index
        attrs.append(parsed[2])
        index = parsed[1]


def is_test_only_attrs(attrs: list[str]) -> bool:
    """True when any attribute makes the following item test-only."""
    return any(CFG_TEST.match(attr) or TEST_ATTR.match(attr) for attr in attrs)


def find_block_or_semi(code: str, index: int) -> tuple[int, bool]:
    """Find the item `{` or `;` after a keyword, ignoring nested `<>()[]` and const-generic `{}`."""
    angle = paren = square = brace = 0
    length = len(code)
    while index < length:
        char = code[index]
        if char == "<":
            angle += 1
        elif char == ">" and angle:
            angle -= 1
        elif char == "(":
            paren += 1
        elif char == ")" and paren:
            paren -= 1
        elif char == "[":
            square += 1
        elif char == "]" and square:
            square -= 1
        elif char == "{":
            if angle == paren == square == brace == 0:
                return index, True
            brace += 1
        elif char == "}" and brace:
            brace -= 1
        elif char == ";" and angle == paren == square == brace == 0:
            return index, False
        index += 1
    raise ValueError("item 缺少 `{` 或 `;`")


def block_end(code: str, start: int) -> int:
    """Return the index after a balanced `{...}` starting at start."""
    return skip_balanced(code, start, "{", "}")


def consume_item(code: str, index: int) -> tuple[int, str | None]:
    """Skip one item after its attributes. Semicolon `mod name;` yields the name."""
    index = skip_vis(code, skip_ws(code, index))
    index = skip_ws(code, index)
    mod = MOD_NAME.match(code[index:])
    if mod:
        name = mod.group(1)
        pos, is_block = find_block_or_semi(code, index + mod.end())
        if is_block:
            return block_end(code, pos), None
        return pos + 1, name
    pos, is_block = find_block_or_semi(code, index)
    if is_block:
        return block_end(code, pos), None
    return pos + 1, None


def crate_cfg_test(code: str) -> bool:
    """True when an inner `#![cfg(test)]` applies to the whole file."""
    index = skip_ws(code, 0)
    while True:
        parsed = parse_attr(code, index)
        if parsed is None:
            return False
        _start, end, text, inner = parsed
        if inner and CFG_TEST.match(text):
            return True
        if not inner:
            return False
        index = skip_ws(code, end)


def find_test_ranges(code: str) -> tuple[list[tuple[int, int]], list[str]]:
    """Locate test-only items. Offsets match the original source."""
    if crate_cfg_test(code):
        return [(0, len(code))], []
    ranges: list[tuple[int, int]] = []
    submods: list[str] = []
    index = 0
    length = len(code)
    while index < length:
        if code[index] != "#":
            index += 1
            continue
        start = index
        try:
            attrs, after_attrs = consume_attrs(code, index)
        except ValueError:
            index += 1
            continue
        if not attrs:
            index += 1
            continue
        if not is_test_only_attrs(attrs):
            index = after_attrs if after_attrs > index else index + 1
            continue
        try:
            end, submod = consume_item(code, after_attrs)
        except ValueError:
            index = after_attrs if after_attrs > index else index + 1
            continue
        ranges.append((start, max(end, start + 1)))
        if submod:
            submods.append(submod)
        index = end if end > start else start + 1
    return ranges, submods


def mask_ranges(code: str, ranges: list[tuple[int, int]]) -> str:
    """Replace non-newline characters in ranges with spaces; keep line numbers."""
    if not ranges:
        return code
    chars = list(code)
    for start, end in ranges:
        for pos in range(start, min(end, len(chars))):
            if chars[pos] != "\n":
                chars[pos] = " "
    return "".join(chars)


def production_line_count(source: str, ranges: list[tuple[int, int]]) -> int:
    """Count physical lines that are not entirely inside a test item."""
    if not source:
        return 0
    marked = bytearray(len(source))
    for start, end in ranges:
        marked[start:end] = b"\x01" * max(0, min(end, len(source)) - start)
    count = 0
    offset = 0
    while offset < len(source):
        newline = source.find("\n", offset)
        end = len(source) if newline < 0 else newline + 1
        if not all(marked[offset:end]):
            count += 1
        if newline < 0:
            break
        offset = end
    return count


def effective_lines(text: str) -> int:
    """Count non-blank lines; comments must already be masked to spaces."""
    return sum(1 for line in text.splitlines() if line.strip())


def submodule_paths(parent: str, name: str) -> list[str]:
    """Resolve `mod name;` to the files Rust would load next to parent."""
    path = Path(parent)
    base = path.parent if path.name in LIB_ENTRY else path.parent / path.stem
    return [(base / f"{name}.rs").as_posix(), (base / name / "mod.rs").as_posix()]


def is_excluded_path(path: str) -> bool:
    """True for tests, benches, examples, and conventional `tests.rs` modules."""
    parts = Path(path).parts
    stem = Path(path).stem
    return (
        "tests" in parts
        or "examples" in parts
        or "benches" in parts
        or stem in {"tests", "test"}
        or stem.endswith("_test")
        or stem.endswith("_tests")
    )


def skip_fn_check(path: str) -> bool:
    """`build.rs` and proc-macro implementations are exempt from the method cap."""
    return Path(path).name == "build.rs" or path.startswith(MACRO_IMPL_PREFIXES)


def functions(code: str) -> list[tuple[str, int, int]]:
    """Return `(name, line, effective_body_lines)` for functions with a body."""
    found: list[tuple[str, int, int]] = []
    for match in FN_NAME.finditer(code):
        try:
            pos, is_block = find_block_or_semi(code, match.end())
        except ValueError:
            continue
        if not is_block:
            continue
        try:
            end = block_end(code, pos)
        except ValueError:
            continue
        body = code[pos + 1 : end - 1]
        found.append((match.group(1), code.count("\n", 0, match.start()) + 1, effective_lines(body)))
    return found


def scan_source(
    path: str,
    source: str,
    *,
    max_file: int = MAX_FILE_LINES,
    max_fn: int = MAX_FN_LINES,
) -> tuple[list[Finding], list[str]]:
    """Check one file. Returns findings and `#[cfg(test)] mod name;` names."""
    code, _ = _code_and_literals(source)
    try:
        ranges, submods = find_test_ranges(code)
    except ValueError as error:
        line = 1
        return [Finding("SIZE-PARSE", path, line, f"无法扫描测试项: {error}")], []
    findings: list[Finding] = []
    lines = production_line_count(source, ranges)
    if lines > max_file:
        findings.append(Finding(
            "SIZE-FILE", path, 1,
            f"生产代码 {lines} 行，上限 {max_file}",
            lines,
        ))
    if not skip_fn_check(path):
        production = mask_ranges(code, ranges)
        try:
            for name, line, count in functions(production):
                if count > max_fn:
                    findings.append(Finding(
                        "SIZE-FN", path, line,
                        f"方法 {name} 有 {count} 个有效行，上限 {max_fn}",
                        count,
                    ))
        except ValueError as error:
            findings.append(Finding("SIZE-PARSE", path, 1, f"无法扫描方法: {error}"))
    return findings, submods


def workspace_sources(backend: Path) -> dict[str, str]:
    """Read active member sources; empty inventory is a tool error."""
    metadata = cargo_metadata(backend)
    sources: dict[str, str] = {}
    for path in active_source_paths(metadata):
        relative = path.relative_to(backend).as_posix()
        sources[relative] = path.read_text(encoding="utf-8")
    if not sources:
        raise RuntimeError("活动源码清单为空")
    return sources


def scan_workspace(
    sources: dict[str, str],
    *,
    max_file: int = MAX_FILE_LINES,
    max_fn: int = MAX_FN_LINES,
) -> list[Finding]:
    """Scan every production source; skip test files declared by their parent."""
    scanned: dict[str, tuple[list[Finding], list[str]]] = {}
    test_files: set[str] = set()
    for path, source in sources.items():
        scanned[path] = scan_source(path, source, max_file=max_file, max_fn=max_fn)
        for name in scanned[path][1]:
            test_files.update(submodule_paths(path, name))
    findings: list[Finding] = []
    for path, (file_findings, _submods) in scanned.items():
        if is_excluded_path(path) or path in test_files:
            continue
        findings.extend(file_findings)
    findings.sort(key=lambda item: (item.path, item.line, item.rule, item.message))
    return findings


def self_tests() -> bool:
    """Run mandatory positive/negative fixtures before judging any workspace."""
    import test_rust_size_gate

    suite = unittest.defaultTestLoader.loadTestsFromModule(test_rust_size_gate)
    if suite.countTestCases() == 0:
        raise RuntimeError("门禁自检未发现测试，禁止空跑成功")
    return unittest.TextTestRunner(stream=sys.stderr, verbosity=1).run(suite).wasSuccessful()


def format_report(findings: list[Finding], file_count: int, max_file: int, max_fn: int) -> str:
    """Human-readable summary; findings first, then status."""
    lines = [f"[{item.rule}] {item.path}:{item.line}: {item.message}" for item in findings]
    files = sum(item.rule == "SIZE-FILE" for item in findings)
    functions_hit = sum(item.rule == "SIZE-FN" for item in findings)
    parse_hit = sum(item.rule == "SIZE-PARSE" for item in findings)
    if findings:
        lines.append(
            f"SIZE_CHECKS_FAILED: 扫描 {file_count} 个活动源码文件，"
            f"超限文件 {files}，超限方法 {functions_hit}，解析失败 {parse_hit}。"
            f"上限：文件 {max_file} 行（不含测试），方法 {max_fn} 有效行。"
        )
    else:
        lines.append(
            f"SIZE_CHECKS_PASSED: 扫描 {file_count} 个活动源码文件。"
            f"上限：文件 {max_file} 行（不含测试），方法 {max_fn} 有效行。"
        )
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    """Return 0 when clean, 1 when blocked, 2 on tool/self-test errors."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", required=True, type=Path)
    parser.add_argument("--self-test-only", action="store_true", help="仅验证门禁夹具，不检查业务代码")
    parser.add_argument("--json", action="store_true", help="stdout 输出 JSON")
    parser.add_argument("--max-file-lines", type=int, default=MAX_FILE_LINES)
    parser.add_argument("--max-fn-lines", type=int, default=MAX_FN_LINES)
    args = parser.parse_args(argv)
    try:
        if not self_tests():
            return 2
        if args.self_test_only:
            print(json.dumps({"status": "SELF_TEST_ONLY", "workspace_checked": False}) if args.json else
                  "门禁夹具通过；未检查实际工作区。")
            return 0
        backend = args.backend.resolve()
        sources = workspace_sources(backend)
        findings = scan_workspace(
            sources, max_file=args.max_file_lines, max_fn=args.max_fn_lines,
        )
        status = "SIZE_CHECKS_FAILED" if findings else "SIZE_CHECKS_PASSED"
        if args.json:
            print(json.dumps({
                "status": status,
                "files": len(sources),
                "max_file_lines": args.max_file_lines,
                "max_fn_lines": args.max_fn_lines,
                "findings": [asdict(item) for item in findings],
            }, ensure_ascii=False, indent=2))
        else:
            sys.stdout.write(format_report(
                findings, len(sources), args.max_file_lines, args.max_fn_lines,
            ))
        return 1 if findings else 0
    except Exception as error:
        if args.json:
            print(json.dumps({"status": "TOOL_ERROR", "error": str(error)}, ensure_ascii=False))
        else:
            print(f"TOOL_ERROR: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
