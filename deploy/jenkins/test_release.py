"""发布命令编排的离线替身测试，不访问 Docker、集群或业务服务。"""
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


def copy_environment(root):
    source = Path(__file__).resolve().parents[2]
    shutil.copytree(source / "deploy/helm/erp", root / "deploy/helm/erp")
    for name in ("environment.py", "render.py"):
        shutil.copy(source / "deploy/jenkins" / name, root / "deploy/jenkins")


class ReleaseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="erp-release-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        scripts = self.root / "deploy/jenkins"
        scripts.mkdir(parents=True)
        shutil.copy(Path(__file__).with_name("release.sh"), scripts)
        copy_environment(self.root)
        artifacts = self.root / "release-artifacts"
        artifacts.mkdir()
        (artifacts / "manifests.yaml").write_text("# command fixture\n")
        (artifacts / "chart.tgz").write_text("offline chart fixture")
        values = json.loads((self.root / "deploy/helm/erp/environments/production.json").read_text())
        values["api"]["image"] = "registry/erp-api@sha256:" + "a" * 64
        values["client"] = {"image": "registry/erp@sha256:" + "b" * 64}
        values["imagePullSecret"] = "tcr-pull"
        (artifacts / "values-release.json").write_text(json.dumps(values))
        (artifacts / "release.json").write_text(json.dumps({
            "environment": "production", "namespace": "prod",
            "api_image": values["api"]["image"], "web_image": values["client"]["image"],
            "sha256": {name: hashlib.sha256((artifacts / name).read_bytes()).hexdigest()
                       for name in ("chart.tgz", "values-release.json", "manifests.yaml")},
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
    sys.exit(1 if os.environ.get('CASE') == 'http-failure' else 0)
if tool == 'helm':
    if '--dry-run=server' in args and os.environ.get('CASE') in ('dry-run-failure', 'ownership-failure'):
        sys.exit(1)
    if 'upgrade' in args and '--dry-run=server' not in args and os.environ.get('CASE') == 'upgrade-failure':
        sys.exit(1)
    print('[]')
    sys.exit(0)
if 'rollout' in args and os.environ.get('CASE') == 'rollout-failure':
    sys.exit(1)
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
        }}}} for name, image in zip(['erp-api', 'erp-client'], images)
    ]}))
'''
        for name in ("kubectl", "curl", "helm"):
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
            "DEPLOY_ENV": "production", "IMAGE_PULL_SECRET": "tcr-pull",
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
        upgrades = [c for c in commands if c[0] == "helm" and "upgrade" in c]
        self.assertEqual(len(upgrades), 2)
        self.assertIn("--dry-run=server", upgrades[0])
        self.assertIn("--rollback-on-failure", upgrades[1])
        self.assertIn("--wait", upgrades[1])
        self.assertFalse(any("--take-ownership" in c or ("apply" in c and "--dry-run=server" not in c) for c in commands))
        for command in (c for c in commands if c[0] == "helm"):
            self.assertEqual(command[1:7], ["--kubeconfig", str(self.kubeconfig), "--kube-context", "test-context", "--namespace", "prod"])
        self.assertEqual([c[-1] for c in commands if c[0] == "curl"],
                         ["https://erp-api.fushangyunfu.com/health", "https://erp.fushangyunfu.com/"])
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
        self.assertFalse(any("upgrade" in command for command in commands))

    def test_new_ingress_can_be_created(self):
        result, commands = self.deploy("new-ingress")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertTrue(any("upgrade" in command for command in commands))

    def test_incompatible_ingress_or_read_failure_prevents_apply(self):
        for case in ("legacy-ingress", "wrong-clb", "ingress-read-failure"):
            with self.subTest(case=case):
                self.log.write_text("")
                result, commands = self.deploy(case)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(any("upgrade" in command for command in commands))

    def test_dry_run_failure_prevents_live_apply(self):
        result, commands = self.deploy("dry-run-failure")
        self.assertNotEqual(result.returncode, 0)
        applies = [command for command in commands if "apply" in command]
        self.assertEqual(len(applies), 1)
        self.assertIn("--dry-run=server", applies[0])
        self.assertFalse(any("upgrade" in command for command in commands))

    def test_image_mismatch_prevents_success_marker(self):
        result, commands = self.deploy("image-mismatch")
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse(any(command[0] == "curl" for command in commands))
        self.assertFalse((self.root / "release-artifacts/deployment-status.txt").exists())

    def test_wrong_environment_prevents_cluster_access(self):
        self.env["DEPLOY_ENV"] = "test"
        result, commands = self.deploy("success")
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(commands, [])

    def test_failure_never_leaves_stale_success_marker(self):
        marker = self.root / "release-artifacts/deployment-status.txt"
        for case in ("ownership-failure", "upgrade-failure", "rollout-failure", "http-failure"):
            with self.subTest(case=case):
                marker.write_text("previous success")
                self.log.write_text("")
                result, commands = self.deploy(case)
                self.assertNotEqual(result.returncode, 0)
                self.assertFalse(marker.exists())
                if case == "ownership-failure":
                    self.assertFalse(any("upgrade" in c and "--dry-run=server" not in c for c in commands))

    def test_test_environment_uses_test_namespace_and_urls(self):
        self.env["DEPLOY_ENV"] = "test"
        artifacts = self.root / "release-artifacts"
        values = json.loads((artifacts / "values-release.json").read_text())
        environment = json.loads((self.root / "deploy/helm/erp/environments/test.json").read_text())
        for key in ("environment", "namespace", "ingress"):
            values[key] = environment[key]
        (artifacts / "values-release.json").write_text(json.dumps(values))
        release = json.loads((artifacts / "release.json").read_text())
        release.update(environment="test", namespace="test")
        release["sha256"]["values-release.json"] = hashlib.sha256((artifacts / "values-release.json").read_bytes()).hexdigest()
        (artifacts / "release.json").write_text(json.dumps(release))
        result, commands = self.deploy("success")
        self.assertEqual(result.returncode, 0, result.stderr)
        for command in (c for c in commands if c[0] in ("kubectl", "helm")):
            self.assertEqual(command[command.index("--namespace") + 1], "test")
        self.assertEqual([c[-1] for c in commands if c[0] == "curl"],
                         ["https://erp-api-test.fushangyunfu.com/health", "https://erp-test.fushangyunfu.com/"])


class BuildTests(unittest.TestCase):
    def test_cleanup_on_login_failure_build_failure_and_success(self):
        for case, expected in (("login-failure", 17), ("build-failure", 23), ("success", 0)):
            with self.subTest(case=case), tempfile.TemporaryDirectory(prefix="erp-build-test-") as temporary:
                root = Path(temporary)
                scripts = root / "deploy/jenkins"
                scripts.mkdir(parents=True)
                shutil.copy(Path(__file__).with_name("release.sh"), scripts)
                copy_environment(root)
                binaries = root / "bin"
                binaries.mkdir()
                log = root / "commands.jsonl"
                mock = '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
auth = os.environ.get('DOCKER_CONFIG')
with open(os.environ['COMMAND_LOG'], 'a') as log:
    log.write(json.dumps({'tool': Path(sys.argv[0]).name, 'args': args, 'auth': auth}) + '\\n')
if Path(sys.argv[0]).name == 'git':
    print('abc123')
elif args[0] == 'login':
    sys.stdin.read()
    Path(auth, 'config.json').write_text('mock credential')
    if os.environ['CASE'] == 'login-failure':
        print('unauthorized', file=sys.stderr)
        sys.exit(17)
elif args[:2] == ['buildx', 'build'] and os.environ['CASE'] == 'build-failure':
    sys.exit(23)
'''
                for name in ("docker", "git"):
                    file = binaries / name
                    file.write_text(mock)
                    file.chmod(0o755)
                result = subprocess.run(
                    ["bash", "deploy/jenkins/release.sh", "build"], cwd=root,
                    env={**os.environ, "PATH": f"{binaries}:{os.environ['PATH']}",
                         "COMMAND_LOG": str(log), "CASE": case, "TCR_USERNAME": "test",
                         "TCR_PASSWORD": "mock-password-not-for-logs", "REGISTRY_HOST": "example.invalid",
                         "BUILD_NUMBER": "1", "IMAGE_PLATFORM": "linux/amd64", "DEPLOY_ENV": "test",
                         "API_REPOSITORY": "example.invalid/api", "WEB_REPOSITORY": "example.invalid/web",
                         "NEXT_PUBLIC_API_BASE_URL": "https://api.example.invalid"},
                    capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertNotIn("unbound variable", result.stderr)
                self.assertNotIn("mock-password-not-for-logs", result.stdout + result.stderr)
                commands = [json.loads(line) for line in log.read_text().splitlines()]
                auth = next(c['auth'] for c in commands if c['args'][0] == 'login')
                self.assertFalse(Path(auth).exists())
                removals = [c for c in commands if c['args'][:2] == ['buildx', 'rm']]
                self.assertEqual(len(removals), 0 if case == "login-failure" else 1)
                if case == "login-failure":
                    self.assertEqual(len(commands), 1)
                    self.assertIn("TCR 登录失败", result.stderr)
                if case == "success":
                    builds = [c["args"] for c in commands if c["args"][:2] == ["buildx", "build"]]
                    self.assertEqual(len(builds), 2)
                    self.assertIn("example.invalid/web:abc123-test-1", builds[1])
                    self.assertIn("NEXT_PUBLIC_API_BASE_URL=https://erp-api-test.fushangyunfu.com", builds[1])


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
        for name in ("git", "docker", "kubectl", "python3", "curl", "cargo", "node", "npm", "grep", "helm"):
            executable = self.binaries / name
            executable.write_text('#!/bin/sh\nexit 0\n')
            executable.chmod(0o755)
        (self.binaries / "helm").write_text('#!/bin/sh\nprintf "v4.1.3"\n')

    def validate(self, quality="false"):
        return subprocess.run(
            [self.bash, "deploy/jenkins/release.sh", "validate"], cwd=self.root,
            env={**os.environ, "PATH": str(self.binaries), "RUN_QUALITY_CHECKS": quality, "DEPLOY_ENV": "test"}, capture_output=True, text=True,
        )

    def test_reports_all_missing_tools(self):
        for name in ("kubectl", "cargo", "npm"):
            (self.binaries / name).unlink()
        result = self.validate(quality="true")
        self.assertNotEqual(result.returncode, 0)
        for name in ("kubectl", "cargo", "npm"):
            self.assertIn(f"[缺失] {name}", result.stderr)
        self.assertNotIn("检查 Docker Buildx", result.stdout)

    def test_skipped_quality_does_not_require_host_build_tools(self):
        for name in ("cargo", "node", "npm"):
            (self.binaries / name).unlink()
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("质量检查已跳过", result.stdout)

    def test_skipped_quality_still_requires_kubectl(self):
        (self.binaries / "kubectl").unlink()
        result = self.validate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("[缺失] kubectl", result.stderr)

    def test_distinguishes_buildx_and_daemon_failures(self):
        for command, message in (("buildx", "Docker Buildx 不可用"), ("info", "Agent 无法访问 Docker daemon")):
            with self.subTest(command=command):
                (self.binaries / "docker").write_text(
                    f'#!/bin/sh\nif [ "$1" = "{command}" ]; then exit 1; fi\nexit 0\n'
                )
                result = self.validate()
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(message, result.stderr)

    def test_old_helm_is_rejected(self):
        (self.binaries / "helm").write_text('#!/bin/sh\nprintf "v3.19.0"\n')
        result = self.validate()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("Helm 4.x", result.stderr)

    def test_reports_success(self):
        result = self.validate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("工具预检查通过", result.stdout)


if __name__ == "__main__":
    unittest.main()
