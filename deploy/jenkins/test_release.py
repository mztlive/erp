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
if 'secret' in args:
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
        self.env = {
            **os.environ, "PATH": f"{binaries}:{os.environ['PATH']}",
            "COMMAND_LOG": str(self.log), "KUBE_CONTEXT": "test-context",
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
        commands = [json.loads(line) for line in self.log.read_text().splitlines()]
        return result, commands

    def test_success_checks_rollout_images_and_https(self):
        result, commands = self.deploy("success")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(sum("rollout" in command for command in commands), 2)
        self.assertEqual(sum(command[0] == "curl" for command in commands), 2)
        self.assertTrue((self.root / "release-artifacts/deployment-status.txt").exists())
        for command in commands:
            if command[0] == "kubectl":
                self.assertEqual(command[1:5], ["--context", "test-context", "--namespace", "prod"])

    def test_missing_secret_prevents_any_apply(self):
        result, commands = self.deploy("missing-secret")
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


if __name__ == "__main__":
    unittest.main()
