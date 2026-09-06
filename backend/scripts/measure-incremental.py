#!/usr/bin/env python3
"""Measure incremental cargo check/build after a unique compile-probe edit.

The command-line interface, restore rules, sample algorithm, and output layout
follow backend/docs/superpowers/plans/domain-crate-migration/compile-measurement.md.
The process never starts web-api, MongoDB, or other external services. Source
files are backed up and restored as raw bytes; SIGINT/SIGTERM and failures
restore the probe file before exit.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import signal
import stat
import subprocess
import sys
import tempfile
import time
import traceback
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Mapping, Sequence


SCENARIOS = ("customer", "sales", "finance")
MODES = ("check", "build")
REVISIONS = ("baseline", "candidate")
BUSINESS_PACKAGE_NAMES = {
    "entities",
    "database",
    "services",
    "erp-core",
    "application-core",
    "persistence-core",
    "erp-processes",
    "erp-read-models",
    "erp-identity",
    "erp-audit",
    "erp-workflow",
    "erp-support",
    "erp-party",
    "erp-customer",
    "erp-supplier",
    "erp-catalog",
    "erp-warehouse",
    "erp-contract",
    "erp-sales",
    "erp-procurement",
    "erp-fulfillment",
    "erp-inventory",
    "erp-finance",
    "erp-returns",
    "erp-import",
    "erp-integration",
    "erp-supply",
    "erp-commerce",
}


class MeasureError(Exception):
    """Fatal measurement or restore-contract error."""


@dataclass
class Probe:
    """One compile-probes.json scenario."""

    id: str
    path: Path
    rel_path: str
    package: str
    symbol: str
    before: str
    after: str
    equivalence: str


@dataclass
class SourceGuard:
    """Byte-for-byte backup and restore of a single probe file."""

    path: Path
    original: bytes
    sha256: str
    mode: int
    backup_path: Path | None = None

    @classmethod
    def capture(cls, path: Path) -> SourceGuard:
        """Read original bytes, hash, and permission bits."""
        original = path.read_bytes()
        mode = path.stat().st_mode
        return cls(
            path=path,
            original=original,
            sha256=hashlib.sha256(original).hexdigest(),
            mode=mode,
        )

    def write_backup(self, backup_path: Path) -> None:
        """Persist the original bytes for crash recovery."""
        backup_path.write_bytes(self.original)
        self.backup_path = backup_path

    def current_sha256(self) -> str:
        """Return the hash of the file currently on disk."""
        return hashlib.sha256(self.path.read_bytes()).hexdigest()

    def ensure_before(self, before: str, after: str) -> None:
        """Require a unique before fragment and the absence of after."""
        blob = self.path.read_bytes()
        before_b = before.encode("utf-8")
        after_b = after.encode("utf-8")
        if blob.count(before_b) != 1:
            raise MeasureError(
                f"{self.path} 的 before 片段必须恰好匹配一次，实际 {blob.count(before_b)} 次"
            )
        if after_b in blob:
            raise MeasureError(f"{self.path} 已包含 after 片段，拒绝模糊替换")

    def apply_after(self, before: str, after: str) -> None:
        """Replace the unique before fragment with after using raw bytes."""
        self.ensure_before(before, after)
        blob = self.path.read_bytes()
        patched = blob.replace(before.encode("utf-8"), after.encode("utf-8"), 1)
        self.path.write_bytes(patched)

    def restore(self) -> None:
        """Write original bytes and permission bits back, then verify the hash."""
        self.path.write_bytes(self.original)
        os.chmod(self.path, stat.S_IMODE(self.mode))
        actual = self.current_sha256()
        if actual != self.sha256:
            raise MeasureError(f"恢复后哈希不匹配: {actual} != {self.sha256}")


def utc_now() -> str:
    """Return an ISO-8601 UTC timestamp."""
    return datetime.now(timezone.utc).isoformat()


def run_checked(
    args: Sequence[str],
    *,
    cwd: Path,
    env: Mapping[str, str] | None = None,
    timeout: int | None = None,
) -> subprocess.CompletedProcess[str]:
    """Run a subprocess with an argument array; never use shell=True."""
    return subprocess.run(
        list(args),
        cwd=str(cwd),
        env=dict(env) if env is not None else None,
        text=True,
        capture_output=True,
        timeout=timeout,
        check=False,
    )


def git_output(repo: Path, git_args: Sequence[str]) -> str:
    """Run git in repo and return stripped stdout, or raise MeasureError."""
    proc = run_checked(["git", *git_args], cwd=repo)
    if proc.returncode != 0:
        raise MeasureError(f"git {' '.join(git_args)} 失败: {proc.stderr.strip()}")
    return proc.stdout.strip()


def assert_isolated_worktree(repo: Path) -> str:
    """Refuse the primary checkout; require a linked git worktree."""
    if not repo.is_dir():
        raise MeasureError(f"--repo 不是目录: {repo}")
    toplevel = Path(git_output(repo, ["rev-parse", "--show-toplevel"])).resolve()
    if toplevel != repo.resolve():
        raise MeasureError(f"--repo 必须是仓库根 {toplevel}，实际 {repo.resolve()}")
    inside = git_output(repo, ["rev-parse", "--is-inside-work-tree"])
    if inside != "true":
        raise MeasureError("--repo 不是 git worktree")
    git_dir = Path(git_output(repo, ["rev-parse", "--absolute-git-dir"])).resolve()
    common = git_output(repo, ["rev-parse", "--git-common-dir"])
    common_path = Path(common)
    if not common_path.is_absolute():
        common_path = (repo / common_path).resolve()
    else:
        common_path = common_path.resolve()
    if git_dir == common_path:
        raise MeasureError("拒绝在主 worktree 测量；请使用 git worktree add 得到的专用工作区")
    return git_output(repo, ["rev-parse", "HEAD"])


def assert_source_clean(repo: Path, rel_path: str) -> None:
    """Fail when the probe file already has uncommitted edits."""
    porcelain = git_output(repo, ["status", "--porcelain", "--", rel_path])
    if porcelain:
        raise MeasureError(f"探针源文件已有未提交修改，拒绝测量: {porcelain}")


def median(values: Sequence[float]) -> float:
    """Return the middle value of five (or any odd-length) samples."""
    ordered = sorted(values)
    return ordered[len(ordered) // 2]


def load_probe(spec_path: Path, scenario: str, revision: str, backend: Path) -> Probe:
    """Load one scenario from compile-probes.json."""
    spec = json.loads(spec_path.read_text(encoding="utf-8"))
    probes = {item["id"]: item for item in spec["probes"]}
    if scenario not in probes:
        raise MeasureError(f"未知 scenario: {scenario}")
    item = probes[scenario]
    rel = item["baseline_path"] if revision == "baseline" else item["final_path"]
    package = item["baseline_package"] if revision == "baseline" else item["final_package"]
    path = backend / rel
    if not path.is_file():
        raise MeasureError(f"探针文件不存在: {path}")
    return Probe(
        id=item["id"],
        path=path,
        rel_path=rel,
        package=package,
        symbol=item["symbol"],
        before=item["before"],
        after=item["after"],
        equivalence=item["equivalence"],
    )


def cargo_metadata(backend: Path, env: Mapping[str, str]) -> dict[str, Any]:
    """Return `cargo metadata` JSON without inheriting a measurement target dir."""
    meta_env = dict(env)
    meta_env.pop("CARGO_TARGET_DIR", None)
    proc = run_checked(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--offline"],
        cwd=backend,
        env=meta_env,
    )
    if proc.returncode != 0:
        proc = run_checked(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=backend,
            env=meta_env,
        )
    if proc.returncode != 0:
        raise MeasureError(f"cargo metadata 失败: {proc.stderr[-4000:]}")
    return json.loads(proc.stdout)


def package_name_from_id(package_id: str) -> str:
    """Extract the crate name from a cargo package_id.

    Cargo 1.77+ uses `registry+url#name@version` and `path+file:///abs/crate#version`.
    Path packages put the version after `#` and the crate name in the path suffix.
    """
    raw = package_id.strip()
    if raw.startswith("path+file://"):
        path_part = raw.split("#", 1)[0]
        path_part = path_part[len("path+file://") :]
        return Path(path_part).name
    if "#" in raw:
        right = raw.rsplit("#", 1)[-1]
        if "@" in right:
            return right.split("@", 1)[0]
        return Path(raw.split("#", 1)[0]).name
    return raw.split(" ", 1)[0]


def reverse_dependencies(metadata: Mapping[str, Any], package_name: str) -> dict[str, Any]:
    """Build the reverse-dependency closure of a workspace package."""
    resolve = metadata.get("resolve") or {}
    nodes = resolve.get("nodes") or []
    id_to_name: dict[str, str] = {}
    dependents: dict[str, set[str]] = {}
    for node in nodes:
        name = package_name_from_id(node["id"])
        id_to_name[node["id"]] = name
        for dep in node.get("deps") or []:
            dependents.setdefault(package_name_from_id(dep["pkg"]), set()).add(name)
    seen: set[str] = set()
    stack = [package_name]
    while stack:
        current = stack.pop()
        if current in seen:
            continue
        seen.add(current)
        for parent in sorted(dependents.get(current, ())):
            if parent not in seen:
                stack.append(parent)
    return {
        "package": package_name,
        "reverse_closure": sorted(seen),
        "direct_dependents": sorted(dependents.get(package_name, ())),
    }


def parse_compiler_units(jsonl: str) -> dict[str, Any]:
    """Parse cargo JSON lines into Fresh/dirty compiler-artifact records."""
    artifacts: list[dict[str, Any]] = []
    build_scripts: list[dict[str, Any]] = []
    other: list[str] = []
    for raw in jsonl.splitlines():
        line = raw.strip()
        if not line:
            continue
        try:
            message = json.loads(line)
        except json.JSONDecodeError:
            other.append(line)
            continue
        reason = message.get("reason")
        if reason == "compiler-artifact":
            target = message.get("target") or {}
            profile = message.get("profile") or {}
            if "fresh" not in message:
                raise MeasureError("compiler-artifact 缺少 fresh，不能当作编译成功证据")
            record = {
                "package_id": message.get("package_id"),
                "package_name": package_name_from_id(str(message.get("package_id", ""))),
                "target_name": target.get("name"),
                "target_kind": list(target.get("kind") or []),
                "profile": profile.get("name") or profile.get("debuginfo"),
                "fresh": bool(message["fresh"]),
            }
            artifacts.append(record)
        elif reason == "build-script-executed":
            build_scripts.append(
                {
                    "package_id": message.get("package_id"),
                    "package_name": package_name_from_id(str(message.get("package_id", ""))),
                    "out_dir": message.get("out_dir"),
                }
            )
    dirty = [item for item in artifacts if not item["fresh"]]
    fresh = [item for item in artifacts if item["fresh"]]
    return {
        "artifacts": artifacts,
        "build_scripts": build_scripts,
        "dirty": dirty,
        "fresh": fresh,
        "non_json_lines": other[:50],
    }


def business_dirty(units: Mapping[str, Any]) -> list[dict[str, Any]]:
    """Return dirty compiler artifacts that belong to business packages."""
    return [item for item in units["dirty"] if item["package_name"] in BUSINESS_PACKAGE_NAMES]


def package_dirty(units: Mapping[str, Any], package: str) -> list[dict[str, Any]]:
    """Return dirty artifacts for one package name."""
    return [item for item in units["dirty"] if item["package_name"] == package]


def copy_timings(target_dir: Path, destination: Path) -> Path | None:
    """Copy the newest cargo timings HTML into the sample directory."""
    timings_dir = target_dir / "cargo-timings"
    if not timings_dir.is_dir():
        return None
    htmls = [path for path in timings_dir.glob("cargo-timing*.html") if path.is_file()]
    if not htmls:
        return None
    newest = max(htmls, key=lambda path: path.stat().st_mtime)
    destination.write_bytes(newest.read_bytes())
    return newest


def write_json(path: Path, value: Any) -> None:
    """Write pretty UTF-8 JSON."""
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def collect_environment(backend: Path, target_dir: Path, commit: str) -> dict[str, Any]:
    """Record allowed toolchain and host fields; do not dump secrets or config.toml."""
    env = os.environ
    rustc = run_checked(["rustc", "-vV"], cwd=backend)
    cargo = run_checked(["cargo", "-vV"], cwd=backend)
    rustup = run_checked(["rustup", "show", "active-toolchain"], cwd=backend)
    nproc = os.cpu_count()
    try:
        loadavg = os.getloadavg()
    except OSError:
        loadavg = None
    df = run_checked(["df", "-h", str(target_dir)], cwd=backend)
    cargo_config = backend / ".cargo" / "config.toml"
    codegen = None
    if cargo_config.is_file():
        text = cargo_config.read_text(encoding="utf-8")
        if 'codegen-backend = "cranelift"' in text:
            codegen = "cranelift (workspace packages; see .cargo/config.toml profile.dev)"
        elif "codegen-backend" in text:
            codegen = "configured (details not dumped)"
    toolchain_file = backend / "rust-toolchain.toml"
    channel = None
    if toolchain_file.is_file():
        for line in toolchain_file.read_text(encoding="utf-8").splitlines():
            if line.strip().startswith("channel"):
                channel = line.split("=", 1)[-1].strip().strip('"')
                break
    return {
        "recorded_at": utc_now(),
        "commit": commit,
        "rustc_verbose": rustc.stdout.strip(),
        "cargo_verbose": cargo.stdout.strip(),
        "active_toolchain": rustup.stdout.strip() or rustup.stderr.strip(),
        "toolchain_channel": channel,
        "profile": "dev",
        "codegen_backend": codegen,
        "features": "default (no --features)",
        "RUSTFLAGS": env.get("RUSTFLAGS"),
        "CARGO_INCREMENTAL": env.get("CARGO_INCREMENTAL"),
        "CARGO_TERM_COLOR": "never",
        "cpu_count": nproc,
        "loadavg": list(loadavg) if loadavg else None,
        "target_dir": str(target_dir),
        "target_dir_df": df.stdout.strip(),
        "cwd": str(backend),
        "notes": "未转储 config.toml / 环境变量全集 / 凭据",
    }


def cargo_command(mode: str) -> list[str]:
    """Return the cargo argv for check or build mode."""
    if mode == "check":
        return [
            "cargo",
            "check",
            "-p",
            "web-api",
            "--locked",
            "--message-format=json",
            "--timings",
        ]
    return [
        "cargo",
        "build",
        "-p",
        "web-api",
        "--bin",
        "web-api",
        "--locked",
        "--message-format=json",
        "--timings",
    ]


def cargo_env(base: Mapping[str, str], target_dir: Path) -> dict[str, str]:
    """Build the environment for a measured cargo subprocess."""
    env = dict(base)
    env["CARGO_TARGET_DIR"] = str(target_dir)
    env["CARGO_LOG"] = "cargo::core::compiler::fingerprint=info"
    env["CARGO_TERM_COLOR"] = "never"
    env.pop("ERP_TEST_MONGO_URI", None)
    return env


def run_cargo(
    *,
    backend: Path,
    mode: str,
    target_dir: Path,
    output_dir: Path,
    env: Mapping[str, str],
    time_it: bool,
) -> dict[str, Any]:
    """Run one cargo check/build, save logs, and optionally record wall time."""
    output_dir.mkdir(parents=True, exist_ok=True)
    argv = cargo_command(mode)
    child_env = cargo_env(env, target_dir)
    started = time.perf_counter()
    proc = subprocess.run(
        argv,
        cwd=str(backend),
        env=child_env,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    elapsed = time.perf_counter() - started
    (output_dir / "cargo.jsonl").write_text(proc.stdout, encoding="utf-8")
    (output_dir / "cargo.stderr.log").write_text(proc.stderr, encoding="utf-8")
    units = parse_compiler_units(proc.stdout)
    write_json(output_dir / "units.json", units)
    timings_src = copy_timings(target_dir, output_dir / "timings.html")
    sample = {
        "command": argv,
        "exit_code": proc.returncode,
        "seconds": elapsed if time_it else None,
        "wall_seconds_including_untimed": elapsed,
        "timed": time_it,
        "timings_source": str(timings_src) if timings_src else None,
        "dirty_package_names": sorted({item["package_name"] for item in units["dirty"]}),
        "fresh_package_names": sorted({item["package_name"] for item in units["fresh"]}),
        "business_dirty_package_names": sorted({item["package_name"] for item in business_dirty(units)}),
        "build_scripts_executed": [item["package_name"] for item in units["build_scripts"]],
    }
    write_json(output_dir / "sample.json", sample)
    if proc.returncode != 0:
        raise MeasureError(
            f"cargo {mode} 退出码 {proc.returncode}: {proc.stderr[-4000:]}"
        )
    if timings_src is None:
        raise MeasureError("缺少 cargo timings HTML，不能作为有效样本")
    return {"units": units, "sample": sample, "seconds": elapsed}


def fingerprint_excerpt(stderr: str, limit: int = 80) -> list[str]:
    """Return fingerprint log lines that explain unexpected rebuilds."""
    lines = []
    for line in stderr.splitlines():
        lower = line.lower()
        if "fingerprint" in lower or "dirty" in lower or "fresh" in lower or "rebuilt" in lower:
            lines.append(line)
        if len(lines) >= limit:
            break
    return lines


def acquire_target_lock(target_dir: Path) -> Path:
    """Create an exclusive lock file in the dedicated target directory."""
    target_dir.mkdir(parents=True, exist_ok=True)
    lock_path = target_dir / "measure-incremental.lock"
    if lock_path.exists():
        try:
            payload = json.loads(lock_path.read_text(encoding="utf-8"))
            pid = int(payload.get("pid", 0))
        except (OSError, ValueError, json.JSONDecodeError):
            pid = 0
        if pid and _pid_alive(pid):
            raise MeasureError(f"目标目录已有正在运行的测量锁: {lock_path} pid={pid}")
    write_json(lock_path, {"pid": os.getpid(), "created_at": utc_now()})
    return lock_path


def _pid_alive(pid: int) -> bool:
    """Return whether a PID exists (best-effort)."""
    try:
        os.kill(pid, 0)
    except OSError:
        return False
    return True


def prove_backup_restore() -> dict[str, Any]:
    """Prove backup/apply/failure-restore on a temporary workspace before measuring."""
    before = "self.mobile.as_deref()"
    after = "self.mobile.as_ref().map(String::as_str)"
    original = (
        b"fn required_value(&self) -> Option<&str> {\n"
        b"        self.mobile.as_deref()\n"
        b"    }\n"
    )
    with tempfile.TemporaryDirectory(prefix="measure-incremental-restore-") as tmp:
        root = Path(tmp)
        source = root / "validation.rs"
        source.write_bytes(original)
        os.chmod(source, 0o644)
        guard = SourceGuard.capture(source)
        backup = root / "source-backup"
        guard.write_backup(backup)
        recovery = {
            "source_path": str(source),
            "sha256": guard.sha256,
            "mode": stat.S_IMODE(guard.mode),
            "backup_file": "source-backup",
        }
        write_json(root / "recovery.json", recovery)
        guard.ensure_before(before, after)
        guard.apply_after(before, after)
        if source.read_bytes() == original:
            raise MeasureError("自检：应用补丁后内容未变化")
        try:
            raise RuntimeError("simulated measurement failure")
        except RuntimeError:
            guard.restore()
        if source.read_bytes() != original:
            raise MeasureError("自检：异常路径未能恢复原始字节")
        if hashlib.sha256(source.read_bytes()).hexdigest() != guard.sha256:
            raise MeasureError("自检：恢复后哈希不匹配")
        source.write_bytes(b"corrupt")
        restored = backup.read_bytes()
        if restored != original:
            raise MeasureError("自检：source-backup 不是原始字节")
        source.write_bytes(restored)
        os.chmod(source, recovery["mode"])
        if hashlib.sha256(source.read_bytes()).hexdigest() != recovery["sha256"]:
            raise MeasureError("自检：recovery.json 路径未能恢复")
        crashed = root / "probe.rs"
        crashed.write_bytes(original)
        crashed_guard = SourceGuard.capture(crashed)
        crashed_guard.apply_after(before, after)
        crashed_guard.restore()
        if crashed.read_bytes() != original:
            raise MeasureError("自检：第二次恢复失败")
    return {
        "passed": True,
        "temporary_workspace": True,
        "simulated_failure_restored": True,
        "recovery_json_restore": True,
    }


class RestoreSession:
    """Install signal handlers so the probe file is restored on interrupt."""

    def __init__(self, guard: SourceGuard | None) -> None:
        self.guard = guard
        self._previous: dict[int, Any] = {}

    def install(self, guard: SourceGuard) -> None:
        """Register SIGINT/SIGTERM restore hooks for this probe."""
        self.guard = guard
        for sig in (signal.SIGINT, signal.SIGTERM):
            self._previous[sig] = signal.getsignal(sig)
            signal.signal(sig, self._handle)

    def restore_quiet(self) -> None:
        """Restore if a guard is active; swallow nothing that still leaves drift."""
        if self.guard is None:
            return
        self.guard.restore()

    def _handle(self, signum: int, frame: Any) -> None:
        try:
            self.restore_quiet()
        finally:
            signal.signal(signum, self._previous.get(signum, signal.SIG_DFL))
            raise SystemExit(128 + signum)


def write_recovery(output: Path, guard: SourceGuard, probe: Probe) -> None:
    """Write recovery.json before any probe mutation."""
    write_json(
        output / "recovery.json",
        {
            "source_path": str(probe.path),
            "source_rel": probe.rel_path,
            "sha256": guard.sha256,
            "mode": stat.S_IMODE(guard.mode),
            "backup_file": "source-backup",
            "pid": os.getpid(),
            "created_at": utc_now(),
            "before_present": True,
        },
    )


def measure(args: argparse.Namespace) -> dict[str, Any]:
    """Run one scenario/mode measurement group into an empty output directory."""
    repo = Path(args.repo).resolve()
    spec_path = Path(args.spec).resolve()
    target_dir = Path(args.target_dir).resolve()
    output = Path(args.output).resolve()
    if output.exists():
        if not output.is_dir():
            raise MeasureError(f"--output 不是目录: {output}")
        if any(output.iterdir()):
            raise MeasureError(f"--output 必须是新的空目录: {output}")
    else:
        output.mkdir(parents=True)
    backend = repo / "backend"
    if not (backend / "Cargo.toml").is_file():
        raise MeasureError(f"仓库根缺少 backend/Cargo.toml: {repo}")
    commit = assert_isolated_worktree(repo)
    restore_proof = prove_backup_restore()
    write_json(output / "restore-self-test.json", restore_proof)
    probe = load_probe(spec_path, args.scenario, args.revision, backend)
    assert_source_clean(repo, str(Path("backend") / probe.rel_path))
    host_env = os.environ.copy()
    metadata = cargo_metadata(backend, host_env)
    default_target = Path(metadata["target_directory"]).resolve()
    if target_dir == default_target:
        raise MeasureError("CARGO_TARGET_DIR 不能使用日常 cargo metadata 目标目录")
    lock_path: Path | None = None
    guard: SourceGuard | None = None
    session = RestoreSession(None)
    summary: dict[str, Any] = {
        "scenario": args.scenario,
        "mode": args.mode,
        "revision": args.revision,
        "commit": commit,
        "valid_samples": 0,
        "seconds": [],
        "median_seconds": None,
        "source_restored": False,
        "noop_all_business_units_fresh": False,
        "measurement_passed": False,
        "selected_package": probe.package,
        "probe_path": probe.rel_path,
    }
    try:
        lock_path = acquire_target_lock(target_dir)
        guard = SourceGuard.capture(probe.path)
        session.install(guard)
        guard.write_backup(output / "source-backup")
        write_recovery(output, guard, probe)
        guard.ensure_before(probe.before, probe.after)
        write_json(output / "environment.json", collect_environment(backend, target_dir, commit))
        write_json(
            output / "metadata.json",
            {
                "workspace_root": metadata.get("workspace_root"),
                "target_directory_default": str(default_target),
                "target_directory_measured": str(target_dir),
                "commit": commit,
                "packages": [
                    pkg["name"]
                    for pkg in metadata.get("packages", [])
                    if pkg.get("id") in set(metadata.get("workspace_members") or [])
                ],
            },
        )
        write_json(output / "reverse-dependencies.json", reverse_dependencies(metadata, probe.package))
        write_json(
            output / "probe.json",
            {
                "id": probe.id,
                "path": probe.rel_path,
                "package": probe.package,
                "symbol": probe.symbol,
                "before": probe.before,
                "after": probe.after,
                "equivalence": probe.equivalence,
                "sha256_before": guard.sha256,
            },
        )
        warmup = run_cargo(
            backend=backend,
            mode=args.mode,
            target_dir=target_dir,
            output_dir=output / "warmup",
            env=host_env,
            time_it=False,
        )
        noop = run_cargo(
            backend=backend,
            mode=args.mode,
            target_dir=target_dir,
            output_dir=output / "noop",
            env=host_env,
            time_it=False,
        )
        noop_dirty = business_dirty(noop["units"])
        dirty_custom_builds = [
            item
            for item in noop["units"]["dirty"]
            if "custom-build" in (item.get("target_kind") or [])
        ]
        # build-script-executed is also emitted for cached scripts; only dirty
        # compiler-artifact (including custom-build) proves a real rebuild.
        summary["noop_all_business_units_fresh"] = not noop_dirty
        if noop_dirty or dirty_custom_builds:
            excerpt = fingerprint_excerpt((output / "noop" / "cargo.stderr.log").read_text(encoding="utf-8"))
            write_json(
                output / "noop-fingerprint.json",
                {
                    "dirty": noop_dirty,
                    "dirty_custom_builds": dirty_custom_builds,
                    "build_script_messages": noop["units"]["build_scripts"],
                    "stderr_excerpt": excerpt,
                },
            )
            raise MeasureError(
                "无修改构建仍编译业务单元或非 Fresh 的 build script，已记录 fingerprint 原因"
            )
        guard.apply_after(probe.before, probe.after)
        run_cargo(
            backend=backend,
            mode=args.mode,
            target_dir=target_dir,
            output_dir=output / "warmup" / "after",
            env=host_env,
            time_it=False,
        )
        guard.restore()
        run_cargo(
            backend=backend,
            mode=args.mode,
            target_dir=target_dir,
            output_dir=output / "warmup" / "restore",
            env=host_env,
            time_it=False,
        )
        if guard.current_sha256() != guard.sha256:
            raise MeasureError("after 预热恢复后源文件哈希不匹配")
        seconds: list[float] = []
        for index in range(1, args.samples + 1):
            sample_dir = output / f"sample-{index:02d}"
            if guard.current_sha256() != guard.sha256:
                raise MeasureError(f"sample-{index:02d} 开始前源文件不是 before 状态")
            guard.apply_after(probe.before, probe.after)
            timed = run_cargo(
                backend=backend,
                mode=args.mode,
                target_dir=target_dir,
                output_dir=sample_dir,
                env=host_env,
                time_it=True,
            )
            selected_dirty = package_dirty(timed["units"], probe.package)
            if not selected_dirty:
                raise MeasureError(
                    f"sample-{index:02d} 选定业务 crate {probe.package} 没有 dirty 编译单元，不是有效编辑样本"
                )
            seconds.append(float(timed["seconds"]))
            guard.restore()
            run_cargo(
                backend=backend,
                mode=args.mode,
                target_dir=target_dir,
                output_dir=sample_dir / "restore-before",
                env=host_env,
                time_it=False,
            )
        summary["seconds"] = seconds
        summary["valid_samples"] = len(seconds)
        if len(seconds) != args.samples:
            raise MeasureError("有效样本不足")
        summary["median_seconds"] = median(seconds)
        guard.restore()
        summary["source_restored"] = guard.current_sha256() == guard.sha256
        if not summary["source_restored"]:
            raise MeasureError("最终源文件未恢复")
        summary["measurement_passed"] = (
            summary["valid_samples"] == args.samples
            and summary["source_restored"]
            and summary["noop_all_business_units_fresh"]
            and summary["median_seconds"] is not None
        )
        summary["warmup_untimed"] = True
        summary["restore_self_test"] = restore_proof
        return summary
    finally:
        try:
            session.restore_quiet()
            if guard is not None and guard.current_sha256() == guard.sha256:
                summary["source_restored"] = True
        except Exception as error:  # noqa: BLE001 — restore must not raise past summary write
            summary["restore_error"] = str(error)
        write_json(output / "summary.json", summary)
        if lock_path is not None:
            try:
                lock_path.unlink()
            except OSError:
                pass


def build_parser() -> argparse.ArgumentParser:
    """Construct the compile-measurement command line."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", help="专用 git worktree 的仓库根绝对路径")
    parser.add_argument("--revision", choices=REVISIONS)
    parser.add_argument("--spec", help="compile-probes.json 绝对路径")
    parser.add_argument("--scenario", choices=SCENARIOS)
    parser.add_argument("--mode", choices=MODES)
    parser.add_argument("--target-dir", help="本组独立 CARGO_TARGET_DIR")
    parser.add_argument("--output", help="新的空证据目录")
    parser.add_argument("--samples", type=int, default=5)
    parser.add_argument(
        "--self-test-only",
        action="store_true",
        help="只在临时目录验证备份/失败恢复，不测量",
    )
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    """Entry point: optional restore self-test, otherwise one measurement group."""
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.samples != 5:
        print("错误: --samples 必须为 5", file=sys.stderr)
        return 2
    try:
        if args.self_test_only:
            proof = prove_backup_restore()
            json.dump(proof, sys.stdout, ensure_ascii=False, indent=2)
            sys.stdout.write("\n")
            return 0
        missing = [
            name
            for name in ("repo", "revision", "spec", "scenario", "mode", "target_dir", "output")
            if getattr(args, name) in (None, "")
        ]
        if missing:
            parser.error("测量模式缺少参数: " + ", ".join(f"--{name.replace('_', '-')}" for name in missing))
        summary = measure(args)
        json.dump(summary, sys.stdout, ensure_ascii=False, indent=2)
        sys.stdout.write("\n")
        return 0 if summary.get("measurement_passed") else 1
    except MeasureError as error:
        print(f"错误: {error}", file=sys.stderr)
        return 1
    except Exception:  # noqa: BLE001 — last-resort restore already ran in measure()
        traceback.print_exc()
        return 1


if __name__ == "__main__":
    sys.exit(main())
