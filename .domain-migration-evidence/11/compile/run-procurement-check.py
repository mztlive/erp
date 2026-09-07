"""Run the supplemental procurement isolation probe using the existing engine.

This reuses the five-sample, source-restore and no-op checks without altering the
three final performance scenarios or their thresholds. Run in the phase 11
worktree after committing source; output must be a new, empty directory.
"""
from argparse import Namespace
import importlib.util
import json
from pathlib import Path
import sys

repo = Path(__file__).resolve().parents[3]
script = repo / 'backend/scripts/measure-incremental.py'
spec = importlib.util.spec_from_file_location('migration_measure', script)
module = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = module
spec.loader.exec_module(module)
args = Namespace(
    repo=str(repo), revision='candidate',
    spec=str(Path(__file__).with_name('procurement-probe.json')),
    scenario='procurement', mode='check', samples=5,
    target_dir='/Volumes/Kingston/erp-domain-crate-11-procurement-check',
    output=str(Path(__file__).with_name('procurement-check')),
)
result = module.measure(args)
print(json.dumps(result, ensure_ascii=False, indent=2))
raise SystemExit(0 if result['measurement_passed'] else 1)
