"""离线发布清单测试；只执行本地 Helm lint/package/template，不连接集群。"""
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
        for directory in ("deploy/helm/erp",):
            shutil.copytree(source / directory, self.root / directory)
        scripts = self.root / "deploy/jenkins"
        scripts.mkdir(parents=True)
        shutil.copy(source / "deploy/jenkins/render.py", scripts)
        shutil.copy(source / "deploy/jenkins/environment.py", scripts)
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
            "DEPLOY_ENV": "production",
            "NEXT_PUBLIC_API_BASE_URL": "https://erp-api.fushangyunfu.com",
            "IMAGE_PULL_SECRET": "tcr-pull",
        }

        self.write_build_context()

    def write_build_context(self):
        values = json.loads((self.root / f"deploy/helm/erp/environments/{self.env['DEPLOY_ENV']}.json").read_text())
        (self.artifacts / "build-context.json").write_text(json.dumps({
            "DEPLOY_ENV": self.env["DEPLOY_ENV"],
            "NEXT_PUBLIC_API_BASE_URL": "https://" + values["ingress"]["apiHost"],
            "API_REPOSITORY": self.env["API_REPOSITORY"], "WEB_REPOSITORY": self.env["WEB_REPOSITORY"],
        }))

    def render(self):
        return subprocess.run(
            ["python3", "deploy/jenkins/render.py"], cwd=self.root, env=self.env,
            capture_output=True, text=True,
        )

    def test_production_release(self):
        production = self.root / "deploy/helm/erp/environments/production.json"
        before = production.read_bytes()
        result = self.render()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = (self.artifacts / "manifests.yaml").read_text()
        release = json.loads((self.artifacts / "release.json").read_text())
        self.assertIn('image: "' + release["api_image"] + '"', manifest)
        self.assertIn('image: "' + release["web_image"] + '"', manifest)
        self.assertEqual(manifest.count("imagePullSecrets:"), 2)
        self.assertEqual(manifest.count("replicas: 1"), 2)
        self.assertNotIn("replicas: 2", manifest)
        self.assertEqual(manifest.count("kind: PodDisruptionBudget"), 2)
        self.assertEqual(manifest.count("minAvailable: 0"), 2)
        self.assertIn("kind: TkeServiceConfig", manifest)
        self.assertIn('ingress.cloud.tencent.com/enable-group: "true"', manifest)
        self.assertIn('kubernetes.io/ingress.existLbId: "lb-gpk8k2ps"', manifest)
        self.assertNotIn("defaultServer:", manifest)
        self.assertIn('secretName: "erp-api-config"', manifest)
        self.assertIn('secretName: "fsytsl-wsk87cm7"', manifest)
        self.assertNotIn("kind: Secret\n", manifest)
        self.assertNotIn("kind: Namespace\n", manifest)
        self.assertNotIn("replace-with-release", manifest)
        self.assertIn("serviceAccountName: erp-api", manifest)
        self.assertIn("app.kubernetes.io/name: erp-api", manifest)
        self.assertIn("name: erp-api", manifest)
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

    def test_test_environment_isolated(self):
        self.env["DEPLOY_ENV"] = "test"
        self.write_build_context()
        result = self.render()
        self.assertEqual(result.returncode, 0, result.stderr)
        manifest = (self.artifacts / "manifests.yaml").read_text()
        self.assertEqual(manifest.count("namespace: test"), 10)
        self.assertNotIn("namespace: prod", manifest)
        self.assertIn("erp-api-test.fushangyunfu.com", manifest)
        self.assertIn("erp-test.fushangyunfu.com", manifest)
        self.assertNotIn("erp-api.fushangyunfu.com", manifest)
        self.assertIn("deployment.environment.name=test", manifest)
        self.assertEqual(json.loads((self.artifacts / "release.json").read_text())["environment"], "test")

    def test_environment_must_be_explicit(self):
        for environment in ("", "staging", "../production"):
            self.env["DEPLOY_ENV"] = environment
            self.assertNotEqual(self.render().returncode, 0)
            self.assertFalse((self.artifacts / "manifests.yaml").exists())

    def test_chart_rejects_wrong_namespace_or_release(self):
        self.assertEqual(self.render().returncode, 0)
        for release, namespace in (("erp", "test"), ("other", "prod")):
            result = subprocess.run([
                "helm", "template", release, str(self.artifacts / "chart.tgz"),
                "-n", namespace, "-f", str(self.artifacts / "values-release.json"),
            ], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)

    def test_cannot_reuse_release_in_other_environment_or_tamper(self):
        self.assertEqual(self.render().returncode, 0)
        def verify():
            return subprocess.run(["python3", "deploy/jenkins/render.py", "verify"],
                                  cwd=self.root, env=self.env, capture_output=True, text=True)
        self.assertEqual(verify().returncode, 0)
        self.env["DEPLOY_ENV"] = "test"
        self.assertNotEqual(verify().returncode, 0)
        self.env["DEPLOY_ENV"] = "production"
        for name in ("chart.tgz", "values-release.json", "manifests.yaml"):
            path = self.artifacts / name
            before = path.read_bytes()
            path.write_bytes(before + b"\n")
            self.assertNotEqual(verify().returncode, 0)
            path.write_bytes(before)

    def test_cannot_publish_images_built_for_other_environment(self):
        self.env["DEPLOY_ENV"] = "test"
        self.assertNotEqual(self.render().returncode, 0)
        self.assertFalse((self.artifacts / "release.json").exists())

    def test_failed_rerender_invalidates_previous_release(self):
        self.assertEqual(self.render().returncode, 0)
        (self.artifacts / "api-build.json").write_text('{}')
        self.assertNotEqual(self.render().returncode, 0)
        self.assertFalse((self.artifacts / "release.json").exists())

    def test_hosts_cannot_overlap_between_environments(self):
        path = self.root / "deploy/helm/erp/environments/test.json"
        values = json.loads(path.read_text())
        values["ingress"]["apiHost"] = "erp-api.fushangyunfu.com"
        path.write_text(json.dumps(values))
        self.assertNotEqual(self.render().returncode, 0)


if __name__ == "__main__":
    unittest.main()
