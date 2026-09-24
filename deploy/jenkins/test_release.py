"""发布命令编排的离线替身测试，不访问 Docker、集群或业务服务。"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="erp-release-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        scripts = self.root / "deploy/jenkins"
        scripts.mkdir(parents=True)
        shutil.copy(Path(__file__).with_name("release.sh"), scripts)
        artifacts = self.root / "release-artifacts"
        artifacts.mkdir()
        (artifacts / "manifests.yaml").write_text("# command fixture\n")
        (artifacts / "release.json").write_text(json.dumps({
            "api_image": "registry/erp-api@sha256:" + "a" * 64,
            "web_image": "registry/erp@sha256:" + "b" * 64,
        }))
        binaries = self.root / "bin"
        binaries.mkdir()
        mock = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
with open(os.environ['COMMAND_LOG'], 'a') as log:
    log.write(json.dumps([tool, *args]) + '\\n')
if tool == 'curl':
    sys.exit(0)
if 'current-context' in args:
    if os.environ.get('CASE') == 'missing-context':
        sys.exit(1)
    print('uploaded-context')
elif 'ingress' in args:
    case = os.environ.get('CASE')
    if case == 'ingress-read-failure':
        sys.exit(1)
    if case != 'new-ingress':
        annotations = {
            'ingress.cloud.tencent.com/enable-group': 'true',
            'kubernetes.io/ingress.existLbId': 'lb-gpk8k2ps',
        }
        if case == 'legacy-ingress':
            annotations.pop('ingress.cloud.tencent.com/enable-group')
        if case == 'wrong-clb':
            annotations['kubernetes.io/ingress.existLbId'] = 'lb-other'
        print(json.dumps({'metadata': {'annotations': annotations}}))
elif 'secret' in args:
    if os.environ.get('CASE') != 'missing-secret':
        print('present', end='')
elif '--dry-run=server' in args and os.environ.get('CASE') == 'dry-run-failure':
    sys.exit(1)
elif 'deployments' in args:
    images = ['registry/erp-api@sha256:' + 'a' * 64, 'registry/erp@sha256:' + 'b' * 64]
    if os.environ.get('CASE') == 'image-mismatch':
        images[0] = 'wrong/image'
    print(json.dumps({'items': [
        {'metadata': {'name': name}, 'spec': {'template': {'spec': {
            'containers': [{'name': name, 'image': image}]
        }}}} for name, image in zip(['web-api', 'erp-client'], images)
    ]}))
'''
        for name in ("kubectl", "curl"):
            executable = binaries / name
            executable.write_text(mock)
            executable.chmod(0o755)
        self.log = self.root / "commands.jsonl"
        self.kubeconfig = self.root / "uploaded-kubeconfig"
        self.kubeconfig.write_text("# mock kubeconfig\n")
        self.env = {
            **os.environ, "PATH": f"{binaries}:{os.environ['PATH']}",
            "COMMAND_LOG": str(self.log), "KUBE_CONTEXT": "test-context",
            "KUBECONFIG": str(self.kubeconfig),
            "KUBE_NAMESPACE": "prod", "IMAGE_PULL_SECRET": "tcr-pull",
            "NEXT_PUBLIC_API_BASE_URL": "https://api.example.invalid",
            "WEB_URL": "https://web.example.invalid",
        }

    def deploy(self, case):
        self.env["CASE"] = case
        result = subprocess.run(
            ["bash", "deploy/jenkins/release.sh", "deploy"], cwd=self.root,
            env=self.env, capture_output=True, text=True,
        )
        commands = [json.loads(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []
        return result, commands

    def test_success_checks_rollout_images_and_https(self):
        result, commands = self.deploy("success")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum("rollout" in command for command in commands), 2)
        self.assertEqual(sum(command[0] == "curl" for command in commands), 2)
        self.assertTrue((self.root / "release-artifacts/deployment-status.txt").exists())
        for command in commands:
            if command[0] == "kubectl":
                self.assertEqual(command[1:7], ["--kubeconfig", str(self.kubeconfig), "--context", "test-context", "--namespace", "prod"])

    def test_blank_or_unset_context_uses_uploaded_default(self):
        for value in ("", None):
            with self.subTest(value=value):
                self.log.write_text("")
                if value is None:
                    self.env.pop("KUBE_CONTEXT", None)
                else:
                    self.env["KUBE_CONTEXT"] = value
                result, commands = self.deploy("success")
                self.assertEqual(result.returncode, 0, result.stderr)
                requests = [c for c in commands if c[0] == "kubectl" and "current-context" not in c]
                self.assertTrue(requests)
                for command in requests:
                    self.assertEqual(command[1:7], ["--kubeconfig", str(self.kubeconfig), "--context", "uploaded-context", "--namespace", "prod"])

    def test_missing_default_context_prevents_cluster_access(self):
        self.env["KUBE_CONTEXT"] = ""
        result, commands = self.deploy("missing-context")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("未设置 current-context", result.stderr)
        self.assertTrue(all("current-context" in command for command in commands))

    def test_missing_uploaded_file_does_not_use_agent_config(self):
        self.kubeconfig.unlink()
        result, commands = self.deploy("success")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(commands, [])

    def test_missing_secret_prevents_any_apply(self):
        result, commands = self.deploy("missing-secret")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any("apply" in command for command in commands))

    def test_new_ingress_can_be_created(self):
        result, commands = self.deploy("new-ingress")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("apply" in command for command in commands))

    def test_incompatible_ingress_or_read_failure_prevents_apply(self):
        for case in ("legacy-ingress", "wrong-clb", "ingress-read-failure"):
            with self.subTest(case=case):
                self.log.write_text("")
                result, commands = self.deploy(case)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any("apply" in command for command in commands))

    def test_dry_run_failure_prevents_live_apply(self):
        result, commands = self.deploy("dry-run-failure")
        self.assertNotEqual(result.returncode, 0)
        applies = [command for command in commands if "apply" in command]
        self.assertEqual(len(applies), 1)
        self.assertIn("--dry-run=server", applies[0])

    def test_image_mismatch_prevents_success_marker(self):
        result, commands = self.deploy("image-mismatch")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(command[0] == "curl" for command in commands))
        self.assertFalse((self.root / "release-artifacts/deployment-status.txt").exists())


class ValidateTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="erp-validate-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        scripts = self.root / "deploy/jenkins"
        scripts.mkdir(parents=True)
        shutil.copy(Path(__file__).with_name("release.sh"), scripts)
        self.binaries = self.root / "bin"
        self.binaries.mkdir()
        self.bash = shutil.which("bash")
        for name in ("bash", "dirname"):
            (self.binaries / name).symlink_to(shutil.which(name))
        for name in ("git", "docker", "kubectl", "python3", "curl", "cargo", "node", "npm", "grep"):
            executable = self.binaries / name
            executable.write_text('#!/bin/sh\nexit 0\n')
            executable.chmod(0o755)

    def validate(self):
        return subprocess.run(
            [self.bash, "deploy/jenkins/release.sh", "validate"], cwd=self.root,
            env={**os.environ, "PATH": str(self.binaries)}, capture_output=True, text=True,
        )

    def test_reports_all_missing_tools(self):
        for name in ("kubectl", "cargo", "npm"):
            (self.binaries / name).unlink()
        result = self.validate()
        self.assertNotEqual(result.returncode, 0)
        for name in ("kubectl", "cargo", "npm"):
            self.assertIn(f"[缺失] {name}", result.stderr)
        self.assertNotIn("检查 Docker Buildx", result.stdout)

    def test_distinguishes_buildx_and_daemon_failures(self):
        for command, message in (("buildx", "Docker Buildx 不可用"), ("info", "Agent 无法访问 Docker daemon")):
            with self.subTest(command=command):
                (self.binaries / "docker").write_text(
                    f'#!/bin/sh\nif [ "$1" = "{command}" ]; then exit 1; fi\nexit 0\n'
                )
                result = self.validate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)

    def test_reports_success(self):
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("工具预检查通过", result.stdout)


if __name__ == "__main__":
    unittest.main()
