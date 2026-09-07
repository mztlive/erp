#!/usr/bin/env python3
"""Run the real static comparator into a new evidence directory.

Replaces capture-domain-contract.py's legacy-layout capture. No runtime assertion
is synthesized here; comparator exit 2 and every raw needs_review are retained.
"""
from __future__ import annotations

import argparse
from pathlib import Path
import subprocess
import sys


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before-repo", type=Path, required=True)
    parser.add_argument("--after-repo", type=Path, required=True)
    parser.add_argument("--phase", choices=[f"{phase:02d}" for phase in range(9, 18)], required=True)
    parser.add_argument("--output", type=Path, required=True, help="不存在的新证据目录；禁止覆盖旧原始记录")
    args = parser.parse_args(argv)
    before, after, output = args.before_repo.resolve(), args.after_repo.resolve(), args.output.resolve()
    for repo in (before, after):
        if not (repo / "backend/Cargo.toml").is_file():
            parser.error(f"--before-repo/--after-repo 必须为含 backend/Cargo.toml 的仓库根: {repo}")
    if output == before or before in output.parents:
        parser.error("output 不得写入只读 before 仓库")
    try:
        output.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(f"不能创建全新证据目录: {error}")
    comparator = Path(__file__).with_name("compare-domain-contracts.py")
    if not comparator.is_file():
        parser.error(f"缺少实际合同比较器: {comparator}")
    command = [sys.executable, str(comparator), "--before-repo", str(before),
               "--after-repo", str(after), "--phase", args.phase, "--output", str(output)]
    return subprocess.run(command, check=False).returncode


if __name__ == "__main__":
    raise SystemExit(main())
