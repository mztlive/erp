"""离线发布清单测试；只执行本地 kubectl kustomize，不连接集群。"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class RenderTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="erp-render-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        source = Path(__file__).resolve().parents[2]
        for directory in ("deploy/k8s/base", "deploy/k8s/overlays/production"):
            shutil.copytree(source / directory, self.root / directory)
        scripts = self.root / "deploy/jenkins"
        scripts.mkdir(parents=True)
        shutil.copy(source / "deploy/jenkins/render.py", scripts)
        self.artifacts = self.root / "release-artifacts"
        self.artifacts.mkdir()
        for name, character in (("api", "a"), ("web", "b")):
            (self.artifacts / f"{name}-build.json").write_text(json.dumps({
                "containerimage.digest": "sha256:" + character * 64,
            }))
        subprocess.run(["git", "init", "--quiet", str(self.root)], check=True)
        subprocess.run([
            "git", "-c", "user.name=Render Test", "-c", "user.email=test@example.invalid",
            "-c", "commit.gpgsign=false", "commit", "--allow-empty", "--quiet", "-m", "fixture",
        ], cwd=self.root, check=True)
        self.env = {
            **os.environ,
            "API_REPOSITORY": "fushangyun.tencentcloudcr.com/fushangyun/erp-api",
            "WEB_REPOSITORY": "fushangyun.tencentcloudcr.com/fushangyun/erp",
            "BUILD_NUMBER": "42",
            "NEXT_PUBLIC_API_BASE_URL": "https://erp-api.fushangyunfu.com",
            "IMAGE_PULL_SECRET": "tcr-pull",
        }

    def render(self):
        return subprocess.run(
            ["python3", "deploy/jenkins/render.py"], cwd=self.root, env=self.env,
            capture_output=True, text=True,
        )

    def test_production_release(self):
        production = self.root / "deploy/k8s/overlays/production/kustomization.yaml"
        before = production.read_bytes()
        result = self.render()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = (self.artifacts / "manifests.yaml").read_text()
        release = json.loads((self.artifacts / "release.json").read_text())
        self.assertIn("image: " + release["api_image"], manifest)
        self.assertIn("image: " + release["web_image"], manifest)
        self.assertEqual(manifest.count("imagePullSecrets:"), 2)
        self.assertEqual(manifest.count("replicas: 1"), 2)
        self.assertNotIn("replicas: 2", manifest)
        self.assertEqual(manifest.count("kind: PodDisruptionBudget"), 2)
        self.assertEqual(manifest.count("minAvailable: 0"), 2)
        self.assertIn("kind: TkeServiceConfig", manifest)
        self.assertIn('ingress.cloud.tencent.com/enable-group: "true"', manifest)
        self.assertIn("kubernetes.io/ingress.existLbId: lb-gpk8k2ps", manifest)
        self.assertNotIn("defaultServer:", manifest)
        self.assertIn("secretName: erp-api-config", manifest)
        self.assertIn("secretName: fsytsl-wsk87cm7", manifest)
        self.assertNotIn("kind: Secret\n", manifest)
        self.assertNotIn("kind: Namespace\n", manifest)
        self.assertNotIn("replace-with-release", manifest)
        self.assertEqual(production.read_bytes(), before)

    def test_tke_managed_pull(self):
        self.env["IMAGE_PULL_SECRET"] = ""
        result = self.render()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn("imagePullSecrets", (self.artifacts / "manifests.yaml").read_text())

    def test_invalid_digest_fails_before_manifest(self):
        (self.artifacts / "api-build.json").write_text('{"containerimage.digest":"latest"}')
        self.assertNotEqual(self.render().returncode, 0)
        self.assertFalse((self.artifacts / "manifests.yaml").exists())

    def test_missing_digest_fails_before_manifest(self):
        (self.artifacts / "api-build.json").write_text('{}')
        self.assertNotEqual(self.render().returncode, 0)
        self.assertFalse((self.artifacts / "manifests.yaml").exists())

    def test_invalid_pull_secret_fails_before_manifest(self):
        self.env["IMAGE_PULL_SECRET"] = "invalid/name"
        self.assertNotEqual(self.render().returncode, 0)
        self.assertFalse((self.artifacts / "manifests.yaml").exists())


if __name__ == "__main__":
    unittest.main()
