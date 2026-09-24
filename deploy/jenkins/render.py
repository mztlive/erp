#!/usr/bin/env python3
"""归档可复现的 Helm 发布包；verify 在集群访问前检查环境与产物一致性。"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

from environment import configuration

ROOT = Path(__file__).resolve().parents[2]
ARTIFACTS = ROOT / "release-artifacts"


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def verify():
    environment = configuration()
    release = json.loads((ARTIFACTS / "release.json").read_text())
    for key in ("environment", "namespace"):
        if release[key] != environment[key]:
            raise ValueError("发布产物与当前环境不一致")
    for name in ("chart.tgz", "values-release.json", "manifests.yaml"):
        if sha256(ARTIFACTS / name) != release["sha256"][name]:
            raise ValueError(f"发布产物校验失败: {name}")
    values = json.loads((ARTIFACTS / "values-release.json").read_text())
    for key in ("environment", "namespace", "ingress"):
        if values[key] != environment[key]:
            raise ValueError("发布 values 与当前环境配置不一致，请重新构建")
    if values["api"]["configSecret"] != environment["api"]["configSecret"]:
        raise ValueError("发布配置 Secret 不一致")
    if values["imagePullSecret"] != os.environ.get("IMAGE_PULL_SECRET", ""):
        raise ValueError("镜像拉取 Secret 与发布产物不一致")
    for key, component in (("api_image", "api"), ("web_image", "client")):
        if release[key] != values[component]["image"]:
            raise ValueError("发布镜像元数据不一致")


def render():
    for name in ("release.json", "deployment-status.txt"):
        (ARTIFACTS / name).unlink(missing_ok=True)
    values = configuration()
    build = json.loads((ARTIFACTS / "build-context.json").read_text())
    expected = {
        "DEPLOY_ENV": values["environment"],
        "NEXT_PUBLIC_API_BASE_URL": "https://" + values["ingress"]["apiHost"],
        "API_REPOSITORY": os.environ["API_REPOSITORY"],
        "WEB_REPOSITORY": os.environ["WEB_REPOSITORY"],
    }
    if build != expected:
        raise ValueError("镜像构建环境与发布环境不一致；前端必须按目标 API 地址重新构建")
    images = {}
    for component, metadata, variable in (
        ("api", "api-build.json", "API_REPOSITORY"),
        ("client", "web-build.json", "WEB_REPOSITORY"),
    ):
        digest = json.loads((ARTIFACTS / metadata).read_text())["containerimage.digest"]
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise ValueError(f"无效镜像 digest: {metadata}")
        repository = os.environ[variable]
        if not re.fullmatch(r"[a-z0-9][a-z0-9.:-]*/[a-z0-9_./-]+", repository):
            raise ValueError(f"无效镜像仓库: {variable}")
        images[component] = f"{repository}@{digest}"
        values.setdefault(component, {})["image"] = images[component]
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    values["releaseId"] = f"{commit}-{values['environment']}-{os.environ['BUILD_NUMBER']}"
    values["imagePullSecret"] = os.environ.get("IMAGE_PULL_SECRET", "")
    # 先在临时目录完成全部校验，失败时不得留下可误发布的半成品。
    with tempfile.TemporaryDirectory(prefix="erp-helm-") as temporary:
        bundle = Path(temporary)
        value_file = bundle / "values-release.json"
        value_file.write_text(json.dumps(values, indent=2) + "\n")
        chart = ROOT / "deploy/helm/erp"
        subprocess.run(["helm", "lint", str(chart), "--strict", "--namespace", values["namespace"], "-f", str(value_file)], check=True)
        subprocess.run(["helm", "package", str(chart), "--destination", str(bundle)], check=True)
        next(bundle.glob("erp-*.tgz")).rename(bundle / "chart.tgz")
        manifests = subprocess.check_output([
            "helm", "template", "erp", str(bundle / "chart.tgz"),
            "--namespace", values["namespace"], "-f", str(value_file),
        ], text=True)
        (bundle / "manifests.yaml").write_text(manifests)
        release = {
            "commit": commit, "build": os.environ["BUILD_NUMBER"],
            "environment": values["environment"], "namespace": values["namespace"],
            "release": "erp", "api_image": images["api"], "web_image": images["client"],
            "api_url": "https://" + values["ingress"]["apiHost"],
            "sha256": {name: sha256(bundle / name) for name in
                       ("chart.tgz", "values-release.json", "manifests.yaml")},
        }
        for name in release["sha256"]:
            shutil.copyfile(bundle / name, ARTIFACTS / name)
        (ARTIFACTS / "release.json").write_text(json.dumps(release, indent=2) + "\n")


if __name__ == "__main__":
    if sys.argv[1:] == ["verify"]:
        verify()
    elif not sys.argv[1:]:
        render()
    else:
        raise SystemExit("用法: render.py [verify]")
