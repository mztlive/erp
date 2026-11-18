#!/usr/bin/env python3
"""独立运行 server 的同一份目录生成器，不编译业务 crate。"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

BACKEND = Path(__file__).resolve().parent.parent
SERVER = BACKEND / "apps/erp-server"
CLIENT_OUTPUT = BACKEND.parent / "erp-client/lib"
GENERATED_FILES = ("permissions.generated.ts", "audit-actions.generated.ts")


# 为独立目录生成构造临时 Cargo 清单，复用工作区依赖和服务端 `build.rs`，不编译业务 crate。
def tool_manifest() -> str:
    workspace = tomllib.loads((BACKEND / "Cargo.toml").read_text())["workspace"]["dependencies"]
    versions = {name: workspace[name] if isinstance(workspace[name], str) else workspace[name]["version"]
                for name in ("syn", "serde_json")}
    return f'''[package]
name = "erp-permission-generator"
version = "0.0.0"
edition = "2024"
publish = false

[[bin]]
name = "erp-permission-generator"
path = {json.dumps(str(SERVER / "build.rs"))}

[dependencies]
erp-permissions = {{ path = {json.dumps(str(BACKEND / "crates/foundation/erp-permissions"))} }}
syn = {{ version = {json.dumps(versions["syn"])}, features = ["full", "visit"] }}
serde_json = {{ version = {json.dumps(versions["serde_json"])} }}
'''


# 在指定目录编译并运行服务端的实际目录生成器，按需再运行内联单测；任何命令失败都阻断后续交付。
def run_generator(directory: Path, output: Path, test: bool) -> None:
    manifest = directory / "Cargo.toml"
    manifest.write_text(tool_manifest())
    shutil.copyfile(BACKEND / "Cargo.lock", directory / "Cargo.lock")
    intermediate = directory / "generated"
    intermediate.mkdir()
    environment = os.environ.copy()
    environment.update({
        "ERP_PERMISSION_MANIFEST_DIR": str(SERVER),
        "ERP_PERMISSION_OUTPUT_DIR": str(output),
        "OUT_DIR": str(intermediate),
    })
    environment.pop("ERP_TEST_MONGO_URI", None)
    # cargo metadata 遵守当前 workspace 的 target-dir 及 CARGO_TARGET_DIR 配置。
    metadata = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"],
        cwd=BACKEND, check=True, capture_output=True, text=True,
    )
    target = Path(json.loads(metadata.stdout)["target_directory"]) / "permission-generator"
    command = ["cargo", "run", "--quiet", "--manifest-path", str(manifest),
               "--target-dir", str(target), "--bin", "erp-permission-generator"]
    subprocess.run(command, cwd=BACKEND, env=environment, check=True, stdout=subprocess.DEVNULL)
    if test:
        command[1] = "test"
        subprocess.run(command, cwd=BACKEND, env=environment, check=True)


def main() -> int:
    """按命令行模式生成权限与审计目录、逐字节检查客户端生成物，或验证生成器内联单测，避免发布过期目录。"""
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--check", action="store_true", help="生成到临时目录并逐字节检查，不修改生成物")
    modes.add_argument("--test", action="store_true", help="运行实际生成器的内联单元测试")
    arguments = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="erp-permissions-") as temporary:
        directory = Path(temporary)
        output = directory / "client" if arguments.check or arguments.test else CLIENT_OUTPUT
        run_generator(directory, output, arguments.test)
        if arguments.test:
            return 0
        if arguments.check:
            changed = [name for name in GENERATED_FILES
                       if not (CLIENT_OUTPUT / name).is_file()
                       or (CLIENT_OUTPUT / name).read_bytes() != (output / name).read_bytes()]
            if changed:
                print("目录生成物已漂移：" + ", ".join(changed))
                print("在 backend/ 执行 python3 scripts/generate-permissions.py 后 review 生成物。")
                return 1
            print("权限与审计目录生成物一致。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
