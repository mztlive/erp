"""Pure regression fixtures for the cutover checkers; never run Cargo or MongoDB."""
from __future__ import annotations

import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import cutover_workspace as workspace
import domain_boundaries as domain
import bpm_workspace as bpm

SCRIPTS = Path(__file__).resolve().parent


class TemporaryWorkspace:
    def __init__(self, root: Path, names=None):
        self.root = root
        self.root.mkdir(parents=True, exist_ok=True)
        self.names = sorted(names or domain.CUTOVER_REQUIRED_PACKAGES)
        self.metadata = {"workspace_root": str(root), "workspace_members": [], "packages": []}
        members = []
        for name in self.names:
            relative = ("apps/" if name in {"cli", "web-api"} else "crates/") + name
            directory = root / relative
            (directory / "src").mkdir(parents=True)
            manifest = directory / "Cargo.toml"
            manifest.write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\nautotests = false\n')
            source = directory / "src/lib.rs"
            source.write_text("pub struct Fixture;\n")
            identity = f"path+file://{directory}#{name}@0.1.0"
            self.metadata["workspace_members"].append(identity)
            self.metadata["packages"].append({
                "id": identity, "name": name, "manifest_path": str(manifest),
                "dependencies": [], "targets": [{"name": name, "kind": ["lib"], "src_path": str(source)}],
            })
            members.append(relative)
        (root / "Cargo.toml").write_text('[workspace]\nmembers = ' + json.dumps(members) + '\n')
        self.package("erp-workflow")["dependencies"].append({"name": "bpm", "kind": None, "rename": None})
        plan = root / "docs/superpowers/plans/domain-crate-migration"
        plan.mkdir(parents=True)
        (plan / "plan-manifest.json").write_text('{"phases": []}')

    def package(self, name):
        return next(p for p in self.metadata["packages"] if p["name"] == name)

    def source(self, name):
        return Path(self.package(name)["targets"][0]["src_path"])


class CutoverTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cutover-tool-fixture-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name) / "backend"
        self.fixture = TemporaryWorkspace(self.root)

    def test_final_workspace_without_legacy_passes_actual_checker(self):
        result = domain.check_workspace(self.root, self.fixture.metadata, cutover=True)
        self.assertEqual(result.errors, [])
        self.assertEqual(bpm.dependency_errors(self.fixture.metadata), [])

    def test_normal_entry_automatically_enforces_final_rules_when_old_members_are_absent(self):
        self.assertEqual(domain.check_workspace(self.root, self.fixture.metadata).errors, [])
        self.fixture.package("erp-read-models")["dependencies"].append({"name": "services", "kind": "dev"})
        self.assertTrue(any("最终旧依赖" in error for error in domain.check_workspace(self.root, self.fixture.metadata).errors))

    def test_workspace_missing_file_table_members_or_package_records_fails(self):
        manifest = self.root / "Cargo.toml"
        original = manifest.read_text()
        manifest.unlink()
        self.assertTrue(workspace.workspace_errors(self.root, self.fixture.metadata, set()))
        manifest.write_text('[package]\nname = "not-a-workspace"\n')
        self.assertTrue(workspace.workspace_errors(self.root, self.fixture.metadata, set()))
        manifest.write_text(original)
        for key, value in (("workspace_members", []), ("packages", []), ("workspace_root", "/wrong")):
            with self.subTest(key=key):
                metadata = copy.deepcopy(self.fixture.metadata)
                metadata[key] = value
                self.assertTrue(workspace.workspace_errors(self.root, metadata, set()))

    def test_missing_declared_and_required_member_fails(self):
        metadata = copy.deepcopy(self.fixture.metadata)
        removed = metadata["packages"].pop()
        metadata["workspace_members"].remove(removed["id"])
        self.assertTrue(workspace.workspace_errors(self.root, metadata, set()))
        metadata["workspace_members"].append("unknown-id")
        self.assertTrue(workspace.workspace_errors(self.root, metadata, set()))
        path = Path(self.fixture.package("cli")["manifest_path"])
        path.unlink()
        self.assertTrue(workspace.workspace_errors(self.root, self.fixture.metadata, set()))
        metadata = copy.deepcopy(self.fixture.metadata)
        metadata["workspace_members"] = [i for i in metadata["workspace_members"] if i != self.fixture.package("erp-finance")["id"]]
        self.assertTrue(workspace.workspace_errors(self.root, metadata, domain.CUTOVER_REQUIRED_PACKAGES))

    def test_missing_activity_target_fails(self):
        metadata = copy.deepcopy(self.fixture.metadata)
        metadata["packages"][0]["targets"] = []
        self.assertTrue(workspace.workspace_errors(self.root, metadata, set()))
        metadata = copy.deepcopy(self.fixture.metadata)
        metadata["packages"][0]["targets"][0]["src_path"] = str(self.root / "absent.rs")
        self.assertTrue(workspace.workspace_errors(self.root, metadata, set()))

    def test_legacy_edges_in_every_entry_composition_and_fixture_kind_fail(self):
        for name in ("cli", "web-api", "erp-processes", "erp-read-models", "test-support"):
            for kind in (None, "build", "dev"):
                for old in sorted(workspace.LEGACY):
                    with self.subTest(package=name, kind=kind, old=old):
                        metadata = copy.deepcopy(self.fixture.metadata)
                        package = next(p for p in metadata["packages"] if p["name"] == name)
                        package["dependencies"].append({"name": old, "rename": "innocent_alias", "kind": kind, "target": 'cfg(unix)', "optional": True})
                        errors = domain.check_workspace(self.root, metadata, cutover=True).errors
                        self.assertTrue(any("最终旧依赖" in e for e in errors), errors)

    def test_legacy_package_and_transitive_nonmember_edge_fail(self):
        metadata = copy.deepcopy(self.fixture.metadata)
        metadata["packages"].append({"id": "external", "name": "external", "dependencies": [{"name": "services", "kind": "dev"}]})
        self.assertTrue(workspace.legacy_graph_errors(metadata))
        metadata["packages"][-1] = {"id": "legacy", "name": "entities", "dependencies": []}
        self.assertTrue(workspace.legacy_graph_errors(metadata))

    def test_old_src_and_manifest_fail_but_historical_archive_is_untouched(self):
        archive = self.root / "services/tests/history.rs"
        archive.parent.mkdir(parents=True)
        original = b"use services::HistoricalType;\n"
        archive.write_bytes(original)
        self.assertEqual(workspace.legacy_tree_errors(self.root), [])
        self.assertNotIn(archive, workspace.active_source_paths(self.fixture.metadata))
        for relative in ("services/src", "database/Cargo.toml"):
            path = self.root / relative
            if path.suffix:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("[package]\n")
            else:
                path.mkdir(parents=True)
            self.assertTrue(workspace.legacy_tree_errors(self.root))
        self.assertEqual(archive.read_bytes(), original)

    def test_legacy_source_all_members_inline_tests_and_explicit_target(self):
        cases = ["use services::{Error as Failure, Result};", "use ::services::Error;", "pub type Failure = services::Error;",
                 "fn x() { database::ensure_indexes(); }", "extern crate entities as archived;",
                 '#[path = "../../services/src/a.rs"] mod a;',
                 '#[cfg_attr(feature = "legacy", path = "../../services/src/a.rs")] mod a;',
                 'const X: &str = include_str!("../../database/src/lib.rs");',
                 'const X: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../entities/src/lib.rs"));']
        for source in cases:
            with self.subTest(source=source):
                self.assertTrue(workspace.legacy_source_errors(Path("source.rs"), source))
        for name in ("web-api", "cli", "erp-processes", "erp-read-models", "test-support"):
            with self.subTest(name=name):
                path = self.fixture.source(name)
                path.write_text("pub type Failure = services::Error;\n")
                errors = domain.check_workspace(self.root, self.fixture.metadata, cutover=True).errors
                self.assertTrue(any("最终旧 crate 源引用" in e for e in errors))
                path.write_text("pub struct Fixture;\n")
        inline = self.fixture.source("test-support").parent / "tests/a.rs"
        inline.parent.mkdir(); inline.write_text("use services::Error;\n")
        self.assertIn(inline.resolve(), workspace.active_source_paths(self.fixture.metadata))
        explicit = self.root / "outside-target/main.rs"
        explicit.parent.mkdir(); explicit.write_text("use database::DatabaseExt;\n")
        self.fixture.package("cli")["targets"].append({"name": "custom", "kind": ["bin"], "src_path": str(explicit)})
        self.assertIn(explicit.resolve(), workspace.active_source_paths(self.fixture.metadata))
        self.assertTrue(domain.check_workspace(self.root, self.fixture.metadata, cutover=True).errors)

    def test_comments_wire_strings_and_nested_comments_are_not_legacy_code(self):
        source = '// services::Old\nconst WIRE: &str = "entities::Type";\n/* nested /* database::Old */ services::Old */\nconst RAW: &str = r#"services::Error"#;'
        self.assertEqual(workspace.legacy_source_errors(Path("source.rs"), source), [])
        self.assertEqual(workspace.legacy_source_errors(Path("source.rs"), "use crate::services::LocalModule;"), [])

    def test_each_missing_service_rule_fails_and_existing_rules_remain(self):
        cases = json.loads((SCRIPTS / "domain-boundary-fixtures/service_seven_rules.json").read_text())
        self.assertEqual(len(cases), 7)
        for name, source in cases.items():
            with self.subTest(rule=name):
                errors = domain.scan_service_mongo("service/a.rs", source)
                self.assertTrue(any(name in e for e in errors), errors)
        self.assertEqual(domain.scan_service_mongo("service/a.rs", (SCRIPTS / "domain-boundary-fixtures/service_positive.rs").read_text()), [])
        self.assertEqual(len(domain.SERVICE_MONGO_RULES), 17)

    def test_bpm_direct_and_transitive_all_kind_rules_without_old_manifests(self):
        for kind in (None, "build", "dev"):
            for parent, target in (("bpm", "mongodb"), ("bpm", "erp-sales"), ("cli", "web-api"), ("cli", "bpm"), ("web-api", "bpm")):
                with self.subTest(kind=kind, parent=parent, target=target):
                    metadata = copy.deepcopy(self.fixture.metadata)
                    package = next(p for p in metadata["packages"] if p["name"] == parent)
                    package["dependencies"].append({"name": target, "kind": kind, "rename": "hidden"})
                    self.assertTrue(bpm.dependency_errors(metadata))
        metadata = copy.deepcopy(self.fixture.metadata)
        next(p for p in metadata["packages"] if p["name"] == "bpm")["dependencies"] = [{"name": "bridge", "kind": "build"}]
        metadata["packages"].append({"name": "bridge", "dependencies": [{"name": "mongodb", "kind": "dev"}]})
        self.assertTrue(bpm.dependency_errors(metadata))
        self.fixture.package("erp-workflow")["dependencies"] = []
        self.assertTrue(bpm.dependency_errors(self.fixture.metadata))

    def test_composition_cycle_fails_final_checker(self):
        self.fixture.package("erp-processes")["dependencies"].append({"name": "erp-read-models"})
        self.fixture.package("erp-read-models")["dependencies"].append({"name": "erp-processes"})
        self.assertTrue(any("回边" in e for e in domain.check_workspace(self.root, self.fixture.metadata, cutover=True).errors))

    def test_negative_workspace_check_has_real_nonzero_process_exit(self):
        metadata = Path(self.temporary.name) / "metadata.json"
        metadata.write_text(json.dumps(self.fixture.metadata))
        (self.root / "Cargo.toml").unlink()
        code = 'import json,sys; from pathlib import Path; from cutover_workspace import workspace_errors; errors=workspace_errors(Path(sys.argv[1]),json.loads(Path(sys.argv[2]).read_text()),set()); print(errors); raise SystemExit(1 if errors else 0)'
        completed = subprocess.run([sys.executable, "-c", code, str(self.root), str(metadata)], cwd=SCRIPTS, capture_output=True, text=True)
        self.assertEqual(completed.returncode, 1, completed.stdout + completed.stderr)
        self.assertIn("缺失 workspace", completed.stdout)


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="cutover-capture-fixture-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        for side in ("before", "after"):
            fixture = TemporaryWorkspace(self.root / side / "backend")
            fixture.source("erp-sales").write_text('#[derive(Serialize, Deserialize)] pub struct Request { pub value: String }\n')
        self.command = [sys.executable, str(SCRIPTS / "capture-cutover-contracts.py"), "--before-repo", str(self.root / "before"), "--after-repo", str(self.root / "after"), "--phase", "17"]

    def invoke(self, output):
        return subprocess.run([*self.command, "--output", str(output)], capture_output=True, text=True)

    def test_real_compare_preserves_review_exit_and_unverified_runtime(self):
        output = self.root / "new-evidence"
        result = self.invoke(output)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        raw = json.loads((output / "missing-drift-report.json").read_text())
        self.assertEqual(raw["status"], "needs_review")
        self.assertTrue(raw["needs_review"])
        transaction = json.loads((output / "transaction-contract.json").read_text())
        self.assertFalse(transaction["real_database_verified"])
        self.assertTrue(all(value == "not_verified" for value in transaction["runtime_verification"].values()))
        original = (output / "transaction-contract.json").read_bytes()
        self.assertEqual(self.invoke(output).returncode, 2)
        self.assertEqual((output / "transaction-contract.json").read_bytes(), original)

    def test_real_comparator_drift_exit_is_not_swallowed(self):
        source = self.root / "after/backend/crates/erp-sales/src/lib.rs"
        source.write_text('#[derive(Serialize, Deserialize)] pub struct Request { pub value: bool }\n')
        result = self.invoke(self.root / "drift-evidence")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)

    def test_read_only_before_and_existing_empty_output_are_rejected(self):
        protected = self.root / "before/evidence"
        self.assertEqual(self.invoke(protected).returncode, 2)
        self.assertFalse(protected.exists())
        output = self.root / "existing"
        output.mkdir()
        self.assertEqual(self.invoke(output).returncode, 2)
        self.assertEqual(list(output.iterdir()), [])


def run_pure_self_tests(stream=None) -> bool:
    suite = unittest.TestSuite()
    suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(CutoverTests))
    suite.addTests(unittest.defaultTestLoader.loadTestsFromTestCase(CaptureTests))
    result = unittest.TextTestRunner(stream=stream or sys.stderr, verbosity=2).run(suite)
    return result.wasSuccessful()


if __name__ == "__main__":
    raise SystemExit(0 if run_pure_self_tests() else 1)
