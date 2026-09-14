#!/usr/bin/env python3
"""Pure regression fixtures for the chapter 9 gate; no MongoDB or Rust build."""

import contextlib
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import org_data_scope_gate as gate
from domain_boundaries import forbidden_graph_errors, graph_from_packages


GOOD_ADAPTER = '''
use erp_identity::service::access_control::resolve::DataScopeService;
impl CustomerDataScopePort for MongoCustomerDataScope {
    async fn resolve(&self, actor: &AuditActor, action: &str,
                     executor: &mut dyn Executor) -> Result<CustomerResolvedScope> {
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "customer", action, executor).await?;
        convert(access)
    }
}
'''


class SourceTests(unittest.TestCase):
    """Assert independent positive and negative boundary examples."""

    def rules(self, path, source):
        """Return blocking rule IDs for a fixture."""
        return {finding.rule for finding in gate.scan_source(path, source)}

    def test_customer_cannot_depend_on_identity_in_any_dependency_kind(self):
        for kind in ("normal", "build", "dev"):
            for rename in (None, "authorization_facts"):
                with self.subTest(kind=kind, rename=rename):
                    graph = graph_from_packages({"packages": [
                        {"name": "erp-customer", "deps": [
                            {"name": "erp-identity", "kind": kind, "rename": rename}]},
                        {"name": "erp-identity", "deps": []},
                    ]})
                    self.assertTrue(forbidden_graph_errors(graph))

    def test_composition_may_depend_on_identity_and_customer(self):
        graph = graph_from_packages({"packages": [
            {"name": "erp-processes", "deps": [{"name": "erp-identity"}, {"name": "erp-customer"}]},
            {"name": "erp-identity", "deps": []}, {"name": "erp-customer", "deps": []},
        ]})
        self.assertFalse(forbidden_graph_errors(graph))

    def test_port_resolved_facts_allowed(self):
        self.assertFalse(self.rules(gate.CUSTOMER_PORT,
                                    "struct Fact { role_clauses: Vec<Clause>, user_limit: Option<Clause> }"))

    def test_port_internal_alias_and_grouped_import_rejected(self):
        self.assertIn("ODS-PORT", self.rules(gate.CUSTOMER_PORT,
                                            "use erp_identity::{OrgTree as Tree, AuthorizedDataScope};"))

    def test_raw_facts_rejected_even_when_named_fact(self):
        self.assertIn("ODS-RAW", self.rules(gate.CUSTOMER_PORT,
                                           "struct Fact { scope_type: Kind, scope_targets: Vec<String> }"))

    def test_legacy_gap_remains_blocking(self):
        findings = gate.scan_source("crates/erp-workflow/src/ports/authorization.rs", "scope.scope_type")
        self.assertEqual(findings[0].category, "待接入项")
        self.assertEqual(findings[0].rule, "ODS-RAW")

    def test_foundation_must_not_own_scope(self):
        path = "crates/application-core/src/access.rs"
        self.assertIn("ODS-OWNER", self.rules(path, "struct ResolvedScope {}"))
        self.assertFalse(self.rules(path, "struct AuditActor {}"))

    def test_second_engine_rejected(self):
        self.assertIn("ODS-ENGINE", self.rules("crates/erp-sales/src/service/access.rs",
                                              "struct DataScopeService {}"))

    def test_repository_explicit_condition_allowed_auth_rejected(self):
        path = "crates/erp-customer/src/repository/scope.rs"
        self.assertFalse(self.rules(path, "fn document(scope: &CustomerReadScope) { scope.roles.iter(); }"))
        self.assertIn("ODS-REPOSITORY", self.rules(path, "db.data_scopes().list_by_subject(user, executor)"))

    def test_identity_repository_may_read_raw_storage(self):
        self.assertFalse(self.rules("crates/erp-identity/src/repository/scope.rs", "rule.scope_type"))

    def test_production_adapter_allowed(self):
        self.assertFalse(self.rules(gate.CUSTOMER_ADAPTER, GOOD_ADAPTER))

    def test_adapter_alias_allowed(self):
        source = GOOD_ADAPTER.replace("::DataScopeService;", "::DataScopeService as Resolver;")
        source = source.replace("DataScopeService::new", "Resolver::new")
        self.assertFalse(self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_adapter_type_name_alone_not_enough(self):
        source = GOOD_ADAPTER.replace("DataScopeService::new", "LegacyReader::new")
        self.assertIn("ODS-ADAPTER", self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_adapter_test_only_call_does_not_satisfy_wiring(self):
        source = GOOD_ADAPTER.replace("DataScopeService::new", "LegacyReader::new")
        source += '#[cfg(test)] mod tests { fn sample() { DataScopeService::new().resolve(); } }'
        self.assertIn("ODS-ADAPTER", self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_executor_swap_and_nested_transaction_rejected(self):
        for replacement in ("&mut NoTransaction", "db.with_transaction(f)", "db.start_session()"):
            with self.subTest(replacement=replacement):
                source = GOOD_ADAPTER.replace('"customer", action, executor', f'"customer", action, {replacement}')
                self.assertIn("ODS-EXECUTOR", self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_missing_executor_rejected(self):
        source = GOOD_ADAPTER.replace("executor: &mut dyn Executor", "other: usize")
        self.assertIn("ODS-EXECUTOR", self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_executor_parameter_must_reach_resolver(self):
        source = GOOD_ADAPTER.replace('"customer", action, executor', '"customer", action, other_executor')
        self.assertIn("ODS-EXECUTOR", self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_unrelated_resolver_construction_does_not_satisfy_call(self):
        source = GOOD_ADAPTER.replace('.resolve(actor, "customer", action, executor)',
                                      '; legacy.resolve(actor, "customer", action, executor)')
        self.assertIn("ODS-ADAPTER", self.rules(gate.CUSTOMER_ADAPTER, source))

    def test_default_success_and_company_fallback_rejected(self):
        for fallback in (".await.unwrap_or_default()", ".await.unwrap_or(company)",
                         ".await.unwrap_or_else(|_| company)"):
            with self.subTest(fallback=fallback):
                self.assertIn("ODS-FALLBACK", self.rules(gate.CUSTOMER_ADAPTER,
                                                        GOOD_ADAPTER.replace(".await?", fallback)))

    def test_comments_strings_nested_comments_do_not_trigger_rules(self):
        source = r'''// scope.scope_type
        /* outer /* scope_targets */ OrgTree */
        const HELP: &str = "scope_targets and NoTransaction";
        const RAW: &str = r###"OrgTree \" scope_type"###;
        fn char_value() { let c = '}'; }
        '''
        self.assertFalse(self.rules(gate.CUSTOMER_PORT, source))

    def test_inline_tests_removed_without_hiding_later_production(self):
        source = "#[cfg(test)] mod tests { fn f() { let x = DataScope { scope_type: T }; } }\n\nfn f() { scope.scope_targets; }"
        findings = gate.scan_source(gate.CUSTOMER_PORT, source)
        self.assertEqual(len(findings), 1)
        self.assertEqual(findings[0].line, 3)

    def test_all_non_test_cfg_branches_checked(self):
        source = '#[cfg(feature = "optional")] fn f() { scope.scope_type; }'
        self.assertIn("ODS-RAW", self.rules(gate.CUSTOMER_PORT, source))

    def test_initializer_is_not_admission(self):
        source = "fn ensure_resource() { predefined_data_scopes::RESOURCE_ACTIONS.iter(); }"
        self.assertIn("ODS-REGISTRY", self.rules(gate.RESOLVER, source))
        self.assertNotIn("ODS-REGISTRY", self.rules(gate.RESOLVER, "registry.require_integrated(resource, action)?;"))

    def test_global_required_dimensions_rejected(self):
        source = "ScopeResolution { required_dimensions: &[ScopeDimension::InternalOrg] }"
        self.assertIn("ODS-DIMENSIONS", self.rules(gate.RESOLVER, source))
        self.assertNotIn("ODS-DIMENSIONS", self.rules(gate.RESOLVER,
                                                     "ScopeResolution { required_dimensions: registration.required_dimensions }"))

    def test_sample_missing_empty_or_comment_only_fails(self):
        self.assertEqual(len(gate.check_required({})), len(gate.REQUIRED))
        self.assertTrue(gate.check_required({path: "" for path in gate.REQUIRED}))
        self.assertTrue(gate.check_required({gate.CUSTOMER_PORT: "// trait CustomerDataScopePort {}"}))

    def test_sample_real_structures_present(self):
        sources = {
            gate.RESOLVER: "struct DataScopeService {} fn f() { ScopeResolution {} }",
            gate.CUSTOMER_PORT: "trait CustomerDataScopePort { fn f(executor: &mut dyn Executor); }",
            gate.CUSTOMER_ADAPTER: GOOD_ADAPTER,
            gate.CUSTOMER_ACCESS: "struct CustomerAccess { scope: Arc<dyn CustomerDataScopePort> }",
        }
        self.assertFalse(gate.check_required(sources))

    def test_malformed_test_block_fails_closed(self):
        with self.assertRaises(ValueError):
            gate.production_code("#[cfg(test)] mod tests { fn f() {}")


class CommandTests(unittest.TestCase):
    """Verify exit status semantics, including failed dependencies and inventory."""

    def invoke(self, findings, domain_code=0):
        """Run CLI orchestration against bounded fake tool results."""
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(gate, "self_tests", return_value=True), \
                patch.object(gate, "workspace", return_value=(findings, 4)), \
                patch.object(gate.subprocess, "run") as run, \
                contextlib.redirect_stdout(io.StringIO()) as output:
            run.return_value.returncode = domain_code
            result = gate.main(["--backend", directory, "--json"])
            return result, output.getvalue()

    def test_static_success_is_not_acceptance(self):
        result, output = self.invoke([])
        self.assertEqual(result, 0)
        self.assertIn("STATIC_CHECKS_PASSED", output)
        self.assertIn("not_proven", output)

    def test_legacy_findings_fail_exit(self):
        finding = gate.Finding("ODS-RAW", "legacy.rs", 1, "raw scope", "待接入项")
        result, _ = self.invoke([finding])
        self.assertEqual(result, 1)

    def test_existing_domain_gate_failure_is_not_swallowed(self):
        result, output = self.invoke([], domain_code=1)
        self.assertEqual(result, 1)
        self.assertIn("ODS-DOMAIN", output)

    def test_metadata_failure_is_tool_error(self):
        with patch.object(gate, "self_tests", return_value=True), \
                patch.object(gate.subprocess, "run"), \
                patch.object(gate, "workspace", side_effect=RuntimeError("metadata failed")), \
                contextlib.redirect_stdout(io.StringIO()) as output:
            result = gate.main(["--backend", str(Path.cwd()), "--json"])
        self.assertEqual(result, 2)
        self.assertIn("TOOL_ERROR", output.getvalue())


if __name__ == "__main__":
    unittest.main()
