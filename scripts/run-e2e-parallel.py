#!/usr/bin/env python3
"""Run E2E shards against disposable copies of the configured development database."""

from __future__ import annotations

import argparse
import asyncio
import copy
import datetime
import ipaddress
import json
import math
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import socket
import sys
import tempfile
import time
import tomllib
from urllib.parse import parse_qsl, unquote, urlencode, urlsplit, urlunsplit


ROOT = Path(__file__).resolve().parent.parent
BACKEND = ROOT / "backend"
E2E = ROOT / "e2e"
SCOPE_MUTATING_SPEC = "s2-org-data-scope-browser.spec.ts"
SUPPLY_MUTATING_SPEC = "flow-18-supply-invalid.spec.ts"


class RunnerError(Exception):
    """An actionable runner failure whose message contains no credentials."""


def private_write(path: Path, value: str) -> None:
    """Create a credential-bearing file readable only by its owner."""
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(descriptor, "w") as handle:
        handle.write(value)
    path.chmod(0o600)


def mongo_hosts(uri: str) -> list[str]:
    """Parse seed hosts without returning user information or connection options."""
    if not uri.startswith(("mongodb://", "mongodb+srv://")):
        raise RunnerError("database.uri 必须使用 mongodb:// 或 mongodb+srv://")
    authority = uri.split("://", 1)[1].split("/", 1)[0].rsplit("@", 1)[-1]
    hosts = []
    for endpoint in authority.split(","):
        if endpoint.startswith("["):
            end = endpoint.find("]")
            if end < 0:
                raise RunnerError("MongoDB 主机格式无效（连接信息已隐藏）")
            host = endpoint[1:end]
        else:
            host = endpoint.rsplit(":", 1)[0] if endpoint.count(":") == 1 else endpoint
        host = unquote(host).rstrip(".").lower()
        if not host or any(token in host for token in ("@", "/", "*", "://")):
            raise RunnerError("MongoDB 主机格式无效（连接信息已隐藏）")
        hosts.append(host)
    return hosts


def normalized_host(host: str) -> str:
    """Normalize an exact host allowlist entry, including IPv6 loopback."""
    host = host.strip().strip("[]").rstrip(".").lower()
    try:
        return ipaddress.ip_address(host).compressed
    except ValueError:
        return host


def require_write_authorization(uri: str, env: dict[str, str]) -> bool:
    """Apply the existing remote reset opt-in before restoring any shard database."""
    hosts = mongo_hosts(uri)
    local = uri.startswith("mongodb://") and all(
        host in {"localhost", "host.docker.internal"}
        or (is_ip_loopback(host)) for host in hosts
    )
    if local:
        return False
    allowed = env.get("ERP_RESET_ALLOWED_REMOTE_HOSTS", "")
    entries = allowed.split(",") if allowed else []
    if env.get("E2E_ALLOW_REMOTE_RESET") != "1":
        raise RunnerError(
            "远程 MongoDB 隔离库写入需要显式设置 E2E_ALLOW_REMOTE_RESET=1"
        )
    if not entries:
        # Match reset-db.sh: explicit remote opt-in authorizes the source config's
        # exact hosts, and the lower-level reset still receives its exact allowlist.
        entries = hosts
        env["ERP_RESET_ALLOWED_REMOTE_HOSTS"] = ",".join(hosts)
    if any(not item.strip() or any(token in item for token in ("*", "/", "@", "://")) for item in entries):
        raise RunnerError("ERP_RESET_ALLOWED_REMOTE_HOSTS 必须是精确主机白名单")
    if not {normalized_host(host) for host in hosts}.issubset({normalized_host(host) for host in entries}):
        raise RunnerError("MongoDB 主机不在 ERP_RESET_ALLOWED_REMOTE_HOSTS 精确白名单中")
    return True


def is_ip_loopback(host: str) -> bool:
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def uri_for_database(uri: str, database: str) -> str:
    """Change the default database while preserving implicit authentication scope."""
    parsed = urlsplit(uri)
    query = parse_qsl(parsed.query, keep_blank_values=True)
    if parsed.username is not None and not any(key.lower() == "authsource" for key, _ in query):
        query.append(("authSource", unquote(parsed.path.lstrip("/")) or "admin"))
    return urlunsplit((parsed.scheme, parsed.netloc, f"/{database}", urlencode(query), parsed.fragment))


def toml_value(value: object) -> str:
    """Serialize TOML values without an additional runtime dependency."""
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return repr(value)
    if isinstance(value, (datetime.datetime, datetime.date, datetime.time)):
        return value.isoformat()
    if isinstance(value, list):
        return "[" + ", ".join(toml_value(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{ " + ", ".join(f"{json.dumps(key)} = {toml_value(item)}" for key, item in value.items()) + " }"
    raise RunnerError("配置包含不支持的 TOML 值类型")


def shard_config(source: dict, uri: str, db_name: str, port: int, run_id: str, shard: int) -> str:
    """Copy all source settings and replace only this shard's resource bindings."""
    result = copy.deepcopy(source)
    result["app"].update(port=port, secret=secrets.token_hex(32))
    result["database"].update(uri=uri, db_name=db_name)
    result["s3"]["key_prefix"] = f"e2e/{run_id}/{shard}"
    # Inline tables are valid TOML and preserve any extra application settings.
    return "\n".join(f"{json.dumps(key)} = {toml_value(value)}" for key, value in result.items()) + "\n"


def assert_owned_database(db_name: str, run_id: str, shard: int) -> None:
    """Permit cleanup only for the exact database generated for this shard."""
    expected = f"erp_e2e_{run_id}_{shard}"
    if not re.fullmatch(r"[0-9]{8}t[0-9]{6}_[0-9a-f]{12}", run_id) or shard < 1 or db_name != expected:
        raise RunnerError("拒绝清理不属于当前 E2E shard 的数据库")


def mongo_settings(env: dict[str, str], config: Path = E2E / "local.config.toml") -> dict[str, str]:
    """Read optional local Mongo settings, with explicit environment precedence."""
    local = tomllib.loads(config.read_text()) if config.is_file() else {}
    settings = {}
    for key in ("mongo_data_root", "target_mongo_uri", "mongo_image"):
        value = env.get(f"E2E_{key.upper()}", local.get(key, ""))
        if not isinstance(value, str):
            raise RunnerError(f"本地 E2E 配置 {key} 必须是字符串")
        settings[key] = value
    settings["mongo_binary"] = env.get("E2E_MONGOD_BINARY", local.get("mongo_binary", ""))
    if not isinstance(settings["mongo_binary"], str):
        raise RunnerError("本地 E2E 配置 mongo_binary 必须是字符串")
    settings["mongo_image"] = settings["mongo_image"] or "docker.1ms.run/mongo:8.0"
    return settings


def mounted_data_root(value: str) -> Path:
    """Reject absent volumes instead of creating directories on the internal disk."""
    root = Path(value).expanduser()
    if not root.is_absolute() or not root.is_dir():
        raise RunnerError("E2E_MONGO_DATA_ROOT 必须是已存在的外置卷目录")
    root = root.resolve()
    if sys.platform == "darwin":
        if len(root.parts) < 4 or root.parts[1] != "Volumes":
            raise RunnerError("E2E_MONGO_DATA_ROOT 必须位于已挂载的 /Volumes/<卷名>/ 下")
        mount = Path("/Volumes") / root.parts[2]
    else:
        mount = next((parent for parent in (root, *root.parents) if os.path.ismount(parent)), Path(root.anchor))
    if mount == Path(root.anchor) or not os.path.ismount(mount):
        raise RunnerError("E2E Mongo 外置卷未挂载，拒绝写入内部磁盘")
    return root


def result_tests(report: dict, shard: int) -> list[dict]:
    """Extract actual Playwright statuses and durations, including serial skips."""
    items = []

    def visit(suite: dict) -> None:
        for spec in suite.get("specs", []):
            for test in spec.get("tests", []):
                results = test.get("results", [])
                items.append({
                    "shard": shard,
                    "file": spec.get("file", suite.get("file", "")),
                    "title": spec.get("title", ""),
                    "project": test.get("projectName", ""),
                    "status": test.get("status", "unknown"),
                    "expected_status": test.get("expectedStatus", "passed"),
                    "result_status": results[-1].get("status", "unknown") if results else "unknown",
                    "duration_ms": sum(result.get("duration", 0) for result in results),
                })
        for child in suite.get("suites", []):
            visit(child)

    for suite in report.get("suites", []):
        visit(suite)
    return items


def duration_weights(path: Path = E2E / "fixtures/flow-durations.json") -> dict[str, float]:
    """Read reviewed file durations without learning from failed runtime reports."""
    report = json.loads(path.read_text())
    if report.get("version") != 1 or not isinstance(report.get("duration_seconds"), dict):
        raise RunnerError("E2E 时长权重文件格式无效")
    result = {}
    for file, duration in report["duration_seconds"].items():
        if not isinstance(file, str) or not isinstance(duration, (int, float)) or isinstance(duration, bool) or not math.isfinite(duration) or duration <= 0:
            raise RunnerError("E2E 文件时长权重必须是有限正数")
        result[file] = float(duration)
    return result


def plan_specs(specs: list[str], workers: int, weights: dict[str, float]) -> list[dict]:
    """Balance whole files with LPT and keep the scope-mutating S2 file last."""
    if workers < 1 or not specs or len(specs) != len(set(specs)):
        raise RunnerError("E2E 分组必须包含唯一文件且至少一个 worker")
    groups = [{"files": [], "estimated_seconds": 0.0} for _ in range(min(workers, len(specs)))]
    for spec in sorted(specs, key=lambda file: (-weights.get(Path(file).name, 60.0), file)):
        group = min(groups, key=lambda item: (item["estimated_seconds"], len(item["files"])))
        group["files"].append(spec)
        group["estimated_seconds"] += weights.get(Path(spec).name, 60.0)
    for group in groups:
        suffix = {SUPPLY_MUTATING_SPEC: 1, SCOPE_MUTATING_SPEC: 2}
        group["files"].sort(key=lambda file: (suffix.get(Path(file).name, 0), file))
        group["estimated_seconds"] = round(group["estimated_seconds"], 3)
    assigned = [file for group in groups for file in group["files"]]
    if sorted(assigned) != sorted(specs) or len(assigned) != len(set(assigned)):
        raise RunnerError("E2E 分组覆盖校验失败：每个文件必须且只能执行一次")
    return groups


def file_filters(files: list[str]) -> list[str]:
    """Pass exact file regular expressions to Playwright's CLI file selector."""
    return ["^" + re.escape(str(E2E / file)) + "$" for file in files]


def report_file(file: str) -> str:
    """Normalize Playwright JSON paths to the same identity used by the plan."""
    path = Path(file)
    if path.is_absolute():
        try:
            return str(path.relative_to(E2E))
        except ValueError:
            raise RunnerError("Playwright JSON 报告包含 E2E 目录外的文件") from None
    return str(path) if path.parts and path.parts[0] == "tests" else str(Path("tests") / path)


class Redactor:
    """Keep process logs useful while removing configuration credentials."""

    def __init__(self, config: dict):
        self.values: set[str] = set()
        self.add_config(config)

    def add_config(self, config: dict) -> None:
        for key, value in config.items():
            if isinstance(value, dict):
                self.add_config(value)
            elif isinstance(value, str) and value and (key == "uri" or any(part in key for part in ("password", "secret", "access_key", "session_token"))):
                self.values.add(value)
                if key == "uri":
                    parsed = urlsplit(value)
                    for credential in (parsed.username, parsed.password):
                        if credential:
                            self.values.update({credential, unquote(credential)})

    def __call__(self, text: str) -> str:
        for value in sorted(self.values, key=len, reverse=True):
            text = text.replace(value, "[REDACTED]")
        return re.sub(r"mongodb(?:\+srv)?://[^\s\"']+", "[MONGO_URI_REDACTED]", text)


class Process:
    """A process group owned by this invocation, with a sanitized private log."""

    def __init__(self, process: asyncio.subprocess.Process, reader: asyncio.Task):
        self.process = process
        self.reader = reader

    async def wait(self, timeout: float | None = None) -> int:
        async def completed() -> int:
            result = await self.process.wait()
            await asyncio.shield(self.reader)
            return result

        try:
            return await asyncio.wait_for(completed(), timeout)
        except asyncio.TimeoutError:
            await self.stop()
            raise RunnerError("子进程超时，已停止本次调用创建的进程组") from None
        except asyncio.CancelledError:
            await self.stop()
            raise

    async def stop(self) -> None:
        async def completed() -> None:
            await self.process.wait()
            # A disk/log-reader error must not prevent stopping the owned group.
            await asyncio.gather(asyncio.shield(self.reader), return_exceptions=True)

        if self.process.returncode is None or not self.reader.done():
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                await asyncio.wait_for(completed(), 5)
            except asyncio.TimeoutError:
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                await self.process.wait()
        await asyncio.gather(asyncio.shield(self.reader), return_exceptions=True)


class Runner:
    def __init__(self, args: argparse.Namespace):
        self.args = args
        self.env = dict(os.environ)
        self.run_id = datetime.datetime.now().strftime("%Y%m%dt%H%M%S") + "_" + secrets.token_hex(6)
        self.report_dir = ROOT / "logs" / "e2e" / self.run_id
        self.report_dir.mkdir(parents=True, mode=0o700)
        self.report_dir.chmod(0o700)
        self.started = time.monotonic()
        self.processes: list[Process] = []
        self.mongo_container: str | None = None
        self.mongo_data_dir: Path | None = None
        self.mongo_root: Path | None = None
        self.mongo_data_owned = False
        self.mongo_process: Process | None = None
        self.summary = {"run_id": self.run_id, "target": args.target, "workers": args.workers, "shards": [], "tests": []}

    def progress(self, value: str) -> None:
        print(value, flush=True)

    async def spawn(self, args: list[str], log: Path, env: dict[str, str] | None = None, cwd: Path = ROOT, console: str | None = None, capture: list[str] | None = None) -> Process:
        process = await asyncio.create_subprocess_exec(
            *args, cwd=cwd, env=env or self.env,
            stdin=asyncio.subprocess.DEVNULL, stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.STDOUT, start_new_session=True, limit=4 * 1024 * 1024,
        )
        private_write(log, "")

        async def read() -> None:
            with log.open("a") as handle:
                while line := await process.stdout.readline():
                    raw = line.decode("utf-8", errors="replace")
                    if capture is not None:
                        capture.append(raw)
                    value = self.redact(raw)
                    handle.write(value)
                    handle.flush()
                    if console:
                        self.progress(f"[{console}] {value.rstrip()}")

        managed = Process(process, asyncio.create_task(read()))
        self.processes.append(managed)
        return managed

    async def command(self, args: list[str], log: Path, env: dict[str, str] | None = None, cwd: Path = ROOT, timeout: float = 180, console: str | None = None, capture: list[str] | None = None) -> None:
        managed = await self.spawn(args, log, env, cwd, console, capture)
        code = await managed.wait(timeout)
        if code:
            raise RunnerError(f"{Path(args[0]).name} 执行失败（退出码 {code}），日志：{log}")

    async def health(self, api_base: str, backend: Process) -> None:
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            if backend.process.returncode is not None:
                raise RunnerError("隔离 web-api 启动失败，详见该 shard 的 backend.log")
            try:
                reader, writer = await asyncio.wait_for(asyncio.open_connection("127.0.0.1", int(api_base.rsplit(":", 1)[1])), 2)
                writer.write(b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                await writer.drain()
                status = await asyncio.wait_for(reader.readline(), 3)
                writer.close()
                await writer.wait_closed()
                if re.match(rb"HTTP/1\.[01] 2[0-9]{2}\b", status):
                    return
            except (OSError, asyncio.TimeoutError):
                pass
            await asyncio.sleep(0.3)
        raise RunnerError("隔离 web-api 在 120 秒内未就绪")

    async def start_managed_mongo(self, settings: dict[str, str]) -> str:
        """Own one temporary replica set whose entire data directory is external."""
        self.mongo_root = mounted_data_root(settings["mongo_data_root"])
        binary = Path(settings["mongo_binary"]).expanduser().resolve() if settings.get("mongo_binary") else None
        if binary is not None:
            if not binary.is_file() or not os.access(binary, os.X_OK):
                raise RunnerError("E2E_MONGOD_BINARY 必须指向本机可执行的 mongod")
        else:
            if not shutil.which("docker"):
                raise RunnerError("外置 Mongo Docker 模式需要本机 Docker")
            await self.command(["docker", "image", "inspect", settings["mongo_image"], "--format", "{{.Id}}"], self.report_dir / "mongo-image.log", timeout=30)
        self.mongo_data_dir = self.mongo_root / self.run_id
        self.mongo_data_dir.mkdir(mode=0o700)
        self.mongo_data_owned = True
        self.summary["managed_mongo"] = {"engine": "native" if binary else "docker", "data_directory": str(self.mongo_data_dir), "cleanup": "pending"}
        self.mongo_data_dir.chmod(0o700)
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        self.summary["managed_mongo"]["port"] = port
        self.progress(f"临时 Mongo 使用外置卷 {self.mongo_data_dir}，端口 {port}")
        if binary:
            self.summary["managed_mongo"]["binary"] = binary.name
            process = await self.spawn([
                str(binary), "--dbpath", str(self.mongo_data_dir), "--port", str(port),
                "--bind_ip", "127.0.0.1", "--replSet", "erp_e2e", "--oplogSize", "128",
            ], self.report_dir / "mongo-server.log")
            member = f"127.0.0.1:{port}"
            initialize_args = ["mongosh", "--norc", "--quiet"]
        else:
            self.mongo_container = f"erp-e2e-mongo-{self.run_id}"
            self.summary["managed_mongo"]["container"] = self.mongo_container
            process = await self.spawn([
                "docker", "run", "--pull=never", "--name", self.mongo_container,
                "--label", f"erp.e2e.run_id={self.run_id}", "--publish", f"127.0.0.1:{port}:27017",
                "--mount", f"type=bind,source={self.mongo_data_dir},target=/data/db",
                "--tmpfs", "/data/configdb:rw,noexec,nosuid,size=16m", "--log-driver", "none",
                "--user", f"{os.getuid()}:{os.getgid()}", "--entrypoint", "mongod",
                settings["mongo_image"], "--replSet", "erp_e2e", "--bind_ip_all", "--oplogSize", "128",
            ], self.report_dir / "mongo-server.log")
            member = "localhost:27017"
            initialize_args = ["docker", "exec", self.mongo_container, "mongosh", "--norc", "--quiet"]
        self.mongo_process = process
        inside_uri = f"mongodb://{member}/?directConnection=true&serverSelectionTimeoutMS=2000&connectTimeoutMS=1000"
        initialize = f"const result = rs.initiate({{_id:'erp_e2e',members:[{{_id:0,host:{json.dumps(member)}}}]}});\n" + """
if (result.ok !== 1 && result.codeName !== 'AlreadyInitialized') throw new Error('E2E replica initialization failed');
for (let attempt=0; attempt<300; attempt++) { if (db.hello().isWritablePrimary) { print('E2E replica ready'); quit(0); } sleep(200); }
throw new Error('E2E replica election timeout');"""
        deadline = time.monotonic() + 120
        while time.monotonic() < deadline:
            if process.process.returncode is not None:
                raise RunnerError("临时 Mongo 进程启动失败，详见 mongo-server.log")
            try:
                await self.command(initialize_args + [inside_uri, "--eval", initialize], self.report_dir / "mongo-initialize.log", timeout=70)
                return f"mongodb://127.0.0.1:{port}/?replicaSet=erp_e2e&directConnection=true"
            except RunnerError:
                await asyncio.sleep(0.5)
        raise RunnerError("临时 Mongo 副本集在 120 秒内未就绪")

    async def cleanup_managed_mongo(self) -> None:
        """Stop owned Mongo and remove only this run's container and data directory."""
        if not self.mongo_data_owned or self.mongo_data_dir is None:
            return
        expected = f"erp-e2e-mongo-{self.run_id}"
        if (self.mongo_container is not None and self.mongo_container != expected) or self.mongo_data_dir != self.mongo_root / self.run_id:
            raise RunnerError("拒绝清理不属于本次运行的 Mongo 容器或目录")
        if self.mongo_process is not None:
            await self.mongo_process.stop()
        if self.mongo_container is not None:
            captured = []
            inspect = await self.spawn(["docker", "inspect", "--format", '{{ index .Config.Labels "erp.e2e.run_id" }}', expected], self.report_dir / "mongo-cleanup-inspect.log", capture=captured)
            code = await inspect.wait(30)
            if code == 0:
                if "".join(captured).strip() != self.run_id:
                    raise RunnerError("临时 Mongo 容器所有权标记不匹配，拒绝删除")
                await self.command(["docker", "rm", "--force", "--volumes", expected], self.report_dir / "mongo-cleanup.log", timeout=60)
            elif not any(f"No such object: {expected}" in line for line in captured):
                raise RunnerError("无法确认临时 Mongo 容器已停止，保留外置数据目录")
        mounted_data_root(str(self.mongo_root))
        if self.mongo_data_dir.is_symlink() or self.mongo_data_dir.resolve().parent != self.mongo_root:
            raise RunnerError("临时 Mongo 数据目录归属不匹配，拒绝删除")
        if self.mongo_data_dir.exists():
            shutil.rmtree(self.mongo_data_dir)
        self.summary["managed_mongo"]["cleanup"] = "removed"
        self.mongo_data_owned = False

    async def reset(self, config: Path, db_name: str, env: dict[str, str], report_dir: Path) -> None:
        command = ["bash", str(BACKEND / "scripts/reset-dev-business-data.sh"), "--config", str(config)]
        reset_env = {**env, "ERP_RESET_ONLY": "1", "ERP_RESET_E2E": "1", "ERP_RESET_INCLUDE_CATALOG": "0"}
        preview = report_dir / "reset-preview.log"
        preview_output = []
        await self.command(command, preview, reset_env, capture=preview_output)
        match = re.search(r"集合摘要: ([0-9a-f]{64})", "".join(preview_output))
        if not match:
            raise RunnerError("隔离库 reset 预览缺少集合摘要，停止执行")
        digest = match.group(1)
        remote = ["--allow-remote"] if self.remote else []
        await self.command(command + ["--execute", "--confirm-db", db_name, "--expect-summary", digest] + remote, report_dir / "reset-execute.log", reset_env)
        await self.command(command + ["--verify", "--expect-summary", digest], report_dir / "reset-verify.log", reset_env)

    async def cleanup_database(self, db_name: str, shard: int, report_dir: Path) -> None:
        assert_owned_database(db_name, self.run_id, shard)
        script = self.temp / f"drop-{shard}.js"
        private_write(script, f"""const expected = {json.dumps(db_name)};
const name = process.env.ERP_E2E_CLEANUP_DB;
if (name !== expected || !/^erp_e2e_[0-9]{{8}}t[0-9]{{6}}_[0-9a-f]{{12}}_[0-9]+$/.test(name)) throw new Error('Unsafe E2E cleanup target');
const connection = new Mongo(process.env.ERP_E2E_CLEANUP_URI);
const result = connection.getDB(name).dropDatabase();
if (result.ok !== 1) throw new Error('E2E database cleanup failed');
print('Dropped E2E database: ' + name);
""")
        env = {**self.env, "ERP_E2E_CLEANUP_URI": uri_for_database(self.target_uri, db_name), "ERP_E2E_CLEANUP_DB": db_name}
        await self.command(["mongosh", "--nodb", "--norc", "--quiet", "--file", str(script)], report_dir / "cleanup.log", env, timeout=180)

    async def run_files(self, files: list[str], shard: int, config: Path, db_name: str, env: dict[str, str], report_dir: Path) -> dict:
        """Run complete specs serially and clear business state between files."""
        batch = {"status": "passed", "exit_code": 0, "tests": [], "file_runs": [], "durations_seconds": {"playwright": 0.0, "between_spec_reset": 0.0}}
        for index, file in enumerate(files, 1):
            started = time.monotonic()
            directory = report_dir / f"spec-{index:02d}-{Path(file).stem}"
            item = {"file": file, "status": "failed", "reset_seconds": 0.0, "playwright_seconds": 0.0, "report_dir": str(directory / "playwright-report"), "result_json": str(directory / "results.json")}
            spec_env = {**env, "ERP_E2E_OUTPUT_DIR": str(directory / "test-results"), "ERP_E2E_REPORT_DIR": item["report_dir"], "ERP_E2E_RESULT_JSON": item["result_json"]}
            try:
                directory.mkdir(mode=0o700)
                if index > 1 and self.env.get("E2E_RESET", "1") != "0":
                    phase = time.monotonic()
                    try:
                        self.progress(f"[shard {shard}] 清业务数据后执行 {Path(file).name}")
                        await self.reset(config, db_name, spec_env, directory)
                    finally:
                        item["reset_seconds"] = round(time.monotonic() - phase, 3)
                args = ["node", str(E2E / "node_modules/@playwright/test/cli.js"), "test", *file_filters([file]), "--workers=1"]
                if self.env.get("E2E_HEADED") == "1":
                    args.append("--headed")
                if self.env.get("E2E_TRACE") == "1":
                    args.extend(["--trace", "on"])
                self.progress(f"[shard {shard}/{self.args.workers}] Playwright {index}/{len(files)}：{Path(file).name}")
                phase = time.monotonic()
                try:
                    playwright = await self.spawn(args, directory / "playwright.log", spec_env, E2E, f"shard {shard}")
                    code = await playwright.wait()
                finally:
                    item["playwright_seconds"] = round(time.monotonic() - phase, 3)
                item["exit_code"] = code
                item["status"] = "passed" if code == 0 else "failed"
                json_report = Path(item["result_json"])
                if not json_report.exists():
                    raise RunnerError("Playwright 没有生成当前 spec 的 JSON 结果报告")
                tests = result_tests(json.loads(json_report.read_text()), shard)
                batch["tests"].extend(tests)
                if {report_file(test["file"]) for test in tests} != {file}:
                    raise RunnerError("Playwright JSON 文件覆盖与当前 spec 计划不一致")
                if self.env.get("E2E_TRACE") == "1":
                    try:
                        await self.command(["node", "scripts/trace-slow-steps.mjs", spec_env["ERP_E2E_OUTPUT_DIR"]], directory / "trace-slow-steps.log", spec_env, E2E)
                    except RunnerError as error:
                        item["trace_error"] = self.redact(str(error))
            except asyncio.CancelledError:
                item["status"] = "interrupted"
                raise
            except RunnerError as error:
                item.update(status="failed", error=self.redact(str(error)))
                self.progress(f"[shard {shard}] {Path(file).name}：{item['error']}")
            except Exception:
                item.update(status="failed", error="当前 spec 的运行环境或结果报告无效（敏感信息已隐藏）")
                self.progress(f"[shard {shard}] {Path(file).name}：{item['error']}")
            finally:
                item["duration_seconds"] = round(time.monotonic() - started, 3)
                batch["file_runs"].append(item)
                for duration, key in (("playwright_seconds", "playwright"), ("reset_seconds", "between_spec_reset")):
                    batch["durations_seconds"][key] += item[duration]
                if item["status"] != "passed":
                    batch.update(status="failed", exit_code=1)
        if {report_file(test["file"]) for test in batch["tests"]} != set(files):
            batch.update(status="failed", exit_code=1, coverage_error="Playwright JSON 文件覆盖与当前 shard 完整计划不一致")
        batch["durations_seconds"] = {key: round(value, 3) for key, value in batch["durations_seconds"].items()}
        return batch

    async def run_shard(self, shard: int, port: int) -> dict:
        started = time.monotonic()
        report_dir = self.report_dir / f"shard-{shard}"
        report_dir.mkdir(mode=0o700)
        db_name = f"erp_e2e_{self.run_id}_{shard}"
        api_base = f"http://127.0.0.1:{port}"
        config = self.temp / f"shard-{shard}.toml"
        uri = uri_for_database(self.target_uri, db_name)
        text = shard_config(self.source, uri, db_name, port, self.run_id, shard)
        self.redact.add_config(tomllib.loads(text))
        private_write(config, text)
        mongo_config = self.temp / f"mongo-shard-{shard}.yaml"
        # Archive namespace remapping selects the database; a default URI database
        # would also act as mongorestore's legacy --db filter and can skip the dump.
        private_write(mongo_config, json.dumps({"uri": uri_for_database(self.target_uri, "")}))
        plan = self.plan[shard - 1]
        result = {"shard": shard, "database": db_name, "api_base": api_base, "status": "failed", "cleanup": "not_created", "report_dir": str(report_dir), "durations_seconds": {}, "planned_files": plan["files"], "estimated_seconds": plan["estimated_seconds"]}
        backend = None
        database_created = False
        env = {
            **self.env, "API_BASE": api_base, "ERP_E2E_CONFIG_PATH": str(config),
            "ERP_E2E_ISOLATED": "1",
            "ERP_E2E_SOURCE_API_BASE": self.env.get("ERP_E2E_SOURCE_API_BASE", "http://127.0.0.1:10001"),
            "E2E_BASE_URL": self.env.get("E2E_BASE_URL", "http://localhost:3000" if self.env.get("E2E_FRONTEND") == "dev" else f"http://127.0.0.1:{self.env.get('E2E_FRONT_PORT', '3100')}"),
            "ERP_E2E_OUTPUT_DIR": str(report_dir / "test-results"),
            "ERP_E2E_REPORT_DIR": str(report_dir / "playwright-report"),
            "ERP_E2E_RESULT_JSON": str(report_dir / "results.json"),
            "PLAYWRIGHT_HTML_OPEN": "never",
            # Keep all owned APIs in their private logs even when the caller has
            # enabled the backend's optional shared backend/logs file appender.
            "LOG_TO_FILE": "0",
        }
        try:
            phase = time.monotonic()
            self.progress(f"[shard {shard}/{self.args.workers}] 克隆到 {db_name}")
            database_created = True
            # The current API composition root creates authoritative indexes before
            # listening; restore data here and build each collection's indexes once.
            await self.command([
                "mongorestore", f"--config={mongo_config}", f"--archive={self.archive}",
                "--nsInclude", f"{self.source_db}.*", "--nsFrom", f"{self.source_db}.*", "--nsTo", f"{db_name}.*", "--stopOnError", "--noIndexRestore",
            ], report_dir / "restore.log", timeout=600)
            if self.env.get("E2E_RESET", "1") != "0":
                await self.reset(config, db_name, env, report_dir)
            result["durations_seconds"]["database_prepare"] = round(time.monotonic() - phase, 3)
            phase = time.monotonic()
            seed_enabled = self.env.get("E2E_SEED", "1") != "0"
            initial_log = "backend-seed.log" if seed_enabled else "backend.log"
            backend = await self.spawn([str(self.binary), "--config-path", str(config)], report_dir / initial_log, env, BACKEND)
            private_write(report_dir / "backend.pid", str(backend.process.pid) + "\n")
            await self.health(api_base, backend)
            result["durations_seconds"]["backend_startup"] = round(time.monotonic() - phase, 3)
            seed_started = time.monotonic()
            if seed_enabled:
                self.progress(f"[shard {shard}] 补齐固定 E2E 账号、职责与目录种子")
                await self.command(["node", str(ROOT / "scripts/seed-dev-foundation.mjs")], report_dir / "seed-foundation.log", env, timeout=180)
            await self.command(["node", str(ROOT / "scripts/publish-approval-definitions.mjs")], report_dir / "approval-publish.log", env, timeout=180)
            if seed_enabled:
                await self.command(["node", str(ROOT / "scripts/seed-dev-catalog.mjs")], report_dir / "seed-catalog.log", env, timeout=180)
                result["durations_seconds"]["seed"] = round(time.monotonic() - seed_started, 3)
                # Seed scripts authenticate many roles. A fresh owned process gives
                # browser tests their full normal login rate-limit window.
                restart_started = time.monotonic()
                await backend.stop()
                backend = await self.spawn([str(self.binary), "--config-path", str(config)], report_dir / "backend.log", env, BACKEND)
                private_write(report_dir / "backend.pid", str(backend.process.pid) + "\n")
                await self.health(api_base, backend)
                result["durations_seconds"]["backend_restart"] = round(time.monotonic() - restart_started, 3)
            result["durations_seconds"]["backend_prepare"] = round(time.monotonic() - phase, 3)
            batch = await self.run_files(plan["files"], shard, config, db_name, env, report_dir)
            result["durations_seconds"].update(batch.pop("durations_seconds"))
            result.update(batch)
        except RunnerError as error:
            result["status"] = "failed"
            result["error"] = self.redact(str(error))
            self.progress(f"[shard {shard}] {result['error']}")
        except asyncio.CancelledError:
            result["status"] = "interrupted"
            raise
        except Exception:
            result["status"] = "failed"
            result["error"] = "隔离 shard 的运行环境或结果文件无效（敏感信息已隐藏）"
            self.progress(f"[shard {shard}] {result['error']}")
        finally:
            phase = time.monotonic()
            if backend:
                await backend.stop()
            (report_dir / "backend.pid").unlink(missing_ok=True)
            if database_created:
                try:
                    await self.cleanup_database(db_name, shard, report_dir)
                    result["cleanup"] = "dropped"
                except RunnerError as error:
                    result["cleanup"] = "failed"
                    result["cleanup_error"] = self.redact(str(error))
                    result["status"] = "failed"
                    self.progress(f"[shard {shard}] 隔离数据库清理失败：{result['cleanup_error']}")
                except Exception:
                    result["cleanup"] = "failed"
                    result["cleanup_error"] = "隔离数据库清理环境无效（敏感信息已隐藏）"
                    result["status"] = "failed"
                    self.progress(f"[shard {shard}] {result['cleanup_error']}")
            result["durations_seconds"]["cleanup"] = round(time.monotonic() - phase, 3)
            result["duration_seconds"] = round(time.monotonic() - started, 3)
            self.summary["shards"].append(result)
            self.progress(f"[shard {shard}] {result['status']}，{result['duration_seconds']:.1f}s，cleanup={result['cleanup']}")
        return result

    async def run(self) -> int:
        try:
            self.source = tomllib.loads(Path(self.args.config).read_text())
            self.source_uri = self.source["database"]["uri"]
            self.source_db = self.source["database"]["db_name"]
            self.redact = Redactor(self.source)
            if not re.fullmatch(r"[A-Za-z0-9_-]+", self.source_db) or self.source_db in {"admin", "config", "local"}:
                raise RunnerError("拒绝复制系统库或名称包含不支持字符的源数据库")
            self.remote = require_write_authorization(self.source_uri, self.env)
            settings = mongo_settings(self.env)
            self.target_uri = settings["target_mongo_uri"] or self.source_uri
            if settings["target_mongo_uri"]:
                self.remote = require_write_authorization(self.target_uri, self.env)
                self.redact.add_config({"uri": self.target_uri})
                self.summary["mongo_mode"] = "target_uri"
            else:
                self.summary["mongo_mode"] = "managed_external" if settings["mongo_data_root"] else "source_instance"
                if settings["mongo_data_root"]:
                    mounted_data_root(settings["mongo_data_root"])
            for command in ("mongodump", "mongorestore", "mongosh", "node", "cargo"):
                if not shutil.which(command):
                    raise RunnerError(f"未找到 {command}")
            if not (E2E / "node_modules/@playwright/test/cli.js").exists():
                raise RunnerError("缺少 Playwright 依赖，请先在 e2e 执行 npm ci")
            specs = sorted(str(path.relative_to(E2E)) for path in (E2E / "tests").rglob("*.spec.ts")) if self.args.target == "all" else [self.args.target]
            self.plan = plan_specs(specs, self.args.workers, duration_weights())
            self.args.workers = len(self.plan)
            self.summary["workers"] = self.args.workers
            self.summary["plan"] = [{"shard": index, **group} for index, group in enumerate(self.plan, 1)]
            for index, group in enumerate(self.plan, 1):
                self.progress(f"[shard {index}] 预计 {group['estimated_seconds']:.1f}s：{', '.join(Path(file).name for file in group['files'])}")
            with tempfile.TemporaryDirectory(prefix=f"erp-e2e-{self.run_id}-") as temp:
                self.temp = Path(temp)
                metadata_log = self.report_dir / "cargo-metadata.log"
                metadata_output = []
                metadata_process = await self.spawn(["cargo", "metadata", "--format-version", "1", "--no-deps"], metadata_log, cwd=BACKEND, capture=metadata_output)
                if await metadata_process.wait(60):
                    raise RunnerError(f"cargo metadata 执行失败，日志：{metadata_log}")
                # Cargo may emit a diagnostic before its JSON payload.
                metadata = next((json.loads(line) for line in metadata_output if line.startswith("{")), None)
                if not metadata:
                    raise RunnerError("cargo metadata 没有返回 target_directory")
                self.binary = Path(metadata["target_directory"]) / "debug/web-api"
                if not self.binary.is_file() or not os.access(self.binary, os.X_OK):
                    raise RunnerError("未找到 debug/web-api；请先运行 cd backend && cargo build -p web-api --locked")
                self.archive = self.temp / "source.archive"
                mongo_config = self.temp / "mongo-source.yaml"
                private_write(mongo_config, json.dumps({"uri": uri_for_database(self.source_uri, self.source_db)}))
                self.progress(f"E2E run {self.run_id}：只读复制源库一次，{self.args.workers} 个独立 shard")
                phase = time.monotonic()
                await self.command(["mongodump", f"--config={mongo_config}", "--db", self.source_db, f"--archive={self.archive}"], self.report_dir / "dump.log", timeout=600)
                self.summary["dump_seconds"] = round(time.monotonic() - phase, 3)
                if settings["mongo_data_root"] and not settings["target_mongo_uri"]:
                    phase = time.monotonic()
                    self.target_uri = await self.start_managed_mongo(settings)
                    self.remote = False
                    self.summary["mongo_prepare_seconds"] = round(time.monotonic() - phase, 3)
                # Reserve distinct ports while choosing them; release immediately before spawning.
                reservations = []
                try:
                    for _ in range(self.args.workers):
                        reservation = socket.socket()
                        reservation.bind(("0.0.0.0", 0))
                        reservations.append(reservation)
                    ports = [reservation.getsockname()[1] for reservation in reservations]
                finally:
                    for reservation in reservations:
                        reservation.close()
                # Even preparation/cleanup failures outside a shard's main try must
                # await every other shard before deleting shared temporary files.
                results = await asyncio.gather(*(self.run_shard(index, port) for index, port in enumerate(ports, 1)), return_exceptions=True)
                for shard, result in enumerate(results, 1):
                    if isinstance(result, BaseException):
                        existing = next((item for item in self.summary["shards"] if item["shard"] == shard), None)
                        error = "隔离 shard 的准备或清理失败（敏感信息已隐藏）"
                        if existing is None:
                            self.summary["shards"].append({"shard": shard, "status": "failed", "cleanup": "unknown", "error": error})
                        else:
                            existing.update(status="failed", error=error)
                self.summary["status"] = "passed" if all(shard["status"] == "passed" for shard in self.summary["shards"]) else "failed"
        except RunnerError as error:
            self.summary["status"] = "failed"
            self.summary["error"] = str(error)
            self.progress(f"错误: {error}")
        except asyncio.CancelledError:
            self.summary["status"] = "interrupted"
            raise
        except Exception:
            self.summary["status"] = "failed"
            self.summary["error"] = "E2E 配置或运行环境无效（敏感配置已隐藏）"
            self.progress(f"错误: {self.summary['error']}")
        finally:
            for process in self.processes:
                await process.stop()
            if self.mongo_data_owned:
                try:
                    await self.cleanup_managed_mongo()
                except Exception:
                    self.summary["status"] = "failed"
                    self.summary["managed_mongo"]["cleanup"] = "failed"
                    self.summary["managed_mongo"]["cleanup_error"] = "本次临时 Mongo 或外置目录清理失败，请检查 managed_mongo 清理结果"
                    self.progress(self.summary["managed_mongo"]["cleanup_error"])
            self.summary.setdefault("status", "failed")
            self.summary["duration_seconds"] = round(time.monotonic() - self.started, 3)
            self.summary["shards"].sort(key=lambda shard: shard["shard"])
            self.summary["tests"] = [test for shard in self.summary["shards"] for test in shard.get("tests", [])]
            summary_path = self.report_dir / "summary.json"
            private_write(summary_path, json.dumps(self.summary, ensure_ascii=False, indent=2) + "\n")
            self.progress(f"E2E {self.summary['status']}：总耗时 {self.summary['duration_seconds']:.1f}s；报告 {summary_path}")
        return 0 if self.summary["status"] == "passed" else 1


def arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", help="all 或一个 *.spec.ts 文件")
    parser.add_argument("--config", default=str(BACKEND / "config.toml"), help="只读源配置")
    parser.add_argument("--workers", type=int, default=int(os.environ.get("E2E_WORKERS", "6")))
    args = parser.parse_args(argv)
    if not 1 <= args.workers <= 16:
        parser.error("workers 必须在 1 到 16 之间")
    if args.target != "all":
        candidates = [Path(args.target), ROOT / args.target, E2E / args.target, E2E / "tests" / Path(args.target).name]
        candidate = next((path.resolve() for path in candidates if path.is_file()), None)
        if candidate is None or not candidate.is_relative_to(E2E / "tests") or not candidate.name.endswith(".spec.ts"):
            parser.error("target 必须是 e2e/tests 下存在的 *.spec.ts 文件")
        args.target = str(candidate.relative_to(E2E))
        args.workers = 1
    return args


def main() -> int:
    async def run() -> int:
        task = asyncio.current_task()
        asyncio.get_running_loop().add_signal_handler(signal.SIGTERM, task.cancel)
        return await Runner(arguments()).run()

    try:
        return asyncio.run(run())
    except (KeyboardInterrupt, asyncio.CancelledError):
        return 130
    except (OSError, ValueError, KeyError, tomllib.TOMLDecodeError):
        # Never include parser text or exception repr: either may contain a config secret.
        print("错误: E2E 配置或运行环境无效（敏感配置已隐藏）", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
