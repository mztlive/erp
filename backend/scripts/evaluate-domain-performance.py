#!/usr/bin/env python3
"""Validate stage-17 recorded evidence; never run Cargo or alter original inputs.

Run --manifest PATH for JSON results; exit zero only when every gate passes.
Run --self-test for temporary synthetic fixtures or --schema for input contracts.
Supply twelve explicit directories and six commit-bound comparison records.
Keep all five original sample durations. Do not trim slow/failed samples.
Recorded attestations support historical hardware/load/configuration claims;
this evaluator does not independently observe historical processes or hardware.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import re
import sys
import tempfile
from decimal import Decimal
from fractions import Fraction
from pathlib import Path
from statistics import median

SCENARIOS = ("customer", "sales", "finance")
MODES = ("check", "build")
REVISIONS = ("baseline", "candidate")
DOMAINS = set("identity audit workflow support party customer supplier catalog warehouse contract sales procurement fulfillment inventory finance returns import integration supply".split())
DOMAINS = {"erp-" + name for name in DOMAINS}
BUSINESS = DOMAINS | {"entities", "database", "services", "erp-core", "application-core", "persistence-core", "erp-processes", "erp-read-models", "erp-commerce"}
ENV_FIELDS = ("rustc_verbose", "cargo_verbose", "toolchain_channel", "profile", "codegen_backend", "features", "RUSTFLAGS", "CARGO_INCREMENTAL", "CARGO_TERM_COLOR", "cpu_count")
PROOF_FIELDS = ("host_id", "hardware_id", "physical_device_id", "build_jobs", "measurement_script_sha256", "build_configuration_sha256", "third_party_versions_sha256")


def encode(value):
    """Serialize exact Decimal outputs as strings without rounding gate operands."""
    return json.dumps(value, ensure_ascii=False, indent=2, default=str)


def write(path, value):
    """Write only self-test fixtures."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(encode(value) + "\n")


def resolve(base, value):
    path = Path(value)
    return (path if path.is_absolute() else base / path).resolve()


def number(value):
    """Accept positive finite raw numeric values; reject bool and string."""
    if isinstance(value, bool) or not isinstance(value, (int, float, Decimal)):
        return None
    result = Decimal(str(value))
    return result if result.is_finite() and result > 0 else None


def package_name(value):
    """Match measure-incremental.py package-id normalization."""
    if value.startswith("path+file://"):
        return Path(value.split("#", 1)[0][len("path+file://"):]).name
    if "#" in value:
        tail = value.rsplit("#", 1)[-1]
        return tail.split("@", 1)[0] if "@" in tail else Path(value.split("#", 1)[0]).name
    return value.split(" ", 1)[0]


def toolchain(value):
    """Normalize only rustup's worktree-specific override annotation."""
    return re.sub(r" \(overridden by '[^']+'\)$", "", value) if isinstance(value, str) else None


def filesystem(value):
    """Compare captured df device and mount, excluding changing free space."""
    if not isinstance(value, str):
        return None
    lines = value.strip().splitlines()
    if len(lines) != 2 or "Mounted on" not in lines[0]:
        return None
    columns = lines[1].split()
    start = 8 if "%iused" in lines[0] else 5
    return (columns[0], " ".join(columns[start:])) if len(columns) > start else None


class Audit:
    def __init__(self):
        self.checks = []
        self.evidence = {}
        self.environment_fingerprints = []

    def check(self, code, valid, reason, *refs, **details):
        row = {"check": code, "passed": bool(valid), "evidence": list(map(str, refs))}
        if not valid:
            row["fail_reason"] = reason
        if details:
            row["details"] = details
        self.checks.append(row)
        return bool(valid)

    def passed_since(self, index):
        return all(row["passed"] for row in self.checks[index:])

    def blob(self, path, code, nonempty=True):
        try:
            raw = path.read_bytes()
        except OSError as error:
            self.check(code, False, "Required evidence cannot be read: " + str(error), path)
            return None
        self.evidence[str(path)] = {"sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}
        if nonempty and not raw:
            self.check(code, False, "Required evidence is empty", path)
            return None
        return raw

    def doc(self, path, code):
        raw = self.blob(path, code)
        if raw is None:
            return {}
        try:
            obj = json.loads(raw, parse_float=Decimal)
            if not isinstance(obj, dict):
                raise ValueError("expected JSON object")
            return obj
        except (ValueError, UnicodeError) as error:
            self.check(code, False, "Invalid JSON: " + str(error), path)
            return {}

    def refs(self, refs, origin, code):
        self.check(code, isinstance(refs, list) and bool(refs), "Require nonempty source evidence references", origin)
        for ref in refs if isinstance(refs, list) else []:
            if not isinstance(ref, dict) or not isinstance(ref.get("path"), str):
                self.check(code, False, "Each source reference needs path and SHA-256", origin)
                continue
            path = resolve(origin.parent, ref["path"])
            raw = self.blob(path, code)
            self.check(code, raw is not None and hashlib.sha256(raw).hexdigest() == ref.get("sha256"),
                       "Referenced evidence hash is absent or mismatched", origin, path)

    def units(self, directory, label):
        raw = self.blob(directory / "cargo.jsonl", label + ".cargo")
        stored = self.doc(directory / "units.json", label + ".units")
        artifacts, scripts, other, finished = [], [], [], []
        invalid_artifacts = []
        for line_number, line in enumerate((raw or b"").decode("utf-8", errors="replace").splitlines(), 1):
            if not line.strip():
                continue
            try:
                message = json.loads(line)
            except ValueError:
                other.append(line.strip())
                continue
            if not isinstance(message, dict):
                self.check(label + ".cargo_message", False, "Cargo message must be an object", directory / "cargo.jsonl", line=line_number)
                continue
            reason = message.get("reason")
            if reason == "compiler-artifact":
                target, profile = message.get("target") or {}, message.get("profile") or {}
                valid = (type(message.get("fresh")) is bool and isinstance(message.get("package_id"), str)
                         and bool(message["package_id"]) and isinstance(target, dict) and isinstance(profile, dict)
                         and isinstance(target.get("name"), str) and bool(target["name"])
                         and isinstance(target.get("kind"), list) and bool(target["kind"])
                         and all(isinstance(kind, str) and bool(kind) for kind in target["kind"])
                         and ("name" in profile or "debuginfo" in profile))
                if not valid:
                    invalid_artifacts.append(line_number)
                    continue
                artifacts.append({"package_id": message["package_id"], "package_name": package_name(message["package_id"]),
                                  "target_name": target.get("name"), "target_kind": list(target.get("kind") or []),
                                  "profile": profile.get("name") or profile.get("debuginfo"), "fresh": message["fresh"]})
            elif reason == "build-script-executed":
                scripts.append({"package_id": message.get("package_id"), "package_name": package_name(str(message.get("package_id", ""))), "out_dir": message.get("out_dir")})
            elif reason == "build-finished":
                finished.append(message.get("success"))
        self.check(label + ".artifact_shape", not invalid_artifacts, "Artifact requires package id, target, profile and boolean fresh", directory / "cargo.jsonl", invalid_lines=invalid_artifacts)
        self.check(label + ".build_finished", bool(finished) and all(x is True for x in finished),
                   "Raw Cargo log must contain successful build-finished evidence", directory / "cargo.jsonl")
        expected = {"artifacts": artifacts, "build_scripts": scripts, "dirty": [a for a in artifacts if not a["fresh"]],
                    "fresh": [a for a in artifacts if a["fresh"]], "non_json_lines": other[:50]}
        self.check(label + ".units_match_raw", bool(artifacts) and stored == expected,
                   "units.json must match nonempty raw artifacts without omitted dirty targets", directory / "units.json", directory / "cargo.jsonl")
        return artifacts

    def run(self, directory, mode, timed, selected, candidate, label, noop=False):
        start = len(self.checks)
        sample = self.doc(directory / "sample.json", label + ".sample")
        self.blob(directory / "cargo.stderr.log", label + ".stderr", nonempty=False)
        self.blob(directory / "timings.html", label + ".timings")
        command = ["cargo", mode, "-p", "web-api"] + (["--bin", "web-api"] if mode == "build" else []) + ["--locked", "--message-format=json", "--timings"]
        self.check(label + ".command", sample.get("command") == command, "Command differs from the contracted mode/flags/features", directory / "sample.json")
        self.check(label + ".exit", type(sample.get("exit_code")) is int and sample["exit_code"] == 0, "Every sample/warmup/noop/restore must exit zero", directory / "sample.json")
        self.check(label + ".timed", sample.get("timed") is timed, "Only retained edit samples must be timed", directory / "sample.json")
        seconds, wall = number(sample.get("seconds")), number(sample.get("wall_seconds_including_untimed"))
        self.check(label + ".seconds", wall is not None and (seconds == wall if timed else sample.get("seconds") is None),
                   "Require finite positive original wall time and exact timed/untimed consistency", directory / "sample.json")
        self.check(label + ".timings_source", isinstance(sample.get("timings_source"), str) and bool(sample["timings_source"]),
                   "Original timings source reference is missing", directory / "sample.json")
        artifacts = self.units(directory, label)
        dirty = {a["package_name"] for a in artifacts if not a["fresh"]}
        for field, fresh in [("dirty_package_names", False), ("fresh_package_names", True)]:
            expected = sorted({a["package_name"] for a in artifacts if a["fresh"] is fresh})
            self.check(label + "." + field, sample.get(field) == expected, "Sample package list differs from raw Cargo artifacts", directory / "sample.json")
        self.check(label + ".business_dirty", sample.get("business_dirty_package_names") == sorted(dirty & BUSINESS),
                   "Business dirty list differs from raw artifacts", directory / "sample.json")
        if timed:
            self.check(label + ".selected_dirty", selected in dirty, "Selected business crate is not dirty; sample is not a real edit measurement", directory / "cargo.jsonl")
        if noop:
            unexpected = [a for a in artifacts if not a["fresh"] and (a["package_name"] in BUSINESS or "custom-build" in a["target_kind"])]
            self.check(label + ".noop_fresh", not unexpected and any(a["package_name"] == selected for a in artifacts),
                       "No-op omits the selected crate or recompiles business/custom-build units", directory / "cargo.jsonl", unexpected=unexpected)
        if candidate and timed:
            # Ordinary final domains have no inter-domain edges. Absence is not Fresh.
            for package in sorted(DOMAINS - {selected}):
                rows = [a for a in artifacts if a["package_name"] == package]
                self.check(label + ".unrelated_fresh." + package, bool(rows) and all(a["fresh"] is True for a in rows),
                           "Unrelated domain is absent or Dirty; every target must be present and Fresh", directory / "cargo.jsonl", artifact_count=len(rows))
        return {"seconds": seconds, "passed": self.passed_since(start), "evidence": str(directory), "dirty_packages": sorted(dirty)}

    def group(self, directory, scenario, mode, revision, commit):
        start = len(self.checks)
        label = ".".join((scenario, mode, revision))
        summary = self.doc(directory / "summary.json", label + ".summary")
        env = self.doc(directory / "environment.json", label + ".environment")
        meta = self.doc(directory / "metadata.json", label + ".metadata")
        probe = self.doc(directory / "probe.json", label + ".probe")
        recovery = self.doc(directory / "recovery.json", label + ".recovery")
        restore = self.doc(directory / "restore-self-test.json", label + ".restore_proof")
        reverse = self.doc(directory / "reverse-dependencies.json", label + ".reverse_dependencies")
        for field, expected in [("scenario", scenario), ("mode", mode), ("revision", revision), ("commit", commit)]:
            self.check(label + "." + field, summary.get(field) == expected, "Measurement identity differs from explicit manifest", directory / "summary.json", expected=expected)
        for name, obj in [("environment.json", env), ("metadata.json", meta)]:
            self.check(label + ".commit." + name, obj.get("commit") == commit, "Evidence commit differs from pinned revision", directory / name)
        for field in ("source_restored", "noop_all_business_units_fresh", "measurement_passed", "warmup_untimed"):
            self.check(label + "." + field, summary.get(field) is True, "Required completion flag is not true", directory / "summary.json", field=field)
        self.check(label + ".no_restore_error", not summary.get("restore_error"), "Recorded restoration error invalidates the group even if another flag says restored", directory / "summary.json")
        self.check(label + ".contracted_environment", env.get("profile") == "dev" and env.get("features") == "default (no --features)"
                   and env.get("CARGO_TERM_COLOR") == "never" and type(env.get("cpu_count")) is int and env["cpu_count"] > 0,
                   "Captured profile/features/CPU/color must agree with the measurement script contract", directory / "environment.json")
        selected = "erp-" + scenario if revision == "candidate" else summary.get("selected_package")
        self.check(label + ".selected_package", isinstance(selected, str) and bool(selected) and selected == summary.get("selected_package") == probe.get("package") == reverse.get("package"),
                   "Probe, summary and reverse-closure selected package mismatch", directory / "probe.json")
        selected = selected if isinstance(selected, str) else "<missing>"
        self.check(label + ".probe_identity", probe.get("id") == scenario and probe.get("path") == summary.get("probe_path")
                   and all(isinstance(probe.get(k), str) and bool(probe[k]) for k in ("symbol", "before", "after", "equivalence"))
                   and probe.get("before") != probe.get("after"), "Invalid probe identity, implementation edit or equivalence description", directory / "probe.json")
        self.check(label + ".reverse_closure", isinstance(reverse.get("reverse_closure"), list) and selected in reverse["reverse_closure"],
                   "Missing selected package's reverse dependency closure", directory / "reverse-dependencies.json")
        backup = self.blob(directory / "source-backup", label + ".source_backup")
        sha = hashlib.sha256(backup).hexdigest() if backup is not None else None
        self.check(label + ".source_hash", sha is not None and sha == probe.get("sha256_before") == recovery.get("sha256")
                   and recovery.get("source_rel") == probe.get("path") and recovery.get("before_present") is True,
                   "Backup bytes, recovery metadata and probe hash must agree", directory / "source-backup", directory / "recovery.json", directory / "probe.json")
        if backup is not None and isinstance(probe.get("before"), str) and isinstance(probe.get("after"), str):
            self.check(label + ".unique_probe", backup.count(probe["before"].encode()) == 1 and probe["after"].encode() not in backup,
                       "Original source needs exactly one before fragment and no after fragment", directory / "source-backup")
        self.check(label + ".restore_self_test", all(restore.get(k) is True for k in ("passed", "temporary_workspace", "simulated_failure_restored", "recovery_json_restore"))
                   and summary.get("restore_self_test") == restore, "Failure/recovery self-test is missing or inconsistent", directory / "restore-self-test.json")
        self.check(label + ".target_metadata", isinstance(env.get("target_dir"), str) and env["target_dir"] == meta.get("target_directory_measured")
                   and env["target_dir"] != meta.get("target_directory_default") and env.get("cwd") == meta.get("workspace_root"),
                   "Dedicated target and workspace metadata are inconsistent", directory / "environment.json", directory / "metadata.json")
        packages = meta.get("packages") if isinstance(meta.get("packages"), list) else []
        self.check(label + ".selected_workspace_package", selected in packages, "Selected crate is absent from workspace metadata", directory / "metadata.json")
        if revision == "candidate":
            self.check(label + ".candidate_domains", DOMAINS <= set(packages), "Final metadata is missing required domains; missing crates do not prove isolation", directory / "metadata.json", missing=sorted(DOMAINS - set(packages)))
        for sub, noop in [("warmup", False), ("noop", True), ("warmup/after", False), ("warmup/restore", False)]:
            self.run(directory / sub, mode, False, selected, False, label + "." + sub, noop)
        expected_dirs = [f"sample-{i:02d}" for i in range(1, 6)]
        actual_dirs = sorted(p.name for p in directory.glob("sample-*") if p.is_dir())
        self.check(label + ".five_sample_directories", actual_dirs == expected_dirs, "Require exactly samples 01-05; do not omit or discard samples", directory, actual=actual_dirs)
        samples = []
        for sub in expected_dirs:
            samples.append(self.run(directory / sub, mode, True, selected, revision == "candidate", label + "." + sub))
            self.run(directory / sub / "restore-before", mode, False, selected, False, label + "." + sub + ".restore")
        values = [s["seconds"] for s in samples]
        reported = summary.get("seconds")
        self.check(label + ".five_valid_samples", type(summary.get("valid_samples")) is int and summary["valid_samples"] == 5
                   and all(s["passed"] and s["seconds"] is not None for s in samples), "Require five successful valid edit samples", directory / "summary.json")
        self.check(label + ".raw_seconds", isinstance(reported, list) and len(reported) == 5 and all(number(x) is not None for x in reported) and reported == values,
                   "Summary seconds must equal all original samples in their original order", directory / "summary.json", raw_seconds=values)
        computed = median(values) if all(x is not None for x in values) else None
        self.check(label + ".median", computed is not None and number(summary.get("median_seconds")) == computed,
                   "Reported median differs from the exact middle of five original sample durations", directory / "summary.json", recomputed=computed)
        passed = self.passed_since(start)
        return {"directory": str(directory), "passed": passed, "commit": commit, "median_seconds": computed if passed else None,
                "observed_raw_seconds": values, "samples": samples, "environment": env, "probe": probe}

    def comparison(self, path, baseline, candidate, label):
        start = len(self.checks)
        a, b = baseline["environment"], candidate["environment"]
        refs = [baseline["directory"] + "/environment.json", candidate["directory"] + "/environment.json"]
        for field in ENV_FIELDS:
            valid = field in a and field in b and a[field] == b[field]
            if field not in {"RUSTFLAGS", "CARGO_INCREMENTAL", "codegen_backend"}:
                valid = valid and a.get(field) is not None and a.get(field) != ""
            self.check(label + ".environment." + field, valid, "Recorded build environment differs or is missing", *refs, field=field)
        self.check(label + ".toolchain", bool(toolchain(a.get("active_toolchain"))) and toolchain(a.get("active_toolchain")) == toolchain(b.get("active_toolchain")),
                   "Active toolchain differs beyond the worktree override annotation", *refs)
        self.check(label + ".filesystem", filesystem(a.get("target_dir_df")) is not None and filesystem(a.get("target_dir_df")) == filesystem(b.get("target_dir_df")),
                   "Captured df device/mount differs or cannot be parsed", *refs, baseline=filesystem(a.get("target_dir_df")), candidate=filesystem(b.get("target_dir_df")))
        for side in (baseline, candidate):
            load = side["environment"].get("loadavg")
            self.check(label + ".loadavg." + side["directory"], isinstance(load, list) and len(load) == 3 and
                       all(not isinstance(x, bool) and isinstance(x, (int, float, Decimal)) and Decimal(str(x)).is_finite() and x >= 0 for x in load),
                       "Require captured load observations and an explicit comparability review", side["directory"] + "/environment.json")
        for field in ("id", "symbol", "before", "after", "equivalence"):
            self.check(label + ".probe." + field, field in baseline["probe"] and baseline["probe"].get(field) == candidate["probe"].get(field),
                       "Baseline and candidate must use the same semantic probe", baseline["directory"] + "/probe.json", candidate["directory"] + "/probe.json", field=field)
        proof = self.doc(path, label + ".comparison_evidence")
        sides = {}
        for revision, group in [("baseline", baseline), ("candidate", candidate)]:
            side = proof.get(revision) if isinstance(proof.get(revision), dict) else {}
            sides[revision] = side
            self.check(label + ".proof_identity." + revision, side.get("commit") == group["commit"] and isinstance(side.get("directory"), str) and str(resolve(path.parent, side["directory"])) == group["directory"],
                       "Comparison record must bind the exact measurement directory and commit", path)
            self.check(label + ".proof_environment_hash." + revision, bool(side.get("environment_sha256")) and side.get("environment_sha256") == self.evidence.get(group["directory"] + "/environment.json", {}).get("sha256"),
                       "Comparison environment hash is missing or stale", path)
        pa, pb = sides["baseline"], sides["candidate"]
        for field in PROOF_FIELDS:
            valid = field in pa and field in pb and pa[field] == pb[field]
            if field == "build_jobs":
                valid = valid and type(pa.get(field)) is int and pa[field] > 0
            elif field.endswith("sha256"):
                valid = valid and isinstance(pa.get(field), str) and bool(re.fullmatch(r"[0-9a-f]{64}", pa[field]))
            else:
                valid = valid and isinstance(pa.get(field), str) and bool(pa[field].strip())
            self.check(label + ".attested." + field, valid, "Required hardware/media/jobs/script/config/dependency evidence is missing or unequal", path, field=field)
        load = proof.get("background_load") if isinstance(proof.get("background_load"), dict) else {}
        self.check(label + ".background_load_review", load.get("comparable") is True and isinstance(load.get("rationale"), str) and bool(load["rationale"].strip()),
                   "Load needs explicit comparable review and rationale; do not invent numerical tolerances", path)
        self.refs(proof.get("evidence"), path, label + ".environment_sources")
        self.refs(load.get("evidence"), path, label + ".load_sources")
        self.environment_fingerprints.append((str(path), {
            "attested": {field: pa.get(field) for field in PROOF_FIELDS},
            "recorded": {field: a.get(field) for field in ENV_FIELDS},
            "toolchain": toolchain(a.get("active_toolchain")),
            "filesystem": filesystem(a.get("target_dir_df")),
        }))
        return self.passed_since(start)


def evaluate(manifest_path):
    """Apply validity, comparability, isolation and independent check/build gates."""
    manifest_path = manifest_path.resolve()
    audit = Audit()
    manifest = audit.doc(manifest_path, "manifest")
    audit.check("manifest.schema", type(manifest.get("schema_version")) is int and manifest["schema_version"] == 1, "Require schema_version=1", manifest_path)
    commits = manifest.get("commits") if isinstance(manifest.get("commits"), dict) else {}
    for revision in REVISIONS:
        value = commits.get(revision)
        audit.check("manifest.commit." + revision, isinstance(value, str) and bool(re.fullmatch(r"[0-9a-f]{40}", value)),
                    "Pin a full lowercase 40-hex commit for each revision", manifest_path)
    audit.check("manifest.distinct_commits", bool(commits.get("baseline")) and commits.get("baseline") != commits.get("candidate"),
                "Baseline and candidate must identify distinct measured revisions", manifest_path)
    mapping = manifest.get("groups") if isinstance(manifest.get("groups"), dict) else {}
    audit.check("manifest.scenarios", set(mapping) == set(SCENARIOS), "Require exactly customer, sales and finance", manifest_path)
    manifest_valid = audit.passed_since(0)
    output, used_dirs, targets = {}, set(), set()
    for scenario in SCENARIOS:
        modes = mapping.get(scenario) if isinstance(mapping.get(scenario), dict) else {}
        audit.check("manifest.modes." + scenario, set(modes) == set(MODES), "Require check and build for each scenario", manifest_path)
        output[scenario] = {}
        for mode in MODES:
            entry = modes.get(mode) if isinstance(modes.get(mode), dict) else {}
            label, pair = scenario + "." + mode, {}
            pair_start = len(audit.checks)
            for revision in REVISIONS:
                loc = entry.get(revision)
                if not audit.check(label + ".directory." + revision, isinstance(loc, str) and bool(loc), "Explicit measurement directory is missing", manifest_path):
                    continue
                directory = resolve(manifest_path.parent, loc)
                audit.check(label + ".unique_directory." + revision, directory not in used_dirs, "Do not reuse evidence directories across groups", directory)
                used_dirs.add(directory)
                group = audit.group(directory, scenario, mode, revision, commits.get(revision))
                target = group["environment"].get("target_dir")
                target = str(Path(target).resolve()) if isinstance(target, str) and target else None
                audit.check(label + ".unique_target." + revision, target is not None and target not in targets, "Each group requires a distinct dedicated warmed target", directory / "environment.json")
                targets.add(target)
                pair[revision] = group
            proof = entry.get("comparison_evidence")
            comparable = False
            if audit.check(label + ".comparison_path", isinstance(proof, str) and bool(proof), "Explicit comparison evidence file is missing", manifest_path) and len(pair) == 2:
                comparable = audit.comparison(resolve(manifest_path.parent, proof), pair["baseline"], pair["candidate"], label)
            valid = manifest_valid and len(pair) == 2 and comparable and all(g["passed"] for g in pair.values()) and audit.passed_since(pair_start)
            result = {"groups": {rev: {k: v for k, v in group.items() if k not in {"environment", "probe"}} for rev, group in pair.items()},
                      "comparable": comparable, "eligible_for_performance_gate": valid, "improvement_percent": None,
                      "improved_at_least_30_percent": False, "regressed_at_most_10_percent": False}
            if valid:
                before, after = pair["baseline"]["median_seconds"], pair["candidate"]["median_seconds"]
                # Use exact rational cross-products; arbitrary decimal precision cannot flip an equality.
                ratio = (Fraction(before) - Fraction(after)) / Fraction(before)
                result.update(improvement_percent=(before - after) * 100 / before,
                              improvement_ratio_exact={"numerator": ratio.numerator, "denominator": ratio.denominator},
                              improved_at_least_30_percent=Fraction(after) * 100 <= Fraction(before) * 70,
                              regressed_at_most_10_percent=Fraction(after) * 100 <= Fraction(before) * 110)
            output[scenario][mode] = result
    comparison_start = len(audit.checks)
    if audit.environment_fingerprints:
        first_path, first = audit.environment_fingerprints[0]
        for path, fingerprint in audit.environment_fingerprints[1:]:
            audit.check("matrix.environment_consistency", fingerprint == first,
                        "Do not combine scenarios/modes measured on different hardware, media, jobs or build configuration", first_path, path)
    matrix_comparable = audit.passed_since(comparison_start)
    modes = {}
    for mode in MODES:
        rows = [output[scenario][mode] for scenario in SCENARIOS]
        valid = matrix_comparable and all(row["eligible_for_performance_gate"] for row in rows)
        wins = sum(row["improved_at_least_30_percent"] for row in rows)
        regressions = all(row["regressed_at_most_10_percent"] for row in rows)
        audit.check(mode + ".all_scenarios_valid", valid, "Missing/invalid scenarios cannot enter the acceptance denominator", manifest_path)
        audit.check(mode + ".two_improvements", valid and wins >= 2, "At least two valid scenarios must improve >=30%", manifest_path, qualifying_scenarios=wins)
        audit.check(mode + ".no_regression_over_10", valid and regressions, "Every valid scenario must regress <=10%", manifest_path)
        modes[mode] = {"passed": valid and wins >= 2 and regressions, "qualifying_scenarios": wins}
    return {"schema_version": 1, "passed": all(x["passed"] for x in audit.checks), "manifest": str(manifest_path),
            "commits": commits, "modes": modes, "scenarios": output, "failures": [x for x in audit.checks if not x["passed"]],
            "checks": audit.checks, "evidence": audit.evidence,
            "verification_scope": "Recorded measurement evidence only. No Cargo, live environment observation or database execution. Historical hardware/load/configuration and source restoration are supported by supplied records, not independently remeasured.",
            "sample_policy": "Exactly five original timed samples per group; no filtering; original order and exact median checked."}


def schema():
    """Emit the manifest and commit-bound comparison record contract."""
    return {"manifest": {"schema_version": 1, "commits": {r: "full lowercase 40-hex commit" for r in REVISIONS},
                         "groups": {s: {m: {"baseline": f"/evidence/baseline-{s}-{m}", "candidate": f"/evidence/candidate-{s}-{m}",
                                             "comparison_evidence": f"/evidence/comparison-{s}-{m}.json"} for m in MODES} for s in SCENARIOS}},
            "comparison_evidence": {r: {"directory": "exact measured directory", "commit": "exact measured commit",
                                        "environment_sha256": "SHA-256 of this group's environment.json",
                                        **{k: (12 if k == "build_jobs" else "equal recorded value on both sides") for k in PROOF_FIELDS}} for r in REVISIONS} |
                                    {"evidence": [{"path": "hardware/media/jobs/config/script/dependencies capture", "sha256": "64-hex digest"}],
                                     "background_load": {"comparable": True, "rationale": "Explain recorded load comparability without concealing differences",
                                                         "evidence": [{"path": "captured load/process observations", "sha256": "64-hex digest"}]}},
            "rules": ["Paths are absolute or relative to their containing manifest/proof.",
                      "Fingerprint fields ending in sha256 require real 64-hex digests; build_jobs is a positive integer.",
                      "Record script/configuration fingerprints and third-party version fingerprints separately from path-crate changes.",
                      "The evaluator checks references and equality; it does not authenticate externally supplied attestations.",
                      "Never modify, filter or trim original samples. Exit 0 means all gates pass; 1 means failed gates; 2 means invalid input/CLI."]}


def fixture(root):
    """Generate temporary full-schema evidence; never execute a subprocess."""
    commits = {"baseline": "a" * 40, "candidate": "b" * 40}
    manifest = {"schema_version": 1, "commits": commits, "groups": {}}
    capture = root / "environment-review.txt"
    capture.write_text("Synthetic hardware/media/jobs/config/dependencies and load record; self-test only.\n")
    ref = {"path": str(capture), "sha256": hashlib.sha256(capture.read_bytes()).hexdigest()}
    for scenario in SCENARIOS:
        manifest["groups"][scenario] = {}
        for mode in MODES:
            entry, proof = {}, {"evidence": [ref], "background_load": {"comparable": True, "rationale": "Synthetic equal workload", "evidence": [ref]}}
            for revision in REVISIONS:
                directory = root / f"{scenario}-{mode}-{revision}"
                directory.mkdir()
                entry[revision] = str(directory)
                selected = "services" if revision == "baseline" else "erp-" + scenario
                duration = 100 if revision == "baseline" else 110 if scenario == "finance" else 70
                values = [duration - 2, duration + 2, duration, duration - 1, duration + 1]
                source = b"fn probe() { before(); }\n"
                (directory / "source-backup").write_bytes(source)
                sha = hashlib.sha256(source).hexdigest()
                write(directory / "probe.json", {"id": scenario, "path": "probe.rs", "package": selected, "symbol": scenario + "_probe", "before": "before()", "after": "after()", "equivalence": "same result", "sha256_before": sha})
                write(directory / "recovery.json", {"source_rel": "probe.rs", "sha256": sha, "before_present": True})
                restored = {k: True for k in ("passed", "temporary_workspace", "simulated_failure_restored", "recovery_json_restore")}
                write(directory / "restore-self-test.json", restored)
                env = {"commit": commits[revision], "rustc_verbose": "rustc exact version", "cargo_verbose": "cargo exact version",
                       "active_toolchain": f"nightly (overridden by '{directory}/rust-toolchain.toml')", "toolchain_channel": "nightly",
                       "profile": "dev", "codegen_backend": "cranelift", "features": "default (no --features)", "RUSTFLAGS": None,
                       "CARGO_INCREMENTAL": None, "CARGO_TERM_COLOR": "never", "cpu_count": 12, "loadavg": [1, 1, 1],
                       "target_dir": str(root / "targets" / directory.name), "target_dir_df": "Filesystem Size Used Avail Use% Mounted on\n/dev/test 100G 50G 50G 50% /same-volume",
                       "cwd": str(root / (revision + "-repo") / "backend")}
                write(directory / "environment.json", env)
                write(directory / "metadata.json", {"commit": commits[revision], "packages": sorted(DOMAINS) if revision == "candidate" else ["services", "web-api"],
                      "target_directory_default": str(root / "daily-target"), "target_directory_measured": env["target_dir"], "workspace_root": env["cwd"]})
                write(directory / "reverse-dependencies.json", {"package": selected, "reverse_closure": [selected, "web-api"]})
                write(directory / "summary.json", {"scenario": scenario, "mode": mode, "revision": revision, "commit": commits[revision],
                      "valid_samples": 5, "seconds": values, "median_seconds": duration, "source_restored": True, "noop_all_business_units_fresh": True,
                      "measurement_passed": True, "selected_package": selected, "probe_path": "probe.rs", "warmup_untimed": True, "restore_self_test": restored})
                runs = [("warmup", False, False, 1), ("noop", False, True, 1), ("warmup/after", False, False, 1), ("warmup/restore", False, False, 1)]
                for index, seconds in enumerate(values, 1):
                    runs += [(f"sample-{index:02d}", True, False, seconds), (f"sample-{index:02d}/restore-before", False, False, 1)]
                for sub, timed, noop, seconds in runs:
                    folder = directory / sub
                    folder.mkdir(parents=True, exist_ok=True)
                    command = ["cargo", mode, "-p", "web-api"] + (["--bin", "web-api"] if mode == "build" else []) + ["--locked", "--message-format=json", "--timings"]
                    names = sorted(DOMAINS) if revision == "candidate" else [selected]
                    messages, artifacts = [], []
                    for name in names:
                        pid = f"registry+https://example.invalid/index#{name}@1.0.0"
                        fresh = noop or name != selected
                        messages.append({"reason": "compiler-artifact", "package_id": pid, "target": {"name": name.replace("-", "_"), "kind": ["lib"]}, "profile": {"debuginfo": 0}, "fresh": fresh})
                        artifacts.append({"package_id": pid, "package_name": name, "target_name": name.replace("-", "_"), "target_kind": ["lib"], "profile": 0, "fresh": fresh})
                    (folder / "cargo.jsonl").write_text("\n".join(json.dumps(x) for x in messages + [{"reason": "build-finished", "success": True}]) + "\n")
                    (folder / "cargo.stderr.log").write_text("")
                    (folder / "timings.html").write_text("<!DOCTYPE html><title>synthetic Cargo timings</title>")
                    dirty, fresh = [x for x in artifacts if not x["fresh"]], [x for x in artifacts if x["fresh"]]
                    write(folder / "units.json", {"artifacts": artifacts, "dirty": dirty, "fresh": fresh, "build_scripts": [], "non_json_lines": []})
                    write(folder / "sample.json", {"command": command, "exit_code": 0, "seconds": seconds if timed else None,
                          "wall_seconds_including_untimed": seconds, "timed": timed, "timings_source": str(root / "target/timings.html"),
                          "dirty_package_names": sorted({x["package_name"] for x in dirty}), "fresh_package_names": sorted({x["package_name"] for x in fresh}),
                          "business_dirty_package_names": sorted({x["package_name"] for x in dirty} & BUSINESS), "build_scripts_executed": []})
                proof[revision] = {"directory": str(directory), "commit": commits[revision], "environment_sha256": hashlib.sha256((directory / "environment.json").read_bytes()).hexdigest(),
                                   **{k: 12 if k == "build_jobs" else "c" * 64 if k.endswith("sha256") else "identical-recorded-identity" for k in PROOF_FIELDS}}
            proof_path = root / f"compare-{scenario}-{mode}.json"
            write(proof_path, proof)
            entry["comparison_evidence"] = str(proof_path)
            manifest["groups"][scenario][mode] = entry
    path = root / "manifest.json"
    write(path, manifest)
    return path


def self_test():
    """Exercise thresholds and rejection of omitted, forged or non-Fresh evidence."""
    results = []
    with tempfile.TemporaryDirectory(prefix="domain-performance-selftest-") as temporary:
        root = Path(temporary)
        manifest = fixture(root)
        result = evaluate(manifest)
        results.append({"case": "exact_30_improvement_and_exact_10_regression_pass", "passed": result["passed"], "failures": result["failures"]})
        def mutate(name, relative, edit, expected):
            path = root / relative
            original = path.read_bytes()
            try:
                value = json.loads(original)
                edit(value)
                write(path, value)
                result = evaluate(manifest)
                results.append({"case": name, "passed": not result["passed"] and any(expected in x["check"] for x in result["failures"]),
                                "expected_failure": expected, "observed_failures": [x["check"] for x in result["failures"]]})
            finally:
                path.write_bytes(original)
        mutate("missing_scenario", "manifest.json", lambda x: x["groups"].pop("finance"), "manifest.scenarios")
        mutate("four_samples", "customer-check-baseline/summary.json", lambda x: x.update(valid_samples=4), "five_valid_samples")
        mutate("source_not_restored", "customer-check-baseline/summary.json", lambda x: x.update(source_restored=False), ".source_restored")
        mutate("forged_median", "customer-check-baseline/summary.json", lambda x: x.update(median_seconds=1), ".median")
        mutate("reordered_summary", "customer-check-baseline/summary.json", lambda x: x.update(seconds=x["seconds"][::-1]), ".raw_seconds")
        mutate("nonzero_exit", "customer-check-candidate/sample-03/sample.json", lambda x: x.update(exit_code=1), ".exit")
        mutate("wrong_commit", "sales-build-candidate/summary.json", lambda x: x.update(commit="d" * 40), ".commit")
        mutate("missing_comparability", "manifest.json", lambda x: x["groups"]["sales"]["build"].pop("comparison_evidence"), ".comparison_path")
        mutate("different_media", "compare-finance-build.json", lambda x: x["candidate"].update(physical_device_id="different"), ".attested.physical_device_id")
        mutate("unreviewed_load", "compare-customer-check.json", lambda x: x["background_load"].update(comparable=False), ".background_load_review")
        mutate("changed_profile", "customer-check-candidate/environment.json", lambda x: x.update(profile="release"), ".environment.profile")
        mutate("failed_measurement_flag", "customer-check-baseline/summary.json", lambda x: x.update(measurement_passed=False), ".measurement_passed")
        mutate("missing_build_mode", "manifest.json", lambda x: x["groups"]["finance"].pop("build"), "manifest.modes")
        mutate("restore_command_failed", "sales-build-candidate/sample-02/restore-before/sample.json", lambda x: x.update(exit_code=2), ".restore.exit")
        mutate("untrue_noop_flag", "customer-check-baseline/summary.json", lambda x: x.update(noop_all_business_units_fresh=False), ".noop_all_business_units_fresh")
        mutate("restore_error_with_true_flag", "customer-check-baseline/summary.json", lambda x: x.update(restore_error="recorded failure"), ".no_restore_error")
        mutate("forged_source_hash", "customer-check-baseline/recovery.json", lambda x: x.update(sha256="0" * 64), ".source_hash")
        mutate("stale_source_evidence_reference", "compare-customer-check.json", lambda x: x["evidence"][0].update(sha256="0" * 64), ".environment_sources")
        mutate("unequal_build_jobs", "compare-customer-check.json", lambda x: x["candidate"].update(build_jobs=6), ".attested.build_jobs")
        mutate("same_changed_jobs_in_one_pair", "compare-customer-check.json", lambda x: (x["candidate"].update(build_jobs=6),x["baseline"].update(build_jobs=6)), "matrix.environment_consistency")
        for name, edit, expected in [
            ("sales_edit_dirty_finance", lambda rows: [dict(r, fresh=False) if r.get("package_id", "").endswith("#erp-finance@1.0.0") else r for r in rows], ".unrelated_fresh.erp-finance"),
            ("sales_edit_missing_finance", lambda rows: [r for r in rows if not r.get("package_id", "").endswith("#erp-finance@1.0.0")], ".unrelated_fresh.erp-finance"),
            ("string_fresh_is_not_boolean", lambda rows: [dict(r, fresh="true") if r.get("package_id", "").endswith("#erp-finance@1.0.0") else r for r in rows], ".artifact"),
        ]:
            path = root / "sales-check-candidate/sample-01/cargo.jsonl"
            original = path.read_bytes()
            try:
                path.write_text("\n".join(json.dumps(x) for x in edit([json.loads(x) for x in original.splitlines()])) + "\n")
                result = evaluate(manifest)
                results.append({"case": name, "passed": not result["passed"] and any(expected in x["check"] for x in result["failures"]), "expected_failure": expected})
            finally:
                path.write_bytes(original)
        path = root / "finance-build-candidate/sample-05/cargo.jsonl"
        original = path.read_bytes()
        try:
            rows = [json.loads(x) for x in original.splitlines()]
            rows = [dict(row, fresh=False) if row.get("package_id", "").endswith("#erp-sales@1.0.0") else row for row in rows]
            path.write_text("\n".join(json.dumps(row) for row in rows) + "\n")
            result = evaluate(manifest)
            results.append({"case": "finance_edit_dirty_sales", "passed": not result["passed"] and any(".unrelated_fresh.erp-sales" in x["check"] and x.get("details",{}).get("artifact_count") == 1 for x in result["failures"])})
        finally:
            path.write_bytes(original)
        path = root / "customer-check-baseline/sample-05"
        displaced = root / "temporarily-removed-sample"
        path.rename(displaced)
        try:
            result = evaluate(manifest)
            results.append({"case": "actual_missing_sample_directory", "passed": not result["passed"] and any("five_sample_directories" in x["check"] for x in result["failures"])})
        finally:
            displaced.rename(path)
        for name, scenario, duration, expected in [
            ("less_than_30_improvement", "sales", 70.000001, ".two_improvements"),
            ("more_than_10_regression", "finance", 110.000001, ".no_regression_over_10"),
        ]:
            paths = [root / f"{scenario}-check-candidate/summary.json", root / f"{scenario}-check-candidate/sample-03/sample.json"]
            originals = [p.read_bytes() for p in paths]
            try:
                summary, sample = [json.loads(x) for x in originals]
                summary["seconds"][2] = duration
                summary["median_seconds"] = duration
                sample["seconds"] = sample["wall_seconds_including_untimed"] = duration
                write(paths[0], summary)
                write(paths[1], sample)
                result = evaluate(manifest)
                results.append({"case": name, "passed": not result["passed"] and any(expected in x["check"] for x in result["failures"]), "expected_failure": expected})
            finally:
                for path, original in zip(paths, originals):
                    path.write_bytes(original)
        (root / "customer-check-baseline/sample-06").mkdir()
        result = evaluate(manifest)
        results.append({"case": "sixth_sample_not_silently_dropped", "passed": not result["passed"] and any("five_sample_directories" in x["check"] for x in result["failures"])})
    return {"self_test": True, "passed": all(x["passed"] for x in results), "cases": results,
            "execution": "Pure Python temporary fixtures only; no subprocess, Cargo, database or repository writes."}


def main():
    parser = argparse.ArgumentParser(add_help=False, exit_on_error=False)
    parser.add_argument("--manifest")
    parser.add_argument("--self-test", action="store_true")
    parser.add_argument("--schema", action="store_true")
    try:
        args, unknown = parser.parse_known_args()
        if unknown or sum(bool(x) for x in (args.manifest, args.self_test, args.schema)) != 1:
            print(encode({"passed": False, "fail_reason": "Choose one: --manifest PATH, --self-test, --schema", "unknown_args": unknown}))
            return 2
        result = self_test() if args.self_test else schema() if args.schema else evaluate(Path(args.manifest))
        print(encode(result))
        return 0 if args.schema or result["passed"] else 1
    except Exception as error:
        print(encode({"passed": False, "fail_reason": "Malformed input or evaluator error", "error_type": type(error).__name__, "error": str(error)}))
        return 2


if __name__ == "__main__":
    sys.exit(main())
