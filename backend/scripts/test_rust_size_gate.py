#!/usr/bin/env python3
"""Pure fixtures for the Rust size gate; no Cargo, MongoDB, or workspace scan."""

from __future__ import annotations

import unittest

import rust_size_gate as gate


def fn_body(lines: int, name: str = "item") -> str:
    """Build a function whose interior has exactly `lines` effective statements."""
    stmts = "\n".join(f"    let v{index} = {index};" for index in range(lines))
    return f"fn {name}() {{\n{stmts}\n}}\n"


class SizeGateTests(unittest.TestCase):
    """Assert file and function limits, including test-code exclusions."""

    def rules(self, path: str, source: str, **kwargs) -> set[str]:
        """Return blocking rule IDs for a single file."""
        findings, _ = gate.scan_source(path, source, **kwargs)
        return {item.rule for item in findings}

    def findings(self, path: str, source: str, **kwargs) -> list[gate.Finding]:
        """Return diagnostics for a single file."""
        found, _ = gate.scan_source(path, source, **kwargs)
        return found

    def test_file_at_limit_passes(self):
        source = "\n".join(f"const C{i}: u8 = 0;" for i in range(8)) + "\n"
        self.assertFalse(self.rules("src/lib.rs", source, max_file=8, max_fn=50))

    def test_file_over_limit_fails(self):
        source = "\n".join(f"const C{i}: u8 = 0;" for i in range(9)) + "\n"
        found = self.findings("src/lib.rs", source, max_file=8, max_fn=50)
        self.assertEqual(found[0].rule, "SIZE-FILE")
        self.assertIn("9 行", found[0].message)

    def test_cfg_test_module_excluded_from_file_size(self):
        production = "\n".join(f"const C{i}: u8 = 0;" for i in range(3))
        tests = "\n".join(f"        let x{i} = {i};" for i in range(40))
        source = f"{production}\n#[cfg(test)]\nmod tests {{\n{tests}\n}}\n"
        self.assertFalse(self.rules("src/lib.rs", source, max_file=6, max_fn=50))
        over = "\n".join(f"const C{i}: u8 = 0;" for i in range(7))
        source = f"{over}\n#[cfg(test)]\nmod tests {{\n{tests}\n}}\n"
        self.assertIn("SIZE-FILE", self.rules("src/lib.rs", source, max_file=6, max_fn=50))

    def test_string_and_comment_do_not_create_functions(self):
        source = '''
fn real() {
    let s = "fn fake() { let a = 1; }";
    // fn also_fake() { let b = 1; }
    let r = r#"fn raw() { let c = 1; }"#;
}
'''
        found = self.findings("src/lib.rs", source, max_file=800, max_fn=1)
        names = {item.message.split()[1] for item in found if item.rule == "SIZE-FN"}
        self.assertEqual(names, {"real"})

    def test_function_at_limit_passes(self):
        source = fn_body(5, "ok")
        self.assertFalse(self.rules("src/lib.rs", source, max_file=800, max_fn=5))

    def test_function_over_limit_fails(self):
        source = fn_body(6, "too_long")
        found = self.findings("src/lib.rs", source, max_file=800, max_fn=5)
        self.assertEqual(found[0].rule, "SIZE-FN")
        self.assertEqual(found[0].line, 1)
        self.assertIn("too_long", found[0].message)
        self.assertIn("6 个有效行", found[0].message)

    def test_blank_and_comment_lines_do_not_count_in_function(self):
        source = """fn padded() {
    let a = 1;

    // comment only
    let b = 2;
    /* block
       comment */
    let c = 3;
}
"""
        self.assertFalse(self.rules("src/lib.rs", source, max_file=800, max_fn=3))
        self.assertIn("SIZE-FN", self.rules("src/lib.rs", source, max_file=800, max_fn=2))

    def test_cfg_test_and_test_attr_functions_excluded(self):
        source = """
fn prod() {
    let a = 1;
}

#[cfg(test)]
fn helper() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
}

#[test]
fn a_test() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
}

#[tokio::test]
async fn tokio_test() {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
}
"""
        self.assertFalse(self.rules("src/lib.rs", source, max_file=800, max_fn=3))

    def test_trait_signature_without_body_is_ignored(self):
        source = "trait T { fn required(&self); fn another(&self) -> u8; }\n"
        self.assertFalse(self.rules("src/lib.rs", source, max_file=800, max_fn=1))

    def test_async_pub_crate_and_generic_functions(self):
        source = """
pub(crate) async fn fetch<T: Clone>(value: T) -> T
where
    T: Send,
{
    let a = value.clone();
    let b = a.clone();
    let c = b.clone();
    c
}
"""
        found = self.findings("src/lib.rs", source, max_file=800, max_fn=3)
        self.assertEqual(found[0].rule, "SIZE-FN")
        self.assertIn("fetch", found[0].message)

    def test_nested_function_is_counted_separately(self):
        source = """fn outer() {
    fn inner() {
        let a = 1;
        let b = 2;
        let c = 3;
    }
    inner();
}
"""
        found = self.findings("src/lib.rs", source, max_file=800, max_fn=2)
        names = sorted(item.message.split()[1] for item in found if item.rule == "SIZE-FN")
        self.assertEqual(names, ["inner", "outer"])

    def test_use_tree_braces_do_not_swallow_following_function(self):
        source = "use foo::{bar, baz};\n" + fn_body(4, "after_use")
        found = self.findings("src/lib.rs", source, max_file=800, max_fn=3)
        self.assertTrue(any("after_use" in item.message for item in found))

    def test_build_rs_skips_function_limit(self):
        source = fn_body(8, "generate")
        self.assertFalse(self.rules("apps/web-api/build.rs", source, max_file=800, max_fn=3))
        long_file = "\n".join(f"const C{i}: u8 = 0;" for i in range(5)) + "\n"
        self.assertIn("SIZE-FILE", self.rules("apps/web-api/build.rs", long_file, max_file=3, max_fn=50))

    def test_cfg_test_mod_semi_is_collected(self):
        source = "fn prod() {}\n#[cfg(test)]\nmod serialization_contract;\n"
        _findings, submods = gate.scan_source("crates/erp-x/src/lib.rs", source)
        self.assertEqual(submods, ["serialization_contract"])
        self.assertEqual(
            gate.submodule_paths("crates/erp-x/src/lib.rs", "serialization_contract"),
            ["crates/erp-x/src/serialization_contract.rs", "crates/erp-x/src/serialization_contract/mod.rs"],
        )
        self.assertEqual(
            gate.submodule_paths("crates/erp-x/src/foo.rs", "tests"),
            ["crates/erp-x/src/foo/tests.rs", "crates/erp-x/src/foo/tests/mod.rs"],
        )

    def test_workspace_skips_declared_and_named_test_files(self):
        helper = "\n".join(f"const T{i}: u8 = {i};" for i in range(20)) + "\n"
        sources = {
            "crates/erp-x/src/lib.rs": "fn prod() {}\n#[cfg(test)]\nmod serialization_contract;\n",
            "crates/erp-x/src/serialization_contract.rs": helper,
            "crates/erp-x/src/foo_tests.rs": helper,
            "crates/erp-x/tests/smoke.rs": helper,
            "apps/web-api/examples/s2_verify.rs": helper + fn_body(20, "verify"),
            "crates/erp-x/src/wide.rs": "\n".join(f"const C{i}: u8 = 0;" for i in range(10)) + "\n",
        }
        findings = gate.scan_workspace(sources, max_file=8, max_fn=50)
        paths = {item.path for item in findings}
        self.assertEqual(paths, {"crates/erp-x/src/wide.rs"})

    def test_crate_level_cfg_test_skips_whole_file(self):
        source = "#![cfg(test)]\n" + "\n".join(f"const C{i}: u8 = 0;" for i in range(20)) + "\n"
        self.assertFalse(self.rules("src/lib.rs", source, max_file=5, max_fn=1))

    def test_one_liner_function_is_one_effective_line(self):
        source = "fn tiny() { let a = 1; }\n"
        self.assertFalse(self.rules("src/lib.rs", source, max_file=800, max_fn=1))
        self.assertIn("SIZE-FN", self.rules("src/lib.rs", source, max_file=800, max_fn=0))

    def test_is_excluded_path(self):
        self.assertTrue(gate.is_excluded_path("crates/id-generator/tests/smoke.rs"))
        self.assertTrue(gate.is_excluded_path("crates/erp-x/src/foo/tests.rs"))
        self.assertTrue(gate.is_excluded_path("crates/erp-x/src/foo_tests.rs"))
        self.assertTrue(gate.is_excluded_path("apps/web-api/examples/s2_verify.rs"))
        self.assertFalse(gate.is_excluded_path("crates/erp-x/src/lib.rs"))
        self.assertFalse(gate.is_excluded_path("crates/erp-x/src/serialization_contract.rs"))

    def test_macro_crate_skips_function_limit(self):
        source = fn_body(8, "expand")
        self.assertFalse(self.rules("crates/entity-macros/src/lib.rs", source, max_file=800, max_fn=3))
        self.assertIn(
            "SIZE-FN",
            self.rules("crates/erp-core/src/lib.rs", source, max_file=800, max_fn=3),
        )


if __name__ == "__main__":
    unittest.main()
