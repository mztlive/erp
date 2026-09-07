#!/usr/bin/env python3
"""Pure fixture checks for measurement facts; no Cargo, DB or production probes."""
from __future__ import annotations
import argparse
import ast
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from concurrent.futures import ThreadPoolExecutor
from contextlib import ExitStack
from unittest.mock import patch

sys.dont_write_bytecode = True

def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    return module


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


MEASURE = Path(__file__).with_name("measure-incremental.py")
M = load(MEASURE, "measured_facts_candidate")
REFERENCE = None
EVALUATOR = None
CONTEXT = None


def artifact(kind="lib", fresh=False):
    return {"reason": "compiler-artifact", "package_id": "path+file:///fixture/pkg#erp-customer@0.1.0",
            "manifest_path": "/fixture/pkg/Cargo.toml", "features": ["default", "unicode"],
            "target": {"name": "erp_customer", "kind": [kind], "crate_types": [kind],
                       "required-features": ["unicode"], "src_path": "/fixture/pkg/src/lib.rs",
                       "edition": "2024", "doc": True, "doctest": False, "test": False},
            "profile": {"opt_level": "1", "debuginfo": 1, "debug_assertions": True,
                        "overflow_checks": True, "test": False, "future_field": {"preserve": True}},
            "filenames": ["/fixture/target/liberp_customer.rmeta"], "executable": None, "fresh": fresh}


def jsonl(messages):
    return "\n".join(json.dumps(x) for x in messages)+"\n"


class FactsTests(unittest.TestCase):
    def test_full_artifact_keeps_fields_and_duplicate_package_targets(self):
        a = artifact(); b = artifact("bin", True); c = artifact("custom-build", True)
        rows = [a,b,c,{"reason":"build-finished","success":True}]
        raw = jsonl(rows); full = M.parse_compiler_units_full(raw)
        self.assertEqual(full["artifacts"],[a,b,c]); self.assertEqual(full["build_finished"],[rows[-1]])
        self.assertEqual(full["raw_cargo_jsonl_sha256"],hashlib.sha256(raw.encode()).hexdigest())
        self.assertEqual(len(full["artifacts"]),3)

    def test_full_rejects_invalid_artifact_without_coercion(self):
        cases=[]
        for value in [None,0,1,"false"]:
            row=artifact();row["fresh"]=value;cases.append(row)
        for key in ["fresh","profile","target","package_id"]:
            row=artifact();row.pop(key);cases.append(row)
        row=artifact();row["target"]["kind"]=[];cases.append(row)
        row=artifact();row["profile"]={};cases.append(row)
        for row in cases:
            with self.subTest(row=row),self.assertRaises(M.MeasureError):M.parse_compiler_units_full(jsonl([row]))
        with self.assertRaises(M.MeasureError):M.parse_compiler_units_full("[]\n")

    def test_build_script_message_is_not_invented_compiler_success(self):
        message={"reason":"build-script-executed","package_id":"pkg", "linked_libs":["x"], "out_dir":"/fixture/out"}
        value=M.parse_compiler_units_full(jsonl([message]))
        self.assertEqual(value["artifacts"],[]);self.assertEqual(value["build_finished"],[])
        self.assertEqual(value["build_scripts"],[message]);self.assertNotIn("fresh",value["build_scripts"][0])

    def test_new_environment_projection_is_whitelisted_and_hashed(self):
        secret="SECRET_SENTINEL_DO_NOT_DUMP"
        env={"CARGO_BUILD_JOBS":"12","CARGO_ENCODED_RUSTFLAGS":secret,"RUSTC_WRAPPER":secret,
             "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER":secret,"CARGO_PROFILE_DEV_CODEGEN_UNITS":"8",
             "AWS_SECRET_ACCESS_KEY":secret,"CARGO_REGISTRIES_PRIVATE_TOKEN":secret,"DATABASE_URL":secret}
        value=M.allowed_build_environment(env)
        self.assertEqual(set(value["values"]),set(env)-{"AWS_SECRET_ACCESS_KEY","CARGO_REGISTRIES_PRIVATE_TOKEN","DATABASE_URL"})
        self.assertNotIn(secret,json.dumps(value));self.assertNotIn("AWS_SECRET",json.dumps(value))
        self.assertEqual(env["RUSTC_WRAPPER"],secret)

    def test_df_uses_absolute_bin_and_legacy_environment_fields(self):
        calls=[]
        def fake(argv,**kwargs):
            calls.append(argv);return subprocess.CompletedProcess(argv,0,"observed\n","")
        with tempfile.TemporaryDirectory() as d,patch.object(M,"run_checked",side_effect=fake):
            root=Path(d);value=M.collect_environment(root,root,"c"*40)
        self.assertIn(["/bin/df","-h",str(root)],calls)
        self.assertFalse(any(argv[0]=="df" for argv in calls))
        self.assertEqual(value["target_dir_df"],"observed");self.assertEqual(value["profile"],"dev")
        self.assertEqual(value["features"],"default (no --features)")

    def input_files(self,root):
        root=root.resolve()
        spec=root/"selected-spec.json";spec.write_text('{"fixture":true}\n')
        contract=root/"selected-contract.md";contract.write_text("fixture contract\n")
        repo=root/"historical-repo";repo.mkdir();out=root/"evidence";out.mkdir()
        return repo,out,spec,contract

    def test_real_inputs_and_full_metadata_preserved_without_historical_script(self):
        with tempfile.TemporaryDirectory() as d:
            repo,out,spec,contract=self.input_files(Path(d))
            metadata={"packages":[{"id":"a","name":"pkg","targets":[artifact()["target"]],"metadata":{"custom":7}}],
                      "workspace_members":["a"],"workspace_default_members":["a"],"resolve":{"nodes":[{"id":"a","deps":[],"features":["x"]}]},
                      "target_directory":"/fixture/default-target","workspace_root":str(repo/"backend"),"version":1}
            snapshot=copy.deepcopy(metadata)
            M.write_measurement_facts(out,repo=repo,target_dir=Path(d)/"dedicated",commit="c"*40,spec_path=spec,
                contract_path=contract,contract_explicit=True,metadata=metadata,env={"CARGO_BUILD_JOBS":"12"})
            full=json.loads((out/"cargo-metadata-full.json").read_text());facts=json.loads((out/"measurement-facts.json").read_text())
            self.assertEqual(full,metadata);self.assertEqual(metadata,snapshot)
            for key,path in [("measurement_script",MEASURE),("probe_spec",spec),("measurement_contract",contract)]:
                self.assertEqual(facts["inputs"][key]["sha256"],sha(path));self.assertEqual(facts["inputs"][key]["location"],"external_to_measured_repo")
            for key in ["full_metadata","build_environment"]:
                ref=facts[key];self.assertEqual(ref["sha256"],sha(out/ref["path"]))
            self.assertFalse((repo/"backend/scripts/measure-incremental.py").exists());self.assertIsNone(facts["comparable"])

    def test_missing_explicit_contract_fails_without_fabricated_metadata(self):
        with tempfile.TemporaryDirectory() as d:
            repo,out,spec,contract=self.input_files(Path(d));contract.unlink()
            with self.assertRaises(M.MeasureError):
                M.write_measurement_facts(out,repo=repo,target_dir=repo,commit="c"*40,spec_path=spec,
                    contract_path=contract,contract_explicit=True,metadata={"fixture":True},env={})
            self.assertEqual(list(out.iterdir()),[])

    def test_private_facts_refuse_overwrite(self):
        with tempfile.TemporaryDirectory() as d:
            p=Path(d)/"facts.json";ref=M.write_private_json(p,{"observed":7})
            self.assertEqual(p.stat().st_mode&0o777,0o600);self.assertEqual(ref["sha256"],sha(p))
            before=p.read_bytes()
            with self.assertRaises(FileExistsError):M.write_private_json(p,{"observed":99})
            self.assertEqual(p.read_bytes(),before)

    def test_atomic_lock_has_exactly_one_winner(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);barrier=threading.Barrier(2)
            def contender():
                barrier.wait()
                try:return M.acquire_target_lock(root)
                except M.MeasureError:return None
            with ThreadPoolExecutor(max_workers=2) as pool:results=list(pool.map(lambda _:contender(),range(2)))
            winners=[x for x in results if x is not None];self.assertEqual(len(winners),1)
            self.assertEqual(winners[0].path.stat().st_mode&0o777,0o600)
            M.release_target_lock(winners[0]);self.assertFalse((root/"measure-incremental.lock").exists())

    def test_existing_live_stale_invalid_and_symlink_locks_are_preserved(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);marker=root/"measure-incremental.lock"
            for value in [json.dumps({"pid":os.getpid()}),json.dumps({"pid":999999999}),"", "broken JSON"]:
                marker.write_text(value)
                with self.assertRaises(M.MeasureError):M.acquire_target_lock(root)
                self.assertEqual(marker.read_text(),value);marker.unlink()
            destination=root/"elsewhere";destination.write_text("keep");marker.symlink_to(destination)
            with self.assertRaises(M.MeasureError):M.acquire_target_lock(root)
            self.assertTrue(marker.is_symlink());self.assertEqual(destination.read_text(),"keep")

    def test_release_does_not_remove_a_replacement_marker(self):
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);lock=M.acquire_target_lock(root);lock.path.unlink();lock.path.write_text("replacement")
            M.release_target_lock(lock);self.assertEqual(lock.path.read_text(),"replacement")
            with self.assertRaises(OSError):os.fstat(lock.fd)

    def test_lock_write_failure_cleans_only_own_marker(self):
        with tempfile.TemporaryDirectory() as d,patch.object(M.json,"dumps",side_effect=RuntimeError("fixture write failure")):
            with self.assertRaises(RuntimeError):M.acquire_target_lock(Path(d))
            self.assertFalse((Path(d)/"measure-incremental.lock").exists())

    def test_run_cargo_retains_timing_and_writes_both_exact_views(self):
        rows=[artifact(),{"reason":"build-finished","success":True}];raw=jsonl(rows)
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);out=root/"out";timings=root/"timings-source.html";timings.write_text("fixture")
            fake=subprocess.CompletedProcess([],0,raw,"fixture stderr")
            with patch.object(M.subprocess,"run",return_value=fake) as child,patch.object(M.time,"perf_counter",side_effect=[10.0,12.5]),patch.object(M,"copy_timings",return_value=timings):
                result=M.run_cargo(backend=root,mode="build",target_dir=root/"target",output_dir=out,env={"CARGO_BUILD_JOBS":"12","ERP_TEST_MONGO_URI":"not-forwarded"},time_it=True)
            self.assertEqual(result["seconds"],2.5);self.assertEqual(child.call_args.args[0],M.cargo_command("build"))
            self.assertEqual(child.call_args.kwargs["env"],M.cargo_env({"CARGO_BUILD_JOBS":"12","ERP_TEST_MONGO_URI":"not-forwarded"},root/"target"))
            self.assertEqual(json.loads((out/"units.json").read_text()),M.parse_compiler_units(raw))
            self.assertEqual(json.loads((out/"units-full.json").read_text())["artifacts"],rows[:1])
            self.assertEqual((out/"cargo.jsonl").read_text(),raw)

    def fixture_measure(self,root,fail_call=None):
        repo,out,spec,contract=self.input_files(root);backend=repo/"backend";backend.mkdir();(backend/"Cargo.toml").write_text("[workspace]\n")
        source=backend/"probe.rs";original=b"fn probe() { before_call(); }\n";source.write_bytes(original)
        probe=M.Probe("customer",source,"probe.rs","erp-customer","probe","before_call()","after_call()","fixture")
        metadata={"workspace_root":str(backend),"target_directory":str(root/"default"),"workspace_members":["pid"],"packages":[{"id":"pid","name":"erp-customer"}],"resolve":{"nodes":[]},"version":1}
        args=argparse.Namespace(repo=str(repo),spec=str(spec),measurement_contract=str(contract),target_dir=str(root/"dedicated"),output=str(out),scenario="customer",revision="candidate",mode="check",samples=5)
        events=[]
        def fake_cargo(**kwargs):
            relative=kwargs["output_dir"].relative_to(out).as_posix();after=b"after_call()" in source.read_bytes()
            expected_after=relative=="warmup/after" or (relative.startswith("sample-") and "/" not in relative)
            self.assertEqual(after,expected_after);events.append((relative,kwargs["time_it"],after))
            if len(events)==fail_call:raise M.MeasureError("fixture simulated Cargo failure; no Cargo executed")
            item={"package_name":"erp-customer","fresh":relative=="noop","target_kind":["lib"]}
            return {"units":{"dirty":[] if relative=="noop" else [item]},"seconds":float(len(events))}
        with ExitStack() as stack:
            for name,value in [("assert_isolated_worktree","c"*40),("assert_source_clean",None),("cargo_metadata",metadata),("load_probe",probe),("collect_environment",{"fixture_only":True})]:stack.enter_context(patch.object(M,name,return_value=value))
            stack.enter_context(patch.object(M,"run_cargo",side_effect=fake_cargo));stack.enter_context(patch.object(M.subprocess,"run",side_effect=AssertionError("external subprocess forbidden in pure fixture")))
            # Exercise real source guard restoration while avoiding global signal replacement in tests.
            stack.enter_context(patch.object(M.RestoreSession,"install",lambda session,guard:setattr(session,"guard",guard)))
            if fail_call:
                with self.assertRaises(M.MeasureError):M.measure(args)
                summary=json.loads((out/"summary.json").read_text())
            else:summary=M.measure(args)
        self.assertEqual(source.read_bytes(),original);self.assertFalse((root/"dedicated/measure-incremental.lock").exists())
        return events,summary,out

    def test_five_samples_warmups_noop_restores_remain_fourteen_calls(self):
        with tempfile.TemporaryDirectory() as d:
            events,summary,out=self.fixture_measure(Path(d))
            expected=[("warmup",False,False),("noop",False,False),("warmup/after",False,True),("warmup/restore",False,False)]
            for n in range(1,6):expected.extend([(f"sample-{n:02d}",True,True),(f"sample-{n:02d}/restore-before",False,False)])
            self.assertEqual(events,expected);self.assertEqual(summary["valid_samples"],5);self.assertTrue(summary["source_restored"])
            self.assertEqual(summary["seconds"],[5.0,7.0,9.0,11.0,13.0]);self.assertEqual(summary["median_seconds"],9.0)
            meta=json.loads((out/"metadata.json").read_text());self.assertEqual(set(meta),{"workspace_root","target_directory_default","target_directory_measured","commit","packages"})

    def test_simulated_failure_retains_original_source_restore_and_lock_release(self):
        with tempfile.TemporaryDirectory() as d:
            events,summary,_=self.fixture_measure(Path(d),fail_call=7)
            self.assertEqual(len(events),7);self.assertTrue(summary["source_restored"]);self.assertFalse(summary["measurement_passed"])

    def test_reference_source_algorithm_and_legacy_parser_unchanged(self):
        if REFERENCE is None:self.skipTest("--reference-script required for source comparison")
        before=REFERENCE.read_text();after=MEASURE.read_text()
        def nodes(text):return {x.name:ast.dump(x,include_attributes=False) for x in ast.parse(text).body if isinstance(x,(ast.FunctionDef,ast.ClassDef))}
        old,new=nodes(before),nodes(after)
        for name in ["cargo_command","cargo_env","cargo_metadata","load_probe","parse_compiler_units","SourceGuard","RestoreSession","prove_backup_restore","median","business_dirty","package_dirty","reverse_dependencies","assert_isolated_worktree","assert_source_clean","write_recovery"]:self.assertEqual(old[name],new[name],name)
        begin='        warmup = run_cargo(';end='    finally:\n        try:\n            session.restore_quiet()'
        self.assertEqual(before[before.index(begin):before.index(end)],after[after.index(begin):after.index(end)])
        begin='    started = time.perf_counter()';end='    (output_dir / "cargo.jsonl")'
        self.assertEqual(before[before.index(begin):before.index(end)],after[after.index(begin):after.index(end)])
        old_collect=before[before.index('def collect_environment('):before.index('def cargo_command(')]
        new_collect=after[after.index('def collect_environment('):after.index('def cargo_command(')]
        self.assertEqual(old_collect.replace('["df", "-h"','["/bin/df", "-h"'),new_collect)

    def test_existing_evaluator_accepts_unchanged_units_shape_and_rejects_forgery(self):
        if EVALUATOR is None:self.skipTest("--evaluator required for integration check")
        raw=jsonl([artifact(),artifact("bin",True),{"reason":"build-finished","success":True}])
        with tempfile.TemporaryDirectory() as d:
            root=Path(d);(root/"cargo.jsonl").write_text(raw);M.write_json(root/"units.json",M.parse_compiler_units(raw))
            M.write_private_json(root/"units-full.json",M.parse_compiler_units_full(raw))
            audit=EVALUATOR.Audit();audit.units(root,"fixture");self.assertTrue(audit.passed_since(0),audit.checks)
            bad=M.parse_compiler_units(raw);bad["artifacts"].pop();M.write_json(root/"units.json",bad)
            audit=EVALUATOR.Audit();audit.units(root,"fixture");self.assertFalse(audit.passed_since(0));self.assertTrue(any(x["check"].endswith("units_match_raw") and not x["passed"] for x in audit.checks))

    def test_context_v2_agrees_on_explicit_external_input_hashes(self):
        if CONTEXT is None:self.skipTest("--context-collector required for integration check")
        with tempfile.TemporaryDirectory() as d:
            repo,out,spec,contract=self.input_files(Path(d))
            chosen=CONTEXT.select_measurement_inputs(repo,measurement_script=MEASURE,measurement_spec=spec,measurement_contract=contract)
            for label,value in chosen.items():
                observed=CONTEXT.observe_measurement_input(value)
                direct=M.measurement_input(Path(value["path"]),repo,label)
                self.assertEqual(observed["sha256"],direct["sha256"]);self.assertEqual(observed["location"],direct["location"])
            env={"CARGO_ENCODED_RUSTFLAGS":"test-flags\x1fnot-a-secret"}
            self.assertEqual(M.allowed_build_environment(env)["values"]["CARGO_ENCODED_RUSTFLAGS"],CONTEXT.safe_value(env["CARGO_ENCODED_RUSTFLAGS"]))

    def test_cli_retains_five_sample_gate_and_accepts_contract_path(self):
        parser=M.build_parser();args=parser.parse_args(["--measurement-contract","/fixture/contract.md"])
        self.assertEqual(args.samples,5);self.assertEqual(args.measurement_contract,"/fixture/contract.md")
        with patch.object(M,"measure",side_effect=AssertionError("measure must not execute")):
            self.assertEqual(M.main(["--samples","4","--self-test-only"]),2)


def main():
    global REFERENCE,EVALUATOR,CONTEXT
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("--reference-script",type=Path);p.add_argument("--evaluator",type=Path);p.add_argument("--context-collector",type=Path);args=p.parse_args()
    REFERENCE=args.reference_script
    if args.evaluator:EVALUATOR=load(args.evaluator,"measurement_facts_evaluator")
    if args.context_collector:CONTEXT=load(args.context_collector,"measurement_facts_context")
    result=unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(FactsTests))
    return 0 if result.wasSuccessful() else 1

if __name__=="__main__":raise SystemExit(main())
