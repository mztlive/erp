#!/usr/bin/env python3
"""Print a read-only fingerprint of HEAD, index and tracked/untracked changes."""

import hashlib
import json
import os
import subprocess
from pathlib import Path


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args])


def snapshot(root):
    head = git(root, "rev-parse", "HEAD").decode().strip()
    index = hashlib.sha256(git(root, "ls-files", "--stage", "-z")).hexdigest()
    changed = git(root, "diff", "HEAD", "--no-renames", "--name-only", "-z")
    untracked = git(root, "ls-files", "--others", "--exclude-standard", "-z")
    files = {}
    for raw in sorted(set((changed + untracked).split(b"\0")) - {b""}):
        name = os.fsdecode(raw)
        path = root / name
        if path.is_symlink():
            data = b"symlink\0" + os.fsencode(os.readlink(path))
        elif not path.exists():
            data = b"deleted\0"
        elif path.is_file():
            data = b"file\0" + str(path.stat().st_mode & 0o777).encode() + b"\0" + path.read_bytes()
        else:
            raise ValueError(f"Unsupported changed path: {name}")
        files[name] = hashlib.sha256(data).hexdigest()
    payload = {"head": head, "index": index, "files": files}
    payload["fingerprint"] = hashlib.sha256(
        json.dumps(payload, sort_keys=True, ensure_ascii=True).encode()
    ).hexdigest()
    return payload


if __name__ == "__main__":
    repo = Path(git(Path.cwd(), "rev-parse", "--show-toplevel").decode().strip())
    print(json.dumps(snapshot(repo), sort_keys=True, ensure_ascii=True))
