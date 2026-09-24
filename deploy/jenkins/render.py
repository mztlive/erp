#!/usr/bin/env python3
"""从构建结果生成独立发布清单，不修改仓库中的生产 overlay。"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile


def main():
    root = Path(__file__).resolve().parents[2]
    artifacts = root / "release-artifacts"
    images = []
    for name, metadata, variable in (
        ("erp-api", "api-build.json", "API_REPOSITORY"),
        ("erp-client", "web-build.json", "WEB_REPOSITORY"),
    ):
        digest = json.loads((artifacts / metadata).read_text())["containerimage.digest"]
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise ValueError(f"无效镜像 digest: {metadata}")
        repository = os.environ[variable]
        if not re.fullmatch(r"[a-z0-9][a-z0-9.:-]*/[a-z0-9_./-]+", repository):
            raise ValueError(f"无效镜像仓库: {variable}")
        images.append({"name": name, "newName": repository, "digest": digest})

    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
    release = {
        "commit": commit,
        "build": os.environ["BUILD_NUMBER"],
        "api_image": f"{images[0]['newName']}@{images[0]['digest']}",
        "web_image": f"{images[1]['newName']}@{images[1]['digest']}",
        "api_url": os.environ["NEXT_PUBLIC_API_BASE_URL"],
    }
    pull_secret = os.environ.get("IMAGE_PULL_SECRET", "")
    if pull_secret and not re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?", pull_secret):
        raise ValueError("无效 IMAGE_PULL_SECRET")

    with tempfile.TemporaryDirectory(prefix="erp-release-") as temporary:
        tree = Path(temporary) / "k8s"
        # 只复制清单，不读取本地 secrets/ 配置目录。
        shutil.copytree(root / "deploy/k8s/base", tree / "base")
        shutil.copytree(root / "deploy/k8s/overlays/production", tree / "overlays/production")
        # prod 必须预先存在；流水线不申请 Namespace 等集群级对象的写权限。
        base = tree / "base"
        (base / "kustomization.yaml").write_text(json.dumps({
            "apiVersion": "kustomize.config.k8s.io/v1beta1",
            "kind": "Kustomization", "namespace": "prod",
            "resources": ["erp-api.yaml", "erp-client.yaml", "ingress.yaml"],
        }))
        # 在临时副本中由 kubectl 原生 Kustomize 修改 image 字段。
        overlay = tree / "overlays/production"
        result = subprocess.check_output(["kubectl", "kustomize", str(overlay)], text=True)
        # 保留生产 overlay 的副本数、PDB 与入口，在外层按容器名写入最终镜像。
        bundle = tree / "release"
        bundle.mkdir()
        (bundle / "production.yaml").write_text(result)
        patches = []
        for name, image in (("erp-api", release["api_image"]), ("erp-client", release["web_image"])):
            pod_spec = {"containers": [{"name": name, "image": image}]}
            if pull_secret:
                pod_spec["imagePullSecrets"] = [{"name": pull_secret}]
            patches.append({"patch": json.dumps({
                "apiVersion": "apps/v1", "kind": "Deployment", "metadata": {"name": name, "namespace": "prod"},
                "spec": {"template": {
                    "metadata": {"annotations": {"erp/release": f"{commit}-{release['build']}"}},
                    "spec": pod_spec,
                }},
            })})
        (bundle / "kustomization.yaml").write_text(json.dumps({
            "apiVersion": "kustomize.config.k8s.io/v1beta1", "kind": "Kustomization",
            "resources": ["production.yaml"], "patches": patches,
        }))
        manifests = subprocess.check_output(["kubectl", "kustomize", str(bundle)], text=True)
        if "registry.example.com" in manifests or "replace-with-release" in manifests:
            raise ValueError("发布清单仍包含占位镜像，拒绝发布")
        for key in ("api_image", "web_image"):
            if release[key] not in manifests:
                raise ValueError(f"发布清单缺少预期镜像: {key}")
        (artifacts / "manifests.yaml").write_text(manifests)
    (artifacts / "release.json").write_text(json.dumps(release, indent=2) + "\n")


if __name__ == "__main__":
    main()
