"""Pure runner contract tests; no MongoDB, backend, or browser is started."""

import asyncio
import argparse
import importlib.util
import json
import os
from pathlib import Path
import signal
import stat
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import AsyncMock, Mock, patch
from urllib.parse import parse_qs, urlsplit


SPEC = importlib.util.spec_from_file_location("e2e_runner", Path(__file__).with_name("run-e2e-parallel.py"))
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class IsolationContracts(unittest.TestCase):
    def test_lpt_covers_real_specs_once_keeps_multitest_files_whole_and_s2_last(self):
        files = sorted(str(path.relative_to(runner.E2E)) for path in (runner.E2E / "tests").rglob("*.spec.ts"))
        weights = runner.duration_weights()
        plan = runner.plan_specs(files, 6, weights)
        assigned = [file for group in plan for file in group["files"]]
        self.assertEqual(sorted(assigned), files)
        self.assertEqual(len(assigned), len(set(assigned)))
        self.assertEqual(len(plan), 6)
        self.assertEqual(sum(Path(file).name == "flow-16-customer-refund.spec.ts" for file in assigned), 1)
        self.assertEqual(sum(Path(file).name == runner.SCOPE_MUTATING_SPEC for file in assigned), 1)
        s2_group = next(group for group in plan if any(Path(file).name == runner.SCOPE_MUTATING_SPEC for file in group["files"]))
        self.assertEqual(Path(s2_group["files"][-1]).name, runner.SCOPE_MUTATING_SPEC)
        # The reviewed concurrent durations target less than 240 s of business
        # work per group before database/backend preparation is added.
        self.assertLess(max(group["estimated_seconds"] for group in plan), 240)
        self.assertEqual(plan, runner.plan_specs(list(reversed(files)), 6, weights))

    def test_exact_file_filters_exclude_similar_names(self):
        files = ["tests/flow-16-customer-refund.spec.ts", "tests/s2-org-data-scope-browser.spec.ts"]
        filters = runner.file_filters(files)
        import re
        for file, pattern in zip(files, filters):
            self.assertTrue(re.fullmatch(pattern, str(runner.E2E / file)))
            self.assertFalse(re.fullmatch(pattern, str(runner.E2E / (file + ".copy"))))
            self.assertFalse(re.fullmatch(pattern, str(runner.E2E / file.replace(".spec.", "XspecX"))))
        self.assertEqual(runner.report_file("flow-16-customer-refund.spec.ts"), files[0])
        self.assertEqual(runner.report_file(files[0]), files[0])
        self.assertEqual(runner.report_file(str(runner.E2E / files[0])), files[0])

    def test_plan_refuses_duplicate_specs_and_handles_one_worker(self):
        files = ["tests/flow-16-customer-refund.spec.ts", "tests/s2-org-data-scope-browser.spec.ts"]
        plan = runner.plan_specs(files, 1, runner.duration_weights())
        self.assertEqual(plan[0]["files"], files)
        with self.assertRaises(runner.RunnerError):
            runner.plan_specs([files[0], files[0]], 2, {})
        suffix_files = [f"tests/{runner.SCOPE_MUTATING_SPEC}", f"tests/{runner.SUPPLY_MUTATING_SPEC}", "tests/flow-19-void-release-reservation.spec.ts", "tests/s1-owner-query-mock.spec.ts"]
        suffix_plan = runner.plan_specs(suffix_files, 1, {})
        self.assertEqual(suffix_plan[0]["files"][-2:], [f"tests/{runner.SUPPLY_MUTATING_SPEC}", f"tests/{runner.SCOPE_MUTATING_SPEC}"])

    def test_local_mongo_settings_are_overridden_by_environment(self):
        with tempfile.TemporaryDirectory() as temp:
            config = Path(temp) / "local.config.toml"
            config.write_text("mongo_data_root = '/Volumes/Drive/local'\nmongo_image = 'existing-image:8'\nmongo_binary = '/Volumes/Drive/local/bin/mongod'\n")
            self.assertEqual(runner.mongo_settings({}, config)["mongo_data_root"], "/Volumes/Drive/local")
            self.assertEqual(runner.mongo_settings({}, config)["mongo_binary"], "/Volumes/Drive/local/bin/mongod")
            settings = runner.mongo_settings({"E2E_MONGO_DATA_ROOT": "/Volumes/Drive/env", "E2E_MONGO_IMAGE": "override-image:8", "E2E_TARGET_MONGO_URI": "mongodb://localhost:28017", "E2E_MONGOD_BINARY": "/Volumes/Drive/env/bin/mongod"}, config)
            self.assertEqual(settings["mongo_data_root"], "/Volumes/Drive/env")
            self.assertEqual(settings["mongo_image"], "override-image:8")
            self.assertEqual(settings["target_mongo_uri"], "mongodb://localhost:28017")
            self.assertEqual(settings["mongo_binary"], "/Volumes/Drive/env/bin/mongod")
            self.assertEqual(runner.mongo_settings({"E2E_MONGO_DATA_ROOT": ""}, config)["mongo_data_root"], "")

    def test_managed_mongo_rejects_absent_and_unmounted_external_roots(self):
        with tempfile.TemporaryDirectory() as temp:
            absent = Path(temp) / "absent"
            with self.assertRaises(runner.RunnerError):
                runner.mounted_data_root(str(absent))
            self.assertFalse(absent.exists())
        with patch.object(runner.sys, "platform", "darwin"), patch.object(Path, "is_dir", return_value=True), patch.object(Path, "resolve", lambda path: path), patch.object(runner.os.path, "ismount", return_value=False):
            with self.assertRaises(runner.RunnerError):
                runner.mounted_data_root("/Volumes/AbsentDrive/erp-e2e")
        with patch.object(runner.sys, "platform", "darwin"), patch.object(Path, "is_dir", return_value=True), patch.object(Path, "resolve", lambda path: path), patch.object(runner.os.path, "ismount", side_effect=lambda path: path == Path("/Volumes/Drive")):
            self.assertEqual(runner.mounted_data_root("/Volumes/Drive/erp-e2e"), Path("/Volumes/Drive/erp-e2e"))
            with self.assertRaises(runner.RunnerError):
                runner.mounted_data_root("/Users/person/erp-e2e")

    def test_uri_changes_default_db_without_changing_auth_database(self):
        uri = runner.uri_for_database("mongodb://alice:p%40ss@localhost:27017/development?replicaSet=rs0", "erp_e2e_new")
        parsed = urlsplit(uri)
        self.assertEqual(parsed.path, "/erp_e2e_new")
        self.assertEqual(parse_qs(parsed.query), {"replicaSet": ["rs0"], "authSource": ["development"]})
        self.assertEqual(parsed.password, "p%40ss")

    def test_explicit_auth_source_survives_backend_and_archive_connections(self):
        source = "mongodb://alice:p@localhost:27017/development?authSource=admin&replicaSet=rs0"
        for database in ("erp_e2e_new", ""):
            parsed = urlsplit(runner.uri_for_database(source, database))
            self.assertEqual(parse_qs(parsed.query)["authSource"], ["admin"])
            self.assertEqual(parsed.path, f"/{database}")

    def test_remote_write_requires_opt_in_and_honors_exact_host_allowlist(self):
        uri = "mongodb://alice:p@db1.example:27017,db2.example:27017/development"
        for env in ({}, {"E2E_ALLOW_REMOTE_RESET": "1", "ERP_RESET_ALLOWED_REMOTE_HOSTS": "db1.example"}, {"E2E_ALLOW_REMOTE_RESET": "1", "ERP_RESET_ALLOWED_REMOTE_HOSTS": "*.example"}):
            with self.assertRaises(runner.RunnerError):
                runner.require_write_authorization(uri, env)
        self.assertTrue(runner.require_write_authorization(uri, {"E2E_ALLOW_REMOTE_RESET": "1", "ERP_RESET_ALLOWED_REMOTE_HOSTS": "db1.example,db2.example"}))
        env = {"E2E_ALLOW_REMOTE_RESET": "1"}
        self.assertTrue(runner.require_write_authorization(uri, env))
        self.assertEqual(env["ERP_RESET_ALLOWED_REMOTE_HOSTS"], "db1.example,db2.example")
        self.assertFalse(runner.require_write_authorization("mongodb://localhost:27017,[::1]:27017/development", {}))

    def test_cleanup_refuses_source_other_shard_and_other_run(self):
        run_id = "20261001t120000_abcdef012345"
        runner.assert_owned_database(f"erp_e2e_{run_id}_2", run_id, 2)
        for database in ("erp", f"erp_e2e_{run_id}_1", "erp_e2e_20261001t120000_000000000000_2", f"erp_e2e_{run_id}_2_extra"):
            with self.assertRaises(runner.RunnerError):
                runner.assert_owned_database(database, run_id, 2)

    def test_config_is_independent_and_preserves_source_extensions(self):
        source = {
            "app": {"port": 10001, "secret": "source-secret", "extra": True},
            "database": {"uri": "mongodb://localhost/erp", "db_name": "erp", "options": {"timeout": 5}},
            "s3": {"key_prefix": "erp/uploads", "secret_access_key": "s3-secret", "bucket": "erp"},
            "demo": {"master_data": True},
        }
        result = tomllib.loads(runner.shard_config(source, "mongodb://localhost/new", "new", 11002, "run", 2))
        self.assertEqual(source["app"]["port"], 10001)
        self.assertEqual(source["database"]["db_name"], "erp")
        self.assertEqual(result["app"]["port"], 11002)
        self.assertNotEqual(result["app"]["secret"], source["app"]["secret"])
        self.assertEqual(len(result["app"]["secret"]), 64)
        self.assertEqual(result["database"]["options"], source["database"]["options"])
        self.assertEqual(result["demo"], source["demo"])
        self.assertEqual(result["s3"]["key_prefix"], "e2e/run/2")
        self.assertEqual(result["s3"]["secret_access_key"], "s3-secret")

    def test_private_config_and_logs_do_not_publish_credentials(self):
        with tempfile.TemporaryDirectory() as temp:
            path = Path(temp) / "config.toml"
            runner.private_write(path, "secret")
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), 0o600)
        redact = runner.Redactor({"database": {"uri": "mongodb://alice:p%40ss@localhost/erp"}, "app": {"secret": "app-secret"}, "s3": {"secret_access_key": "s3-secret"}})
        value = redact("URI mongodb://alice:p%40ss@localhost/erp alice p@ss app-secret s3-secret")
        for secret in ("alice", "p@ss", "app-secret", "s3-secret", "mongodb://"):
            self.assertNotIn(secret, value)

    def test_result_summary_retains_failed_and_serial_skipped_tests(self):
        report = {"suites": [{"suites": [{"file": "flow.spec.ts", "specs": [
            {"title": "failed", "tests": [{"status": "unexpected", "expectedStatus": "passed", "results": [{"status": "failed", "duration": 200}]}]},
            {"title": "serial skip", "tests": [{"status": "skipped", "results": [{"status": "skipped", "duration": 0}]}]},
        ]}]}]}
        results = runner.result_tests(report, 3)
        self.assertEqual([(result["title"], result["result_status"], result["duration_ms"]) for result in results], [("failed", "failed", 200), ("serial skip", "skipped", 0)])
        self.assertTrue(all(result["shard"] == 3 for result in results))


class ProcessCleanup(unittest.IsolatedAsyncioTestCase):
    async def test_each_spec_resets_business_state_and_failures_do_not_skip_other_files(self):
        files = ["tests/flow-10-stock-adjustment.spec.ts", "tests/flow-16-customer-refund.spec.ts", "tests/s2-org-data-scope-browser.spec.ts"]
        for reset_flag in ("1", "0"):
            with self.subTest(reset=reset_flag), tempfile.TemporaryDirectory() as temp:
                root = Path(temp).resolve()
                with patch.object(runner, "ROOT", root):
                    target = runner.Runner(argparse.Namespace(target="all", workers=1, config="unused"))
                target.env = {"E2E_RESET": reset_flag}
                target.redact = runner.Redactor({})
                target.progress = lambda text: None
                config = root / "private-shard.toml"
                env = {"API_BASE": "http://127.0.0.1:11001", "ERP_E2E_CONFIG_PATH": str(config)}
                balances = {"stock": 0}
                observed = []
                resets = []
                invocations = []

                async def reset(actual_config, db_name, actual_env, directory):
                    self.assertEqual(actual_config, config)
                    self.assertEqual(db_name, "owned_e2e_database")
                    self.assertEqual(actual_env["ERP_E2E_CONFIG_PATH"], str(config))
                    self.assertEqual(actual_env["API_BASE"], env["API_BASE"])
                    resets.append(balances["stock"])
                    balances["stock"] = 0

                async def spawn(args, log, actual_env, *other):
                    self.assertIn("--workers=1", args)
                    matches = [file for file in files if runner.file_filters([file])[0] in args]
                    self.assertEqual(len(matches), 1)
                    file = matches[0]
                    index = files.index(file)
                    invocations.append((file, actual_env["ERP_E2E_RESULT_JSON"]))
                    observed.append(balances["stock"])
                    if index == 0:
                        balances["stock"] = 85
                    status = "failed" if index == 0 else "passed"
                    count = (1, 2, 3)[index]
                    report = {"suites": [{"specs": [{"file": Path(file).name, "title": f"test {number}", "tests": [{"status": "unexpected" if status == "failed" else "expected", "results": [{"status": status, "duration": 100}]}]} for number in range(count)]}]}
                    Path(actual_env["ERP_E2E_RESULT_JSON"]).write_text(json.dumps(report))
                    return Mock(wait=AsyncMock(return_value=1 if index == 0 else 0))

                target.reset = reset
                target.spawn = spawn
                result = await target.run_files(files, 1, config, "owned_e2e_database", env, root)
                self.assertEqual(result["status"], "failed")
                self.assertEqual(result["exit_code"], 1)
                self.assertEqual([item["status"] for item in result["file_runs"]], ["failed", "passed", "passed"])
                self.assertEqual([file for file, _ in invocations], files)
                self.assertEqual(len({path for _, path in invocations}), 3)
                self.assertEqual(len(result["tests"]), 6)
                self.assertEqual({runner.report_file(test["file"]) for test in result["tests"]}, set(files))
                self.assertEqual(observed, [0, 0, 0] if reset_flag == "1" else [0, 85, 85])
                self.assertEqual(len(resets), 2 if reset_flag == "1" else 0)
                self.assertNotIn("coverage_error", result)

    async def test_native_mongo_uses_local_port_and_stops_before_owned_data_cleanup(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            binary = root / "tools/mongod"
            binary.parent.mkdir()
            binary.write_text("not executed")
            binary.chmod(0o700)
            with patch.object(runner, "ROOT", root):
                target = runner.Runner(argparse.Namespace(target="all", workers=1, config="unused"))
            target.progress = lambda text: None
            target.source_uri = "mongodb://source-user:source-pass@localhost:27017/source"
            spawned = []
            commands = []
            stop_observed = []

            async def stop():
                stop_observed.append(target.mongo_data_dir.exists())

            owned_process = Mock(process=Mock(returncode=None), stop=AsyncMock(side_effect=stop))

            async def spawn(args, *other, **kwargs):
                spawned.append(args)
                return owned_process

            async def command(args, *other, **kwargs):
                commands.append(args)

            target.spawn = spawn
            target.command = command
            settings = {"mongo_data_root": str(root), "mongo_binary": str(binary), "mongo_image": "unused-image"}
            with patch.object(runner, "mounted_data_root", return_value=root):
                uri = await target.start_managed_mongo(settings)
                port = urlsplit(uri).port
                self.assertEqual(spawned[0][0], str(binary))
                self.assertEqual(spawned[0][spawned[0].index("--port") + 1], str(port))
                self.assertEqual(spawned[0][spawned[0].index("--bind_ip") + 1], "127.0.0.1")
                self.assertEqual(commands[0][0], "mongosh")
                self.assertIn(f"127.0.0.1:{port}", commands[0][-1])
                self.assertEqual(target.source_uri, "mongodb://source-user:source-pass@localhost:27017/source")
                self.assertEqual(target.summary["managed_mongo"]["engine"], "native")
                self.assertEqual(target.summary["managed_mongo"]["binary"], "mongod")
                self.assertIsNone(target.mongo_container)
                await target.cleanup_managed_mongo()
            self.assertTrue(stop_observed and all(stop_observed))
            self.assertFalse(target.mongo_data_dir.exists())
            self.assertTrue(all(args[0] != "docker" for args in commands + spawned))

    async def test_separate_target_never_changes_source_dump_connection(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp).resolve()
            config = root / "source.toml"
            source = "mongodb://source-user:source-pass@127.0.0.1:27017/source?authSource=source_auth"
            target_uri = "mongodb://target-user:target-pass@127.0.0.1:28017/?authSource=target_auth"
            config.write_text(f"[app]\nport=10001\nsecret='source-jwt'\n[database]\nuri={json.dumps(source)}\ndb_name='source'\n[s3]\nkey_prefix='source/uploads'\n")
            binary = root / "target/debug/web-api"
            binary.parent.mkdir(parents=True)
            binary.write_text("not executed")
            binary.chmod(0o700)
            with patch.object(runner, "ROOT", root):
                target = runner.Runner(argparse.Namespace(target="all", workers=1, config=str(config)))
            target.progress = lambda text: None
            dump_uris = []

            async def command(args, log, **kwargs):
                if args[0] == "mongodump":
                    mongo_config = Path(next(value.split("=", 1)[1] for value in args if value.startswith("--config=")))
                    dump_uris.append(json.loads(mongo_config.read_text())["uri"])

            async def spawn(args, log, capture=None, **kwargs):
                capture.append(json.dumps({"target_directory": str(root / "target")}))
                return Mock(wait=AsyncMock(return_value=0))

            async def shard(index, port):
                self.assertEqual(target.target_uri, target_uri)
                result = {"shard": index, "status": "passed", "cleanup": "dropped"}
                target.summary["shards"].append(result)
                return result

            target.command = command
            target.spawn = spawn
            target.run_shard = shard
            settings = {"mongo_data_root": "", "target_mongo_uri": target_uri, "mongo_image": "existing-image:8"}
            with patch.object(runner, "mongo_settings", return_value=settings):
                self.assertEqual(await target.run(), 0)
            self.assertEqual(len(dump_uris), 1)
            self.assertEqual(parse_qs(urlsplit(dump_uris[0]).query)["authSource"], ["source_auth"])
            self.assertEqual(urlsplit(dump_uris[0]).username, "source-user")
            self.assertNotIn("target-user", dump_uris[0])
            self.assertEqual(target.summary["mongo_mode"], "target_uri")

    async def test_mongo_cleanup_refuses_wrong_container_label(self):
        with tempfile.TemporaryDirectory() as temp:
            target = runner.Runner.__new__(runner.Runner)
            target.run_id = "20261001t120000_abcdef012345"
            target.mongo_root = Path(temp).resolve()
            target.mongo_data_dir = target.mongo_root / target.run_id
            target.mongo_data_dir.mkdir()
            target.mongo_container = f"erp-e2e-mongo-{target.run_id}"
            target.mongo_data_owned = True
            target.mongo_process = None
            target.report_dir = Path(temp)
            target.command = AsyncMock()

            async def inspect(*args, capture, **kwargs):
                capture.append("another-run\n")
                return Mock(wait=AsyncMock(return_value=0))

            target.spawn = inspect
            with self.assertRaises(runner.RunnerError):
                await target.cleanup_managed_mongo()
            target.command.assert_not_called()
            self.assertTrue(target.mongo_data_dir.exists())

    async def test_mongo_cleanup_removes_only_current_labeled_container_and_directory(self):
        with tempfile.TemporaryDirectory() as temp:
            target = runner.Runner.__new__(runner.Runner)
            target.run_id = "20261001t120000_abcdef012345"
            target.mongo_root = Path(temp).resolve()
            target.mongo_data_dir = target.mongo_root / target.run_id
            target.mongo_data_dir.mkdir()
            (target.mongo_data_dir / "mongo.data").write_text("owned data")
            sibling = target.mongo_root / "another-run"
            sibling.mkdir()
            target.mongo_container = f"erp-e2e-mongo-{target.run_id}"
            target.mongo_data_owned = True
            target.mongo_process = None
            target.report_dir = Path(temp)
            target.summary = {"managed_mongo": {"cleanup": "pending"}}
            target.command = AsyncMock()

            async def inspect(*args, capture, **kwargs):
                capture.append(target.run_id + "\n")
                return Mock(wait=AsyncMock(return_value=0))

            target.spawn = inspect
            with patch.object(runner, "mounted_data_root", return_value=target.mongo_root):
                await target.cleanup_managed_mongo()
            command = target.command.call_args.args[0]
            self.assertEqual(command, ["docker", "rm", "--force", "--volumes", target.mongo_container])
            self.assertFalse(target.mongo_data_dir.exists())
            self.assertTrue(sibling.exists())
            self.assertEqual(target.summary["managed_mongo"]["cleanup"], "removed")

    async def test_cancel_stops_owned_process_before_returning(self):
        process = await asyncio.create_subprocess_exec(sys.executable, "-c", "import time; time.sleep(60)", stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.STDOUT, start_new_session=True)
        reader = asyncio.create_task(process.stdout.read())
        managed = runner.Process(process, reader)
        waiter = asyncio.create_task(managed.wait())
        await asyncio.sleep(0.05)
        waiter.cancel()
        with self.assertRaises(asyncio.CancelledError):
            await waiter
        self.assertIsNotNone(process.returncode)
        self.assertTrue(reader.done())
        with self.assertRaises(ProcessLookupError):
            os.killpg(process.pid, signal.SIGTERM)

    async def test_timeout_stops_owned_process(self):
        process = await asyncio.create_subprocess_exec(sys.executable, "-c", "import time; time.sleep(60)", stdout=asyncio.subprocess.PIPE, start_new_session=True)
        managed = runner.Process(process, asyncio.create_task(process.stdout.read()))
        with self.assertRaises(runner.RunnerError):
            await managed.wait(0.05)
        self.assertIsNotNone(process.returncode)

    async def test_timeout_stops_descendant_after_launcher_has_exited(self):
        child = "import time; time.sleep(60)"
        launcher = f"import subprocess,sys; subprocess.Popen([sys.executable, '-c', {child!r}])"
        process = await asyncio.create_subprocess_exec(sys.executable, "-c", launcher, stdout=asyncio.subprocess.PIPE, start_new_session=True)
        reader = asyncio.create_task(process.stdout.read())
        managed = runner.Process(process, reader)
        with self.assertRaises(runner.RunnerError):
            await managed.wait(0.1)
        self.assertIsNotNone(process.returncode)
        self.assertTrue(reader.done())

    async def test_log_failure_does_not_prevent_stopping_owned_process(self):
        process = await asyncio.create_subprocess_exec(sys.executable, "-c", "import time; time.sleep(60)", stdout=asyncio.subprocess.PIPE, start_new_session=True)

        async def broken_log():
            raise OSError("simulated full log filesystem")

        reader = asyncio.create_task(broken_log())
        managed = runner.Process(process, reader)
        await managed.stop()
        self.assertIsNotNone(process.returncode)
        self.assertTrue(reader.done())


if __name__ == "__main__":
    unittest.main()
