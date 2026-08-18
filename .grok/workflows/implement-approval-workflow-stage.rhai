let meta = #{
    name: "implement-approval-workflow-stage",
    description: "Implement exactly one approval-workflow stage from docs/dev-plan/_meta.json, including a stages[] object or a perDocumentTypeStages[].p3/.p4 object. All subagents are general-purpose. Scope and dependency proofs are general-purpose execute work; every project-file addition, modification, deletion, test/document change, review repair, or quality-gate repair is exclusively general-purpose with capability all. Baseline and quality-gate execution are general-purpose execute work and are never reviews. All contract, architecture, security, correctness, scope, test, and final acceptance reviews are general-purpose execute work and must not edit files. MongoDB integration tests and browser E2E tests are excluded. Ordinary success uses 8 agents; the maximum path is 21 agents after two review repairs, one quality-gate repair, and the mandatory post-quality review panel.",
    when_to_use: "Use only on a clean, already-prepared stage branch with args.stage_id and optional args.base_ref (default main); agents are instructed not to create branches, commits, pushes, PRs, or nested workflows, and blocked runs may leave uncommitted modifications that require final status/diff inspection.",
    phases: [
        #{ title: "Scope", detail: "Resolve the exact metadata stage and prove branch, dependency, ownership, and precondition safety" },
        #{ title: "Baseline", detail: "Run the applicable pre-stage checks without edits" },
        #{ title: "Implement", detail: "Use only general-purpose/all for implementation or repair" },
        #{ title: "Review", detail: "Run independent general-purpose read-only review panels" },
        #{ title: "Repair", detail: "Route findings and gate failures only to general-purpose/all" },
        #{ title: "Verify", detail: "Run metadata acceptance and all applicable merge gates" },
        #{ title: "Final Review", detail: "Perform independent general-purpose final acceptance" },
        #{ title: "Report", detail: "Write a scratch Markdown evidence report" },
    ],
};

let scope_plan_schema = #{
    "type": "object",
    "required": [
        "stage_id", "stage_source", "stage_kind", "document_type", "approval_requirement", "plan_doc",
        "branch", "depends_on", "dependency_groups", "owns", "owns_within", "creates", "deletes",
        "forbids", "acceptance", "backend_merge_gates", "frontend_merge_gates", "docs_merge_gates",
        "baseline_commands", "implementation_steps", "metadata_checked", "stage_exists", "dependencies_satisfied",
        "branch_matches", "clean_start", "preconditions_clear", "ownership_complete", "gates_complete",
        "scope_safe", "blockers", "evidence"
    ],
    "properties": #{
        "stage_id": #{ "type": "string", "minLength": 1 },
        "stage_source": #{ "type": "string", "minLength": 1 },
        "stage_kind": #{ "type": "string", "minLength": 1 },
        "document_type": #{ "type": "string" },
        "approval_requirement": #{ "type": "string" },
        "plan_doc": #{ "type": "string", "minLength": 1 },
        "branch": #{ "type": "string", "minLength": 1 },
        "depends_on": #{ "type": "array", "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
        "dependency_groups": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string", "minLength": 1 } },
        "owns": #{ "type": "array", "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "owns_within": #{
            "type": "array",
            "maxItems": 128,
            "items": #{
                "type": "object",
                "required": ["path", "scope"],
                "properties": #{
                    "path": #{ "type": "string", "minLength": 1 },
                    "scope": #{ "type": "string", "minLength": 1 },
                },
            },
        },
        "creates": #{ "type": "array", "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "deletes": #{ "type": "array", "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "forbids": #{ "type": "array", "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "acceptance": #{ "type": "array", "minItems": 1, "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
        "backend_merge_gates": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string", "minLength": 1 } },
        "frontend_merge_gates": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string", "minLength": 1 } },
        "docs_merge_gates": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string", "minLength": 1 } },
        "baseline_commands": #{ "type": "array", "minItems": 1, "maxItems": 16, "items": #{ "type": "string", "minLength": 1 } },
        "implementation_steps": #{ "type": "array", "minItems": 1, "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
        "metadata_checked": #{ "type": "boolean" },
        "stage_exists": #{ "type": "boolean" },
        "dependencies_satisfied": #{ "type": "boolean" },
        "branch_matches": #{ "type": "boolean" },
        "clean_start": #{ "type": "boolean" },
        "preconditions_clear": #{ "type": "boolean" },
        "ownership_complete": #{ "type": "boolean" },
        "gates_complete": #{ "type": "boolean" },
        "scope_safe": #{ "type": "boolean" },
        "blockers": #{ "type": "array", "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
        "evidence": #{ "type": "array", "minItems": 1, "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
    },
};

let baseline_schema = #{
    "type": "object",
    "required": ["baseline_green", "git_status_clean", "branch_matches", "current_branch", "stage_plan_consistent", "commands", "evidence", "blocked_reason"],
    "properties": #{
        "baseline_green": #{ "type": "boolean" },
        "git_status_clean": #{ "type": "boolean" },
        "branch_matches": #{ "type": "boolean" },
        "current_branch": #{ "type": "string", "minLength": 1 },
        "stage_plan_consistent": #{ "type": "boolean" },
        "commands": #{
            "type": "array",
            "minItems": 1,
            "maxItems": 32,
            "items": #{
                "type": "object",
                "required": ["command", "passed", "summary", "evidence"],
                "properties": #{
                    "command": #{ "type": "string", "minLength": 1 },
                    "passed": #{ "type": "boolean" },
                    "summary": #{ "type": "string", "minLength": 1 },
                    "evidence": #{ "type": "string", "minLength": 1 },
                },
            },
        },
        "evidence": #{ "type": "array", "minItems": 1, "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
        "blocked_reason": #{ "type": "string", "minLength": 1 },
    },
};

let implementation_schema = #{
    "type": "object",
    "required": [
        "completed", "scope_respected", "forbidden_paths_untouched", "generated_files_handled",
        "files_changed", "commands", "symbols_searched", "summary", "blockers"
    ],
    "properties": #{
        "completed": #{ "type": "boolean" },
        "scope_respected": #{ "type": "boolean" },
        "forbidden_paths_untouched": #{ "type": "boolean" },
        "generated_files_handled": #{ "type": "boolean" },
        "files_changed": #{ "type": "array", "minItems": 1, "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "commands": #{ "type": "array", "minItems": 1, "maxItems": 32, "items": #{ "type": "string", "minLength": 1 } },
        "symbols_searched": #{ "type": "array", "minItems": 1, "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "summary": #{ "type": "string", "minLength": 1 },
        "blockers": #{ "type": "array", "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
    },
};

let review_schema = #{
    "type": "object",
    "required": [
        "reviewer_kind", "approved", "scope_clean", "metadata_match", "contract_ok", "tests_sufficient",
        "evidence", "findings", "summary"
    ],
    "properties": #{
        "reviewer_kind": #{ "type": "string", "minLength": 1 },
        "approved": #{ "type": "boolean" },
        "scope_clean": #{ "type": "boolean" },
        "metadata_match": #{ "type": "boolean" },
        "contract_ok": #{ "type": "boolean" },
        "tests_sufficient": #{ "type": "boolean" },
        "evidence": #{ "type": "array", "minItems": 1, "maxItems": 64, "items": #{ "type": "string", "minLength": 1 } },
        "findings": #{
            "type": "array",
            "maxItems": 64,
            "items": #{
                "type": "object",
                "required": ["severity", "file", "issue", "evidence", "remediation"],
                "properties": #{
                    "severity": #{ "type": "string", "minLength": 1 },
                    "file": #{ "type": "string", "minLength": 1 },
                    "issue": #{ "type": "string", "minLength": 1 },
                    "evidence": #{ "type": "string", "minLength": 1 },
                    "remediation": #{ "type": "string", "minLength": 1 },
                },
            },
        },
        "summary": #{ "type": "string", "minLength": 1 },
    },
};

let verification_schema = #{
    "type": "object",
    "required": [
        "all_pass", "metadata_acceptance_pass", "scope_clean", "commands", "acceptance_results", "evidence",
        "environment_blocked", "summary"
    ],
    "properties": #{
        "all_pass": #{ "type": "boolean" },
        "metadata_acceptance_pass": #{ "type": "boolean" },
        "scope_clean": #{ "type": "boolean" },
        "commands": #{
            "type": "array",
            "minItems": 1,
            "maxItems": 128,
            "items": #{
                "type": "object",
                "required": ["command", "passed", "summary", "evidence"],
                "properties": #{
                    "command": #{ "type": "string", "minLength": 1 },
                    "passed": #{ "type": "boolean" },
                    "summary": #{ "type": "string", "minLength": 1 },
                    "evidence": #{ "type": "string", "minLength": 1 },
                },
            },
        },
        "acceptance_results": #{
            "type": "array",
            "minItems": 0,
            "maxItems": 64,
            "items": #{
                "type": "object",
                "required": ["item", "passed", "evidence"],
                "properties": #{
                    "item": #{ "type": "string", "minLength": 1 },
                    "passed": #{ "type": "boolean" },
                    "evidence": #{ "type": "string", "minLength": 1 },
                },
            },
        },
        "evidence": #{ "type": "array", "minItems": 1, "maxItems": 128, "items": #{ "type": "string", "minLength": 1 } },
        "environment_blocked": #{ "type": "boolean" },
        "summary": #{ "type": "string", "minLength": 1 },
    },
};

fn is_nonempty_text(value) {
    if type_of(value) == "string" {
        value != ""
    } else {
        false
    }
}

fn is_bool_value(value) {
    type_of(value) == "bool"
}

fn is_text_array(value) {
    type_of(value) == "array"
}

fn is_nonempty_text_array(value) {
    let valid = false;
    if is_text_array(value) {
        if value.len() > 0 {
            valid = true;
            for item in value {
                if !is_nonempty_text(item) {
                    valid = false;
                }
            }
        }
    }
    valid
}

fn text_array_items_nonempty(value) {
    let valid = false;
    if is_text_array(value) {
        valid = true;
        for item in value {
            if !is_nonempty_text(item) {
                valid = false;
            }
        }
    }
    valid
}

fn owns_within_items_valid(value) {
    let valid = false;
    if is_text_array(value) {
        valid = true;
        for item in value {
            if item == () || type_of(item) != "map" || !is_nonempty_text(item.path) || !is_nonempty_text(item.scope) {
                valid = false;
            }
        }
    }
    valid
}

fn array_contains_text(items, wanted) {
    let found = false;
    if is_text_array(items) {
        for item in items {
            if item == wanted {
                found = true;
            }
        }
    }
    found
}

fn add_unique_text(items, value) {
    if is_nonempty_text(value) {
        if !array_contains_text(items, value) {
            items.push(value);
        }
    }
    items
}

fn safe_base_ref(value) {
    let valid = false;
    if type_of(value) == "string" {
        if value != "" {
            valid = true;
            if value.starts_with("-") || value.starts_with("/") || value.starts_with(".") || value.starts_with("..") || value == "." || value == ".." {
                valid = false;
            }
            if value.ends_with("/") || value.ends_with(".") || value.contains("..") || value.contains("@{") {
                valid = false;
            }
            if value.contains(" ") || value.contains("\n") || value.contains("\r") || value.contains("\t") {
                valid = false;
            }
            if value.contains("\\") || value.contains("\"") || value.contains("'") {
                valid = false;
            }
            if value.contains(";") || value.contains("|") || value.contains("&") || value.contains("<") || value.contains(">") {
                valid = false;
            }
            if value.contains("$") || value.contains("`") || value.contains("(") || value.contains(")") {
                valid = false;
            }
            if value.contains("@") || value.contains("{") || value.contains("}") || value.contains(",") {
                valid = false;
            }
            if value.contains("^") || value.contains(":") || value.contains("~") || value.contains("*") || value.contains("?") {
                valid = false;
            }
            if value.contains("[") || value.contains("]") || value.contains("!") || value.contains("%") || value.contains("=") || value.contains("+") {
                valid = false;
            }
            if value.contains("/") && (value.contains("//") || value.contains("/.")) {
                valid = false;
            }
        }
    }
    valid
}

fn path_in_surface(value, root) {
    let matched = false;
    if is_nonempty_text(value) {
        if value == root || value.starts_with(root + "/") {
            matched = true;
        }
    }
    matched
}

fn scope_has_surface(plan, root) {
    let matched = false;
    if plan.owns != () {
        for item in plan.owns {
            if path_in_surface(item, root) {
                matched = true;
            }
        }
    }
    if plan.owns_within != () {
        for item in plan.owns_within {
            if item != () && type_of(item) == "map" {
                if path_in_surface(item.path, root) {
                    matched = true;
                }
            }
        }
    }
    if plan.creates != () {
        for item in plan.creates {
            if path_in_surface(item, root) {
                matched = true;
            }
        }
    }
    if plan.deletes != () {
        for item in plan.deletes {
            if path_in_surface(item, root) {
                matched = true;
            }
        }
    }
    matched
}

fn executable_acceptance_item(value) {
    let executable = false;
    if is_nonempty_text(value) {
        if value.starts_with("cargo ") || value.starts_with("npm ") || value.starts_with("./") || value.starts_with("git ") || value.starts_with("jq ") || value.starts_with("bash ") || value.starts_with("sh ") || value.starts_with("node ") || value.starts_with("python ") {
            executable = true;
        }
    }
    executable
}

fn excluded_external_test(value) {
    let excluded = false;
    if is_nonempty_text(value) {
        if value.contains("--include-ignored") || value.contains("--ignored") || value.contains("ERP_TEST_MONGO_URI") || value.contains("require_mongo") {
            excluded = true;
        }
        let mentions_mongo = value.contains("MongoDB") || value.contains("mongodb") || value.contains("Mongo") || value.contains("mongo");
        let mentions_test = value.contains("integration test") || value.contains("Integration test") || value.contains("test") || value.contains("Test") || value.contains("集成测试") || value.contains("测试");
        if mentions_mongo && mentions_test {
            excluded = true;
        }
        if value.contains("E2E") || value.contains("e2e") || value.contains("end-to-end") || value.contains("End-to-end") || value.contains("Playwright") || value.contains("playwright") || value.contains("Cypress") || value.contains("cypress") || value.contains("浏览器端到端") || value.contains("浏览器验收") {
            excluded = true;
        }
    }
    excluded
}

fn without_excluded_external_tests(items) {
    let filtered = [];
    if is_text_array(items) {
        for item in items {
            if is_nonempty_text(item) && !excluded_external_test(item) {
                filtered = add_unique_text(filtered, item);
            }
        }
    }
    filtered
}

fn implementation_paths_safe(paths) {
    let valid = false;
    if is_nonempty_text_array(paths) {
        valid = true;
        for item in paths {
            if item.starts_with("/") || item.starts_with(".grok/") || item.contains("..") {
                valid = false;
            }
        }
    }
    valid
}

fn implementation_result_valid(output) {
    let valid = false;
    if output != () {
        if type_of(output) == "map" {
            let fields_ok = is_bool_value(output.completed) && is_bool_value(output.scope_respected) && is_bool_value(output.forbidden_paths_untouched) && is_bool_value(output.generated_files_handled) && is_nonempty_text(output.summary);
            let commands_ok = is_nonempty_text_array(output.commands);
            let symbols_ok = is_nonempty_text_array(output.symbols_searched);
            let files_ok = implementation_paths_safe(output.files_changed);
            let blockers_ok = output.blockers != () && is_text_array(output.blockers) && output.blockers.len() == 0;
            valid = fields_ok && commands_ok && symbols_ok && files_ok && blockers_ok && output.completed == true && output.scope_respected == true && output.forbidden_paths_untouched == true && output.generated_files_handled == true;
        }
    }
    valid
}

fn command_results_basic_valid(results) {
    let valid = false;
    if is_text_array(results) {
        if results.len() > 0 {
            valid = true;
            for item in results {
                if item == () {
                    valid = false;
                } else if type_of(item) != "map" {
                    valid = false;
                } else if !is_nonempty_text(item.command) || !is_bool_value(item.passed) || !is_nonempty_text(item.summary) || !is_nonempty_text(item.evidence) || item.passed != true {
                    valid = false;
                }
            }
        }
    }
    valid
}

fn command_results_exact(results, expected) {
    let valid = false;
    if is_text_array(expected) {
        if is_text_array(results) {
            if results.len() == expected.len() {
                valid = true;
                let seen = [];
                for item in results {
                    if item == () || type_of(item) != "map" {
                        valid = false;
                    } else {
                        if !is_nonempty_text(item.command) || !is_bool_value(item.passed) || !is_nonempty_text(item.summary) || !is_nonempty_text(item.evidence) {
                            valid = false;
                        }
                        if !array_contains_text(expected, item.command) || array_contains_text(seen, item.command) {
                            valid = false;
                        }
                        if item.passed != true {
                            valid = false;
                        }
                        seen.push(item.command);
                    }
                }
                for wanted in expected {
                    if !array_contains_text(seen, wanted) {
                        valid = false;
                    }
                }
            }
        }
    }
    valid
}

fn acceptance_results_exact(results, expected) {
    let valid = false;
    if is_text_array(expected) {
        if is_text_array(results) {
            if results.len() == expected.len() {
                valid = true;
                let index = 0;
                for item in results {
                    if item == () || type_of(item) != "map" {
                        valid = false;
                    } else {
                        if !is_nonempty_text(item.item) || !is_bool_value(item.passed) || !is_nonempty_text(item.evidence) {
                            valid = false;
                        }
                        if item.item != expected[index] || item.passed != true {
                            valid = false;
                        }
                    }
                    index += 1;
                }
            }
        }
    }
    valid
}

fn finding_valid(finding) {
    let valid = false;
    if finding != () {
        if type_of(finding) == "map" {
            valid = is_nonempty_text(finding.severity) && is_nonempty_text(finding.file) && is_nonempty_text(finding.issue) && is_nonempty_text(finding.evidence) && is_nonempty_text(finding.remediation);
        }
    }
    valid
}

fn assess_one_review(output, expected_kind) {
    let assessed = #{ usable: false, passed: false, findings: [], reason: "review output was missing" };
    if output != () {
        if type_of(output) == "map" {
            let shape_ok = is_nonempty_text(output.reviewer_kind) && is_bool_value(output.approved) && is_bool_value(output.scope_clean) && is_bool_value(output.metadata_match) && is_bool_value(output.contract_ok) && is_bool_value(output.tests_sufficient) && is_nonempty_text_array(output.evidence) && is_text_array(output.findings) && is_nonempty_text(output.summary);
            if shape_ok {
                if output.reviewer_kind != expected_kind {
                    assessed.reason = "reviewer_kind did not match the assigned review";
                } else {
                    let findings_ok = true;
                    for finding in output.findings {
                        if !finding_valid(finding) {
                            findings_ok = false;
                        }
                    }
                    let core_ok = output.scope_clean == true && output.metadata_match == true && output.contract_ok == true && output.tests_sufficient == true;
                    if !findings_ok {
                        assessed.reason = "review findings were malformed";
                    } else if output.findings.len() == 0 && (output.approved != true || !core_ok) {
                        assessed.reason = "review was not approved without a concrete finding";
                    } else {
                        assessed.usable = true;
                        assessed.findings = output.findings;
                        assessed.passed = output.approved == true && core_ok && output.findings.len() == 0;
                        if assessed.passed {
                            assessed.reason = "review passed";
                        } else {
                            assessed.reason = "review findings or failed proof must be repaired";
                        }
                    }
                }
            } else {
                assessed.reason = "review output lacked required non-empty evidence fields";
            }
        }
    }
    assessed
}

fn assess_review_panel(results, expected_count) {
    let assessed = #{ usable: false, passed: false, findings: [], summaries: [], reasons: [] };
    if results == () {
        assessed.reasons.push("parallel review returned unit");
    } else if !is_text_array(results) {
        assessed.reasons.push("parallel review returned a non-array");
    } else if results.len() != expected_count {
        assessed.reasons.push("review panel returned an unexpected number of results");
    } else {
        let usable = true;
        let passed = true;
        let index = 0;
        for result in results {
            let expected_kind = "contract";
            if index == 1 {
                expected_kind = "ownership";
            }
            if index == 2 {
                expected_kind = "tests";
            }
            let output = ();
            let result_usable = false;
            let result_assessment = #{ usable: false, passed: false, findings: [], reason: "review agent failed" };
            if result != () {
                if type_of(result) == "map" {
                    if result.success == true {
                        if result.output != () {
                            output = result.output;
                            result_assessment = assess_one_review(output, expected_kind);
                        }
                    }
                }
            }
            if result_assessment.usable == true {
                result_usable = true;
                assessed.summaries.push(output);
                for finding in result_assessment.findings {
                    assessed.findings.push(finding);
                }
                if result_assessment.passed != true {
                    passed = false;
                }
            } else {
                usable = false;
                assessed.reasons.push("reviewer " + index.to_string() + ": " + result_assessment.reason);
            }
            if !result_usable {
                usable = false;
            }
            index += 1;
        }
        assessed.usable = usable;
        assessed.passed = usable && passed;
    }
    assessed
}

fn assess_verification(output, expected_commands, acceptance_checks) {
    let assessed = #{ usable: false, passed: false, environment_blocked: false, reason: "verification output was missing" };
    if output != () {
        if type_of(output) == "map" {
            let shape_ok = is_bool_value(output.all_pass) && is_bool_value(output.metadata_acceptance_pass) && is_bool_value(output.scope_clean) && is_text_array(output.commands) && is_text_array(output.acceptance_results) && is_nonempty_text_array(output.evidence) && is_bool_value(output.environment_blocked) && is_nonempty_text(output.summary);
            if shape_ok {
                let command_ok = command_results_exact(output.commands, expected_commands);
                let acceptance_ok = acceptance_results_exact(output.acceptance_results, acceptance_checks);
                let environment_blocked = output.environment_blocked == true;
                assessed.usable = true;
                assessed.environment_blocked = environment_blocked;
                assessed.passed = output.all_pass == true && output.metadata_acceptance_pass == true && output.scope_clean == true && command_ok && acceptance_ok && !environment_blocked;
                if assessed.passed {
                    assessed.reason = "all exact quality-gate and acceptance evidence passed";
                } else if environment_blocked {
                    assessed.reason = "required command environment was blocked";
                } else {
                    assessed.reason = "quality-gate or exact acceptance evidence failed";
                }
            } else {
                assessed.reason = "verification output lacked required non-empty evidence fields";
            }
        }
    }
    assessed
}

fn build_review_jobs(plan, base_ref, implementation_evidence, review_schema, label_prefix, panel_note) {
    let jobs = [];
    let base_ref_json = json_encode(base_ref);
    let status_command = "git status --porcelain=v1 --untracked-files=all";
    let diff_command = "git diff --name-status " + base_ref_json + " --";
    let cached_diff_command = "git diff --cached --name-status " + base_ref_json + " --";
    let base_commit_command = "git rev-parse --verify --end-of-options " + base_ref_json + "^{commit}";
    let common = "Role: general-purpose read-only reviewer. Use agent_type general-purpose with capability_mode execute. ";
    common += "This is a read-only review and you must not edit, format, generate, regenerate, write, delete, commit, push, branch, repair, or alter any project file. Do not write tests or documentation. ";
    common += "Use read_file, grep/rg, and terminal tools; inspect the actual current code and call sites before answering. Do not run MongoDB integration tests or browser E2E tests. ";
    common += "Before every conclusion run exactly: ";
    common += status_command;
    common += ", git branch --show-current, ";
    common += diff_command;
    common += ", ";
    common += cached_diff_command;
    common += ", and ";
    common += "git diff --cached --name-status --";
    common += ". Prove the base commit with ";
    common += base_commit_command;
    common += ". ";
    common += "Read every path reported as ??, including all untracked files, and classify every added, modified, deleted, or renamed path against owns, ownsWithin symbol scope, creates, deletes, forbids, frozen-file restrictions, and generated-file provenance. Never rely only on files_changed or an implementation summary. ";
    common += "Re-read docs/dev-plan/_meta.json, docs/dev-plan/conventions.md, docs/dev-plan/approval-workflow.md, the exact planDoc, and relevant repository instructions. ";
    common += "The dependency proof must use exact metadata dependency branch tips and treat stage dependsOnGroups plus project dependencyGroups as one complete dependency-group contract; prove each dependsOn stage and every referenced group member is an ancestor of both the resolved base_ref commit and HEAD. Missing refs, partial cherry-picks, similar contents, or mere branch existence are not proof. ";
    common += "Requested stage plan: ";
    common += json_encode(plan);
    common += ". Implementation and repair evidence: ";
    common += json_encode(implementation_evidence);
    common += ". Base ref: ";
    common += base_ref_json;
    common += ". ";
    common += panel_note;
    common += " Return concrete file/line or command evidence. MongoDB integration-test execution and browser E2E execution are outside this workflow and must not be required for approval. ";

    let contract_prompt = common;
    contract_prompt += "Reviewer kind is contract. Review contract and business semantics, architecture, security, authorization, DataScope timing, transaction/CAS/idempotency behavior, BPM/ERP dependency direction, failure-closed behavior, strong-typed actions, no fallback/double-write/Noop, no second definition source, and the exact stage plan. Check actual code, tests, and call sites. Set approved=false with concrete findings for any violation; an approved=false result without findings is unusable. Do not edit.";
    jobs.push(#{
        label: label_prefix + "-contract-architecture-security-correctness",
        agent_type: "general-purpose",
        capability_mode: "execute",
        prompt: contract_prompt,
        output_schema: review_schema,
    });

    let ownership_prompt = common;
    ownership_prompt += "Reviewer kind is ownership. Return reviewer_kind exactly \"ownership\". Review the complete status and both base/cached name-status lists item by item against raw _meta.json and conventions.md: exact stages[] or perDocumentTypeStages[].p3/.p4 identity, branch, dependsOn, dependencyGroups, owns, ownsWithin symbol scope, creates, deletes, forbids, frozen-file restrictions, and generated-file provenance. Read every ?? file. Any path outside exact ownership, unregistered deletion/creation, forbidden/frozen/generated violation, or dependency boundary mismatch is a blocking finding. Do not edit.";
    jobs.push(#{
        label: label_prefix + "-ownership-scope",
        agent_type: "general-purpose",
        capability_mode: "execute",
        prompt: ownership_prompt,
        output_schema: review_schema,
    });

    let test_prompt = common;
    test_prompt += "Reviewer kind is tests. Review actual changed code and non-excluded tests for happy paths, failures, boundaries, regressions, call-site omissions, generated-file drift, targeted command evidence, applicable non-excluded merge gates, and exact non-excluded acceptance results. Do not run or require MongoDB integration tests or browser E2E tests. Set tests_sufficient=false with concrete findings for gaps in the remaining unit, service, API-contract, static-analysis, build, or script coverage. Do not edit or repair.";
    jobs.push(#{
        label: label_prefix + "-tests-regression",
        agent_type: "general-purpose",
        capability_mode: "execute",
        prompt: test_prompt,
        output_schema: review_schema,
    });

    jobs
}

let args_is_map = false;
if args != () {
    if type_of(args) == "map" {
        args_is_map = true;
    }
}
let smoke = false;
if args_is_map {
    if args.smoke != () {
        if type_of(args.smoke) == "bool" {
            smoke = args.smoke;
        }
    }
}
if smoke == true {
    complete(#{ status: "compile-only", summary: "validate_only 只覆盖编译路径；未调用任何 agent。" });
}

let stage_id = ();
let invalid_base_ref = false;
let base_ref = "main";
if args_is_map {
    if args.stage_id != () {
        stage_id = args.stage_id;
    }
    if args.base_ref != () {
        if type_of(args.base_ref) == "string" {
            base_ref = args.base_ref;
        } else {
            invalid_base_ref = true;
        }
    }
}
if !safe_base_ref(base_ref) {
    invalid_base_ref = true;
}

let invalid_stage = false;
if stage_id == () {
    invalid_stage = true;
} else if type_of(stage_id) != "string" {
    invalid_stage = true;
} else {
    if stage_id == "" || stage_id.contains("..") || stage_id.contains("/") || stage_id.contains("\\") || stage_id.contains(" ") || stage_id.contains("\n") || stage_id.contains("\r") || stage_id.contains("\t") {
        invalid_stage = true;
    }
}
if invalid_stage || invalid_base_ref {
    let invalid_report = "# implement-approval-workflow-stage\n\nStatus: blocked before agent calls.\n\n";
    invalid_report += "## Requested stage\n\n";
    if stage_id == () {
        invalid_report += "- stage_id: missing\n";
    } else {
        invalid_report += "- stage_id: " + json_encode(stage_id) + "\n";
    }
    invalid_report += "- base_ref: " + json_encode(base_ref) + "\n";
    invalid_report += "- reason: stage_id is required and must be an exact safe metadata identifier; base_ref must be an exact safe Git reference with no option prefix, revision expression, whitespace, shell metacharacter, or Git revision danger character.\n";
    let invalid_path = write_scratch_file("implement-approval-workflow-stage-report.md", invalid_report);
    complete(#{ status: "blocked", path: invalid_path, stage_id: if stage_id == () { "" } else { stage_id }, summary: "Missing or unsafe stage_id/base_ref; no agent was called and no project file was changed." });
}

let stage_is_p6_pilot = stage_id == "P6-PILOT";
let stage_is_p6_final = stage_id == "P6-FINAL";
let stage_is_p6 = stage_is_p6_pilot || stage_is_p6_final;
let base_ref_json = json_encode(base_ref);
let base_commit_command = "git rev-parse --verify --end-of-options " + base_ref_json + "^{commit}";
let base_diff_command = "git diff --name-status " + base_ref_json + " --";
let cached_base_diff_command = "git diff --cached --name-status " + base_ref_json + " --";

phase("Scope");
let scope_prompt = "Role: general-purpose read-only scope and implementation-plan agent. Use agent_type general-purpose with capability_mode execute. ";
scope_prompt += "This is a blocking read-only scope proof. You must not edit, format, generate, regenerate, write, delete, commit, push, branch, repair, or alter any project file, test, document, generated file, or workflow. ";
scope_prompt += "Use read_file, grep/rg, and terminal evidence. First read docs/dev-plan/README.md, docs/dev-plan/conventions.md, docs/dev-plan/domains.md, docs/dev-plan/_meta.json, docs/dev-plan/approval-workflow.md, docs/approval-workflow-implementation-plan/README.md, docs/approval-workflow-contract.md, docs/erp-phase-1.md, docs/erp-phase-2.md, and docs/approval-workflow-implementation-plan/11-integration-and-acceptance.md. ";
scope_prompt += "Then read the exact planDoc selected from metadata, including the relevant plan file for stages[] or the default plan file for a perDocumentTypeStages[].p3/.p4 object. Resolve the requested stage_id exactly from _meta.json: one exact object in projects.approval-workflow.stages[] or one exact p3/p4 object nested under projects.approval-workflow.perDocumentTypeStages[]. Never invent, merge, rename, or infer a stage model. ";
scope_prompt += "Return the exact id, source, normalized stage_kind, documentType, approvalRequirement, branch, dependsOn, dependencyGroups, owns, ownsWithin, creates, deletes, forbids, acceptance, implementation steps, and complete applicable backend, frontend, and docs merge-gate lists. stage_kind is a normalized script field, not a metadata source field: use exactly top_level for stages[], p3 for perDocumentTypeStages[].p3, and p4 for perDocumentTypeStages[].p4. stages[] may return documentType and approvalRequirement as empty strings; only perDocumentTypeStages[].p3/.p4 must return both as non-empty strings. ";
scope_prompt += "Before any conclusion run: git status --porcelain=v1 --untracked-files=all; ";
scope_prompt += base_diff_command;
scope_prompt += "; ";
scope_prompt += cached_base_diff_command;
scope_prompt += "; git diff --cached --name-status --; and ";
scope_prompt += base_commit_command;
scope_prompt += ". Use the safe quoted base reference exactly as shown and use -- for every path list. Read every path shown as ??, even if it is not in files_changed, and classify every added, modified, deleted, or renamed path item by item against owns, ownsWithin symbol scope, creates, deletes, forbids, frozen-file restrictions, and generated-file provenance. ";
scope_prompt += "The startup worktree must be empty and the current branch must equal the exact metadata branch. Return at least one item in owns or ownsWithin; an empty creates, deletes, or forbids list is valid when metadata has no such entries. ";
scope_prompt += "Treat stage dependsOnGroups and project dependencyGroups as the same complete dependency-group contract. For every dependsOn stage and every member of every referenced dependencyGroups group, resolve the exact dependency branch from raw metadata with git rev-parse --verify --end-of-options <safe-dependency-branch>^{commit}; prove the dependency tip is an ancestor of both the resolved base_ref commit and HEAD with git merge-base --is-ancestor. A missing ref, partial cherry-pick, similar contents, or mere branch existence is failure. Do not accept any weaker dependency claim. ";
scope_prompt += "Verify DOC gates, P0 prerequisites, pilot gates, frozen files, generated-file rules, and every plan precondition. Set dependencies_satisfied=false for any unproven dependency. Keep acceptance and merge-gate lists faithful to their sources, but MongoDB integration tests and browser E2E tests are excluded from execution by this workflow: do not include them in baseline_commands or implementation validation, and record their exclusion in evidence. ";
scope_prompt += "For P6-PILOT and P6-FINAL, include these non-excluded backend commands explicitly: cargo fmt --all -- --check, cargo check --workspace, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace, cargo test -p bpm --lib, and ./scripts/check-bpm-boundaries.sh. Also include the complete non-excluded frontend gates and npm run openapi:lint. Return non-empty evidence for every conclusion. When there is no blocker, return blockers: [] exactly; do not use a placeholder blocker string. ";
scope_prompt += "Return scope_safe=true only when all metadata, branch, dependency, clean-start, precondition, ownership, and gate proofs are concrete. Requested stage_id: ";
scope_prompt += json_encode(stage_id);
scope_prompt += ". Requested base_ref: ";
scope_prompt += base_ref_json;
let scope = agent(scope_prompt, #{
    label: "general:approval-stage-scope",
    agent_type: "general-purpose",
    capability_mode: "execute",
    output_schema: scope_plan_schema,
});

let scope_out = ();
if scope != () {
    if scope.success == true {
        if scope.output != () {
            if type_of(scope.output) == "map" {
                scope_out = scope.output;
            }
        }
    }
}

let scope_details = "No usable scope result was returned.";
if scope_out != () {
    scope_details = json_encode(scope_out);
}
let scope_ok = false;
if scope_out != () {
    if type_of(scope_out) == "map" {
        let source_ok = scope_out.stage_source == "stages[]" || scope_out.stage_source == "perDocumentTypeStages[].p3" || scope_out.stage_source == "perDocumentTypeStages[].p4";
        let stage_kind_ok = false;
        if scope_out.stage_source == "stages[]" {
            stage_kind_ok = scope_out.stage_kind == "top_level";
        } else if scope_out.stage_source == "perDocumentTypeStages[].p3" {
            stage_kind_ok = scope_out.stage_kind == "p3";
        } else if scope_out.stage_source == "perDocumentTypeStages[].p4" {
            stage_kind_ok = scope_out.stage_kind == "p4";
        }
        let document_fields_ok = type_of(scope_out.document_type) == "string" && type_of(scope_out.approval_requirement) == "string";
        if scope_out.stage_source == "perDocumentTypeStages[].p3" || scope_out.stage_source == "perDocumentTypeStages[].p4" {
            document_fields_ok = is_nonempty_text(scope_out.document_type) && is_nonempty_text(scope_out.approval_requirement);
        }
        let identity_ok = is_nonempty_text(scope_out.stage_id) && is_nonempty_text(scope_out.stage_source) && stage_kind_ok && document_fields_ok && is_nonempty_text(scope_out.plan_doc) && is_nonempty_text(scope_out.branch) && scope_out.stage_id == stage_id;
        let owns_declared = false;
        if is_text_array(scope_out.owns) {
            if scope_out.owns.len() > 0 {
                owns_declared = true;
            }
        }
        if is_text_array(scope_out.owns_within) {
            if scope_out.owns_within.len() > 0 {
                owns_declared = true;
            }
        }
        let ownership_shape_ok = text_array_items_nonempty(scope_out.depends_on) && text_array_items_nonempty(scope_out.dependency_groups) && text_array_items_nonempty(scope_out.owns) && owns_within_items_valid(scope_out.owns_within) && text_array_items_nonempty(scope_out.creates) && text_array_items_nonempty(scope_out.deletes) && text_array_items_nonempty(scope_out.forbids);
        let gate_shape_ok = is_nonempty_text_array(scope_out.acceptance) && text_array_items_nonempty(scope_out.acceptance) && text_array_items_nonempty(scope_out.backend_merge_gates) && text_array_items_nonempty(scope_out.frontend_merge_gates) && text_array_items_nonempty(scope_out.docs_merge_gates) && is_nonempty_text_array(scope_out.baseline_commands) && is_nonempty_text_array(scope_out.implementation_steps);
        let evidence_shape_ok = text_array_items_nonempty(scope_out.blockers) && is_nonempty_text_array(scope_out.evidence);
        let flags_ok = scope_out.metadata_checked == true && scope_out.stage_exists == true && scope_out.dependencies_satisfied == true && scope_out.branch_matches == true && scope_out.clean_start == true && scope_out.preconditions_clear == true && scope_out.ownership_complete == true && scope_out.gates_complete == true && scope_out.scope_safe == true;
        let blockers_clear = is_text_array(scope_out.blockers) && scope_out.blockers.len() == 0;
        scope_ok = source_ok && identity_ok && owns_declared && ownership_shape_ok && gate_shape_ok && evidence_shape_ok && blockers_clear && flags_ok;
    }
}

let stage_plan = scope_out;
let baseline_commands = [];
if stage_plan != () {
    if type_of(stage_plan) == "map" {
        baseline_commands = without_excluded_external_tests(stage_plan.baseline_commands);
        scope_details = json_encode(stage_plan);
    }
}

let blocked = false;
let blocked_reason = "";
if !scope_ok {
    blocked = true;
    blocked_reason = "Scope/plan proof failed or the requested stage is unsafe. No baseline or write-capable agent was started.";
}
if !blocked {
    if baseline_commands.len() == 0 {
        blocked = true;
        blocked_reason = "No non-excluded baseline command remained after removing MongoDB integration and browser E2E tests.";
    }
}
if !blocked {
    let scope_budget = budget();
    let minimum_remaining = 7;
    if scope_budget.remaining < minimum_remaining {
        blocked = true;
        blocked_reason = "Insufficient remaining agent budget after scope; the ordinary path requires at least " + minimum_remaining.to_string() + " remaining agents.";
    }
}

let backend_surface = false;
let frontend_surface = false;
let docs_surface = false;
let expected_commands = [];
let expected_acceptance_checks = [];
if !blocked {
    backend_surface = scope_has_surface(stage_plan, "backend");
    frontend_surface = scope_has_surface(stage_plan, "erp-client");
    docs_surface = scope_has_surface(stage_plan, "docs");

    for acceptance_item in stage_plan.acceptance {
        if !excluded_external_test(acceptance_item) {
            expected_acceptance_checks = add_unique_text(expected_acceptance_checks, acceptance_item);
            if executable_acceptance_item(acceptance_item) {
                expected_commands = add_unique_text(expected_commands, acceptance_item);
            }
        }
    }
    for gate in stage_plan.backend_merge_gates {
        if !excluded_external_test(gate) {
            if executable_acceptance_item(gate) {
                expected_commands = add_unique_text(expected_commands, gate);
            } else {
                expected_acceptance_checks = add_unique_text(expected_acceptance_checks, gate);
            }
        }
    }
    for gate in stage_plan.frontend_merge_gates {
        if !excluded_external_test(gate) {
            if executable_acceptance_item(gate) {
                expected_commands = add_unique_text(expected_commands, gate);
            } else {
                expected_acceptance_checks = add_unique_text(expected_acceptance_checks, gate);
            }
        }
    }
    for gate in stage_plan.docs_merge_gates {
        if !excluded_external_test(gate) {
            if executable_acceptance_item(gate) {
                expected_commands = add_unique_text(expected_commands, gate);
            } else {
                expected_acceptance_checks = add_unique_text(expected_acceptance_checks, gate);
            }
        }
    }

    let canonical_backend = [
        "cargo fmt --all -- --check",
        "cargo check --workspace",
        "cargo clippy --workspace --all-targets --all-features -- -D warnings",
        "cargo test --workspace",
    ];
    let canonical_frontend = [
        "npm run format:check",
        "npm run lint",
        "npm run test",
        "npm run test:node",
        "npm exec -- tsc --noEmit",
        "npm run build",
    ];
    let canonical_docs_commands = [
        "git diff --check",
        "jq empty docs/dev-plan/_meta.json",
    ];
    let canonical_docs_manual = [
        "相对 Markdown 链接检查",
        "阶段 10 文档语义扫描",
    ];
    if backend_surface {
        for command in canonical_backend {
            expected_commands = add_unique_text(expected_commands, command);
        }
    }
    if frontend_surface {
        for command in canonical_frontend {
            expected_commands = add_unique_text(expected_commands, command);
        }
    }
    if docs_surface {
        for command in canonical_docs_commands {
            expected_commands = add_unique_text(expected_commands, command);
        }
        for check in canonical_docs_manual {
            expected_acceptance_checks = add_unique_text(expected_acceptance_checks, check);
        }
    }

    if stage_is_p6 {
        let forced_backend = [
            "cargo fmt --all -- --check",
            "cargo check --workspace",
            "cargo clippy --workspace --all-targets --all-features -- -D warnings",
            "cargo test --workspace",
            "cargo test -p bpm --lib",
            "./scripts/check-bpm-boundaries.sh",
        ];
        let forced_frontend = [
            "npm run format:check",
            "npm run lint",
            "npm run test",
            "npm run test:node",
            "npm exec -- tsc --noEmit",
            "npm run build",
        ];
        for command in forced_backend {
            expected_commands = add_unique_text(expected_commands, command);
        }
        for command in forced_frontend {
            expected_commands = add_unique_text(expected_commands, command);
        }
        expected_commands = add_unique_text(expected_commands, "npm run openapi:lint");
    }
    if expected_commands.len() == 0 {
        blocked = true;
        blocked_reason = "No applicable executable merge gate or acceptance command was resolved by scope.";
    }
}

let baseline_out = ();
let implementation_out = ();
let implementation_summaries = [];
let review_summaries = [];
let repair_summaries = [];
let review_round = 0;
let review_passed = false;
let verification_out = ();
let quality_repair_used = false;
let final_review_out = ();
let final_review_passed = false;

if !blocked {
    phase("Baseline");
    let baseline_budget = budget();
    if baseline_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "Insufficient remaining agent budget before the general-purpose baseline.";
    } else {
        let baseline_prompt = "Role: general-purpose execute baseline agent. Use agent_type general-purpose with capability_mode execute. This command is not a review and must not edit, format, generate, regenerate, write, delete, commit, push, branch, repair, or alter source, tests, docs, generated files, or workflows. ";
        baseline_prompt += "Read the exact stage plan and re-check raw metadata/conventions. Run git status --porcelain=v1 --untracked-files=all and git branch --show-current first. Also run ";
        baseline_prompt += base_diff_command;
        baseline_prompt += ", ";
        baseline_prompt += cached_base_diff_command;
        baseline_prompt += ", git diff --cached --name-status --, and ";
        baseline_prompt += base_commit_command;
        baseline_prompt += ". Read every ?? file and confirm the startup tree is clean; do not trust files_changed summaries. ";
        baseline_prompt += "Run every read-only baseline command listed below in its correct repository directory. MongoDB integration tests and browser E2E tests have been removed from this list and must not be run or added back. Do not repair failures, do not skip a remaining command, and do not call a failing pre-existing command a pass. Return exactly one command result for every filtered baseline command and no other command result; each result must preserve the exact command text and include concise evidence. Record blocked_reason as non-empty none when clear. ";
        baseline_prompt += "Stage plan: ";
        baseline_prompt += json_encode(stage_plan);
        baseline_prompt += ". Filtered baseline commands: ";
        baseline_prompt += json_encode(baseline_commands);
        baseline_prompt += ". Expected branch: ";
        baseline_prompt += json_encode(stage_plan.branch);
        let baseline = agent(baseline_prompt, #{
            label: "general:approval-stage-baseline",
            agent_type: "general-purpose",
            capability_mode: "execute",
            output_schema: baseline_schema,
        });
        if baseline != () {
            if baseline.success == true {
                if baseline.output != () {
                    if type_of(baseline.output) == "map" {
                        baseline_out = baseline.output;
                    }
                }
            }
        }
        let baseline_ok = false;
        if baseline_out != () {
            let evidence_ok = is_nonempty_text_array(baseline_out.evidence);
            let current_branch_ok = is_nonempty_text(baseline_out.current_branch) && baseline_out.current_branch == stage_plan.branch;
            let command_results_ok = command_results_exact(baseline_out.commands, baseline_commands);
            let flags_ok = baseline_out.baseline_green == true && baseline_out.git_status_clean == true && baseline_out.branch_matches == true && baseline_out.stage_plan_consistent == true;
            baseline_ok = evidence_ok && current_branch_ok && command_results_ok && flags_ok && is_nonempty_text(baseline_out.blocked_reason);
        }
        if !baseline_ok {
            blocked = true;
            blocked_reason = "The general-purpose execute baseline was not green, the startup tree/branch was unsafe, or baseline evidence was incomplete. No implementation was attempted.";
        }
    }
}

if !blocked {
    phase("Implement");
    let implementation_budget = budget();
    if implementation_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "Insufficient remaining agent budget before implementation.";
    } else {
        let implementation_prompt = "Role: general-purpose implementation agent. Use agent_type general-purpose with capability_mode all. This is the write-capable implementation role: every project-file addition, modification, deletion, test code change, documentation edit, review repair, and quality-gate repair must be done by general-purpose/all; execute-only reviewers must not edit. ";
        implementation_prompt += "Read backend/AGENTS.md or its repository instruction indirection, erp-client/AGENTS.md and erp-client/CLAUDE.md when frontend paths apply, relevant coding standards, docs/dev-plan/conventions.md, docs/dev-plan/domains.md, docs/dev-plan/_meta.json, docs/dev-plan/approval-workflow.md, docs/approval-workflow-contract.md, docs/erp-phase-1.md, docs/erp-phase-2.md, and the exact planDoc before editing. ";
        implementation_prompt += "Before editing, run git status --porcelain=v1 --untracked-files=all, ";
        implementation_prompt += base_diff_command;
        implementation_prompt += ", ";
        implementation_prompt += cached_base_diff_command;
        implementation_prompt += ", and ";
        implementation_prompt += base_commit_command;
        implementation_prompt += ". Read every ?? file and classify all actual paths against owns, ownsWithin, creates, deletes, forbids, frozen files, and generated-file rules. Use full-repository symbol and call-site search before changing an entry point. ";
        implementation_prompt += "Implement only the exact metadata stage plan below; respect owns and ownsWithin symbol scopes and do not touch frozen or unowned files. Do not invent another stage model, compatibility path, fallback, double-write, Noop success, default assignee, or undocumented business rule. ";
        implementation_prompt += "All new or modified Rust non-generated methods/functions need the repository multi-line /// documentation rules; production Rust must not use forbidden unwrap/expect/panic. For frontend paths use Client Components for business UI, TanStack Query for all requests, useAppForm plus Zod for forms, JSDoc for new functions/hooks/components, the glossary for user-visible wording, and no BPM-internal types in the client. Generated files must be produced only by the registered generator, never hand-edited. ";
        implementation_prompt += "Run only the non-excluded stage targeted commands after edits. Do not run MongoDB integration tests or browser E2E tests, and do not claim either as validation evidence. Do not create a branch, commit, push, open a PR, or modify this workflow file. If metadata, dependency, ownership, or business semantics cannot be implemented safely, stop and return completed=false rather than guessing. ";
        implementation_prompt += "Exact stage plan: ";
        implementation_prompt += json_encode(stage_plan);
        let implementation = agent(implementation_prompt, #{
            label: "general:approval-stage-implementation",
            agent_type: "general-purpose",
            capability_mode: "all",
            output_schema: implementation_schema,
        });
        if implementation != () {
            if implementation.success == true {
                if implementation.output != () {
                    if type_of(implementation.output) == "map" {
                        implementation_out = implementation.output;
                    }
                }
            }
        }
        if implementation_out != () {
            implementation_summaries.push(implementation_out);
        }
        if !implementation_result_valid(implementation_out) {
            blocked = true;
            blocked_reason = "The general-purpose implementation agent failed, reported an unsafe scope, or returned incomplete implementation evidence.";
        }
    }
}

while !blocked && !review_passed && review_round <= 2 {
    phase("Review");
    let expected_reviewers = 3;
    let review_jobs = build_review_jobs(stage_plan, base_ref, implementation_summaries, review_schema, "general:approval-review-round-" + review_round.to_string(), "This is the ordinary independent review panel.");
    let review_budget = budget();
    if review_budget.remaining < expected_reviewers {
        blocked = true;
        blocked_reason = "Insufficient remaining agent budget before review panel round " + review_round.to_string() + ".";
    } else {
        let review_results = parallel(review_jobs);
        let panel_assessment = assess_review_panel(review_results, expected_reviewers);
        for panel_summary in panel_assessment.summaries {
            review_summaries.push(panel_summary);
        }
        if panel_assessment.usable != true {
            blocked = true;
            blocked_reason = "A required general-purpose review agent failed or lacked evidence: " + json_encode(panel_assessment.reasons);
        } else if panel_assessment.passed == true {
            review_passed = true;
        } else if review_round >= 2 {
            blocked = true;
            blocked_reason = "The implementation did not pass the required general-purpose review panel within the two allowed review-repair rounds. Findings: " + json_encode(panel_assessment.findings);
        } else {
            phase("Repair");
            let repair_budget = budget();
            if repair_budget.remaining < 1 {
                blocked = true;
                blocked_reason = "Insufficient remaining agent budget before review repair round " + review_round.to_string() + ".";
            } else {
                let repair_prompt = "Role: general-purpose review-repair agent. Use agent_type general-purpose with capability_mode all. All project-file additions, modifications, deletions, test writing, documentation editing, review repairs, and quality-gate repairs must be done only by general-purpose/all; this is the write-capable repair role and it is not a review. Execute-only reviewers must not edit. ";
                repair_prompt += "Read the actual current diff and code, raw docs/dev-plan/_meta.json, conventions.md, approval-workflow.md, the exact planDoc, and relevant instructions before editing. Run git status --porcelain=v1 --untracked-files=all, ";
                repair_prompt += base_diff_command;
                repair_prompt += ", ";
                repair_prompt += cached_base_diff_command;
                repair_prompt += ", and ";
                repair_prompt += base_commit_command;
                repair_prompt += ". Read every ?? file and compare every actual path against owns, ownsWithin, creates, deletes, forbids, frozen, and generated-file rules. ";
                repair_prompt += "Repair only concrete findings below that are valid for the exact metadata stage. Do not broaden scope, touch unowned/forbidden/frozen/generated paths, invent compatibility behavior, weaken tests, or change unrelated pre-existing code. Use the registered generator for generated files. Do not commit, push, branch, or modify the workflow. Run only non-excluded targeted commands after the repair; do not run MongoDB integration tests or browser E2E tests. If a finding cannot be safely repaired within owns/ownsWithin, return completed=false rather than guessing. ";
                repair_prompt += "Exact stage plan: ";
                repair_prompt += json_encode(stage_plan);
                repair_prompt += ". Review findings: ";
                repair_prompt += json_encode(panel_assessment.findings);
                repair_prompt += ". Review evidence summary: ";
                repair_prompt += json_encode(review_summaries);
                let repair = agent(repair_prompt, #{
                    label: "general:approval-stage-review-repair-" + review_round.to_string(),
                    agent_type: "general-purpose",
                    capability_mode: "all",
                    output_schema: implementation_schema,
                });
                let repair_out = ();
                if repair != () {
                    if repair.success == true {
                        if repair.output != () {
                            if type_of(repair.output) == "map" {
                                repair_out = repair.output;
                            }
                        }
                    }
                }
                if repair_out != () {
                    repair_summaries.push(repair_out);
                    implementation_summaries.push(repair_out);
                }
                if !implementation_result_valid(repair_out) {
                    blocked = true;
                    blocked_reason = "A general-purpose review repair failed or returned unsafe/incomplete evidence.";
                } else {
                    review_round += 1;
                }
            }
        }
    }
}
if !review_passed && !blocked {
    blocked = true;
    blocked_reason = "The review loop ended without a passing general-purpose review panel.";
}

if !blocked {
    phase("Verify");
    let verification_prompt = "Role: general-purpose execute quality-gate agent. Use agent_type general-purpose with capability_mode execute. This is command execution, not review, and you must not edit, format, generate, regenerate, write, delete, commit, push, branch, repair, or alter files. All project-file changes and gate repairs belong exclusively to general-purpose/all. ";
    verification_prompt += "Read the raw docs/dev-plan/_meta.json, docs/dev-plan/conventions.md, docs/dev-plan/approval-workflow.md, docs/approval-workflow-contract.md, docs/erp-phase-1.md, docs/erp-phase-2.md, exact planDoc, repository instructions, and actual current code. Before any conclusion run git status --porcelain=v1 --untracked-files=all, git branch --show-current, ";
    verification_prompt += base_diff_command;
    verification_prompt += ", ";
    verification_prompt += cached_base_diff_command;
    verification_prompt += ", git diff --cached --name-status --, and ";
    verification_prompt += base_commit_command;
    verification_prompt += ". Read every ?? file and compare all added, modified, deleted, and renamed paths item by item with owns, ownsWithin, creates, deletes, forbids, frozen files, and generated-file provenance; do not trust files_changed. ";
    verification_prompt += "Confirm the final scope still matches the exact metadata stage and dependency proof. Run the exact expected command set below, in the correct repository directories, with no substitutions, aliases, partial commands, or duplicates. ";
    verification_prompt += "Return one command result for every expected command and no other command result; every result needs a non-empty command, summary, and evidence. Return acceptance_results with exactly one entry per expected_acceptance_checks item, in the same order and exact item text; expected_acceptance_checks is non-excluded stage acceptance followed by every non-executable non-excluded merge gate, deduplicated. Every acceptance entry must pass and include manual evidence where applicable. Do not represent DOC manual relative-Markdown-link or Stage-10 semantic scans as shell commands. ";
    verification_prompt += "MongoDB integration tests and browser E2E tests are excluded. Do not run them, add them to results, require their environments, or claim them as acceptance evidence. Set environment_blocked=true only when a command that remains in the exact expected command set cannot run in its required environment. ";
    verification_prompt += "Exact stage plan: ";
    verification_prompt += json_encode(stage_plan);
    verification_prompt += ". Expected command set: ";
    verification_prompt += json_encode(expected_commands);
    verification_prompt += ". Expected acceptance checks: ";
    verification_prompt += json_encode(expected_acceptance_checks);
    if stage_is_p6 {
        verification_prompt += ". P6 non-excluded executable backend gates are explicitly cargo fmt --all -- --check, cargo check --workspace, cargo clippy --workspace --all-targets --all-features -- -D warnings, cargo test --workspace, cargo test -p bpm --lib, and ./scripts/check-bpm-boundaries.sh; non-excluded frontend gates are npm run format:check, npm run lint, npm run test, npm run test:node, npm exec -- tsc --noEmit, and npm run build; also run npm run openapi:lint.";
    }
    let verification_budget = budget();
    if verification_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "Insufficient remaining agent budget before quality-gate verification.";
    } else {
        let verification = agent(verification_prompt, #{
            label: "general:approval-stage-quality-gates",
            agent_type: "general-purpose",
            capability_mode: "execute",
            output_schema: verification_schema,
        });
        if verification != () {
            if verification.success == true {
                if verification.output != () {
                    if type_of(verification.output) == "map" {
                        verification_out = verification.output;
                    }
                }
            }
        }
        let verification_assessment = assess_verification(verification_out, expected_commands, expected_acceptance_checks);
        if verification_assessment.usable != true {
            blocked = true;
            blocked_reason = "The general-purpose quality-gate agent failed or returned incomplete gate evidence.";
        } else if verification_assessment.environment_blocked == true {
            blocked = true;
            blocked_reason = "A required non-excluded quality gate is environmentally blocked; the workflow fails closed and will not claim success.";
        } else if verification_assessment.passed != true {
            phase("Repair");
            let quality_repair_budget = budget();
            if quality_repair_budget.remaining < 1 {
                blocked = true;
                blocked_reason = "Insufficient remaining agent budget before quality-gate repair.";
            } else {
                let quality_repair_prompt = "Role: general-purpose quality-gate repair agent. Use agent_type general-purpose with capability_mode all. All project-file changes, including fixes for quality-gate failures, belong exclusively to general-purpose/all; this is not a review and execute-only agents must not edit. ";
                quality_repair_prompt += "Inspect the actual current diff, raw metadata/conventions, exact planDoc, repository instructions, and the complete failed gate evidence. Before editing run git status --porcelain=v1 --untracked-files=all, ";
                quality_repair_prompt += base_diff_command;
                quality_repair_prompt += ", ";
                quality_repair_prompt += cached_base_diff_command;
                quality_repair_prompt += ", and ";
                quality_repair_prompt += base_commit_command;
                quality_repair_prompt += ". Read every ?? file and compare every actual path item by item against owns, ownsWithin, creates, deletes, forbids, frozen, and generated-file rules. ";
                quality_repair_prompt += "Repair only failures introduced by this stage and only within owns/ownsWithin. Do not mask pre-existing or environmental failures, weaken tests, skip non-excluded gates, hand-edit generated files, alter forbidden/frozen/unowned paths, commit, push, branch, or modify the workflow. Do not run MongoDB integration tests or browser E2E tests. If the failure is pre-existing, environmental, or cannot be safely repaired, return completed=false. Run the failing non-excluded targeted commands after the repair. ";
                quality_repair_prompt += "Exact stage plan: ";
                quality_repair_prompt += json_encode(stage_plan);
                quality_repair_prompt += ". Expected command set: ";
                quality_repair_prompt += json_encode(expected_commands);
                quality_repair_prompt += ". Failed quality evidence: ";
                quality_repair_prompt += json_encode(verification_out);
                let quality_repair = agent(quality_repair_prompt, #{
                    label: "general:approval-stage-quality-repair",
                    agent_type: "general-purpose",
                    capability_mode: "all",
                    output_schema: implementation_schema,
                });
                let quality_repair_out = ();
                if quality_repair != () {
                    if quality_repair.success == true {
                        if quality_repair.output != () {
                            if type_of(quality_repair.output) == "map" {
                                quality_repair_out = quality_repair.output;
                            }
                        }
                    }
                }
                if quality_repair_out != () {
                    repair_summaries.push(quality_repair_out);
                    implementation_summaries.push(quality_repair_out);
                }
                if !implementation_result_valid(quality_repair_out) {
                    blocked = true;
                    blocked_reason = "The single allowed general-purpose quality-gate repair failed or returned unsafe/incomplete evidence.";
                } else {
                    quality_repair_used = true;
                    phase("Verify");
                    let rerun_budget = budget();
                    if rerun_budget.remaining < 1 {
                        blocked = true;
                        blocked_reason = "Insufficient remaining agent budget before the mandatory quality-gate rerun.";
                    } else {
                        let verification_rerun_prompt = verification_prompt;
                        verification_rerun_prompt += " This is the mandatory full rerun after the general-purpose quality repair. Rerun every expected command and every acceptance result from scratch, return the exact same command-set and acceptance-set shape, and do not rely on the first result.";
                        let verification_rerun = agent(verification_rerun_prompt, #{
                            label: "general:approval-stage-quality-gates-rerun",
                            agent_type: "general-purpose",
                            capability_mode: "execute",
                            output_schema: verification_schema,
                        });
                        let verification_rerun_out = ();
                        if verification_rerun != () {
                            if verification_rerun.success == true {
                                if verification_rerun.output != () {
                                    if type_of(verification_rerun.output) == "map" {
                                        verification_rerun_out = verification_rerun.output;
                                    }
                                }
                            }
                        }
                        verification_out = verification_rerun_out;
                        let rerun_assessment = assess_verification(verification_rerun_out, expected_commands, expected_acceptance_checks);
                        if rerun_assessment.usable != true || rerun_assessment.environment_blocked == true || rerun_assessment.passed != true {
                            blocked = true;
                            blocked_reason = "The complete quality gates did not pass after the single allowed general-purpose repair, or the rerun lacked exact command and acceptance evidence.";
                        } else {
                            phase("Review");
                            let post_review_expected = 3;
                            let post_review_jobs = build_review_jobs(stage_plan, base_ref, implementation_summaries, review_schema, "general:approval-post-quality-review", "This is the mandatory post-quality repair review. Any failed proof or non-empty findings block immediately; no further repair is allowed after this panel.");
                            let post_review_budget = budget();
                            if post_review_budget.remaining < post_review_expected {
                                blocked = true;
                                blocked_reason = "Insufficient remaining agent budget before the mandatory post-quality review panel.";
                            } else {
                                let post_review_results = parallel(post_review_jobs);
                                let post_panel_assessment = assess_review_panel(post_review_results, post_review_expected);
                                for post_summary in post_panel_assessment.summaries {
                                    review_summaries.push(post_summary);
                                }
                                if post_panel_assessment.usable != true || post_panel_assessment.passed != true {
                                    blocked = true;
                                    blocked_reason = "The mandatory general-purpose post-quality review failed after repair; the workflow is blocked. Findings: " + json_encode(post_panel_assessment.findings) + ". Reasons: " + json_encode(post_panel_assessment.reasons);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

if !blocked {
    phase("Final Review");
    let final_review_prompt = "Role: general-purpose read-only final acceptance reviewer. Use agent_type general-purpose with capability_mode execute. All project-file additions, modifications, deletions, test writing, documentation editing, review repairs, and quality-gate repairs belong exclusively to general-purpose/all; you must not edit, format, generate, regenerate, write, delete, commit, push, branch, repair, or alter any project file. ";
    final_review_prompt += "Inspect the actual complete current diff and status, actual changed code and tests, every untracked ?? file, and raw docs/dev-plan/_meta.json, docs/dev-plan/conventions.md, docs/dev-plan/approval-workflow.md, exact planDoc, repository instructions, baseline, implementation/repair evidence, all review evidence, and complete quality-gate evidence. Before any conclusion run git status --porcelain=v1 --untracked-files=all, git branch --show-current, ";
    final_review_prompt += base_diff_command;
    final_review_prompt += ", ";
    final_review_prompt += cached_base_diff_command;
    final_review_prompt += ", git diff --cached --name-status --, and ";
    final_review_prompt += base_commit_command;
    final_review_prompt += ". Read every ?? file and classify every added, modified, deleted, or renamed path item by item against owns, ownsWithin, creates, deletes, forbids, frozen-file restrictions, and generated-file provenance. Do not approve from files_changed or summaries alone. ";
    final_review_prompt += "Prove every dependsOn stage and every member of stage dependsOnGroups or project dependencyGroups is merged by exact dependency branch-tip ancestry into both base_ref and HEAD; partial cherry-picks, content similarity, and branch existence are not proof. Check exact stage identity and branch, contract and business semantics, architecture/security/correctness, test sufficiency, exact non-excluded command and acceptance evidence, and full non-excluded gate evidence. MongoDB integration tests and browser E2E tests are outside this workflow: do not run them, require them, or treat their absence as a finding. ";
    final_review_prompt += "Reviewer kind must be final, exactly \"final\". Return approved=true only when every required non-excluded proof is concrete and the current diff is safe to merge as this exact metadata stage. Any non-empty findings block; approved=false with empty findings is unusable. Return non-empty evidence and summary. Do not edit.";
    final_review_prompt += " Exact stage plan: ";
    final_review_prompt += json_encode(stage_plan);
    final_review_prompt += ". Implementation and repair evidence: ";
    final_review_prompt += json_encode(implementation_summaries);
    final_review_prompt += ". Review evidence: ";
    final_review_prompt += json_encode(review_summaries);
    final_review_prompt += ". Quality-gate evidence: ";
    final_review_prompt += json_encode(verification_out);
    let final_budget = budget();
    if final_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "Insufficient remaining agent budget before final acceptance review.";
    } else {
        let final_review = agent(final_review_prompt, #{
            label: "general:approval-stage-final-acceptance",
            agent_type: "general-purpose",
            capability_mode: "execute",
            output_schema: review_schema,
        });
        if final_review != () {
            if final_review.success == true {
                if final_review.output != () {
                    if type_of(final_review.output) == "map" {
                        final_review_out = final_review.output;
                    }
                }
            }
        }
        let final_assessment = assess_one_review(final_review_out, "final");
        final_review_passed = final_assessment.usable == true && final_assessment.passed == true;
        if final_assessment.usable != true {
            blocked = true;
            blocked_reason = "The general-purpose final acceptance reviewer failed or returned incomplete evidence: " + final_assessment.reason;
        } else if !final_review_passed {
            blocked = true;
            blocked_reason = "The final general-purpose acceptance review found blocking issues; no unsafe approval was returned. Findings: " + json_encode(final_assessment.findings);
        }
    }
}

phase("Report");
let report_status = "completed";
if blocked || !final_review_passed {
    report_status = "blocked";
}
let verdict_reason = blocked_reason;
if report_status == "completed" {
    verdict_reason = "All exact metadata, ownership, dependency, review, quality-gate, acceptance, and final-review proofs passed for the requested stage.";
}
if !is_nonempty_text(verdict_reason) {
    verdict_reason = "Workflow blocked because a required proof was unavailable.";
}
let report = "# implement-approval-workflow-stage\n\n";
report += "Status: " + report_status + "\n\n";
report += "## Stage\n\n";
report += "- requested stage_id: " + json_encode(stage_id) + "\n";
report += "- base_ref: " + base_ref_json + "\n";
if scope_ok {
    report += "- resolved stage_id: " + json_encode(stage_plan.stage_id) + "\n";
    report += "- stage source: " + json_encode(stage_plan.stage_source) + "\n";
    report += "- metadata branch: " + json_encode(stage_plan.branch) + "\n";
    report += "- planDoc: " + json_encode(stage_plan.plan_doc) + "\n";
    report += "- excluded tests: MongoDB integration and browser E2E\n";
} else {
    report += "- resolved stage: unavailable or unsafe\n";
}
report += "\n## Scope plan\n\n```json\n" + scope_details + "\n```\n\n";
report += "## Baseline\n\n";
if baseline_out == () {
    report += "Not run or no usable result.\n\n";
} else {
    report += "```json\n" + json_encode(baseline_out) + "\n```\n\n";
}
report += "## Implementation and repairs\n\n```json\n" + json_encode(implementation_summaries) + "\n```\n\n";
report += "## Reviews\n\n```json\n" + json_encode(review_summaries) + "\n```\n\n";
report += "## Repair records\n\n```json\n" + json_encode(repair_summaries) + "\n```\n\n";
report += "## Quality gates\n\n";
if verification_out == () {
    report += "Not run or no usable result.\n\n";
} else {
    report += "```json\n" + json_encode(verification_out) + "\n```\n\n";
}
report += "## Excluded tests\n\n";
report += "- MongoDB integration tests were not executed or required.\n";
report += "- Browser E2E tests were not executed or required.\n\n";
report += "## Final review\n\n```json\n" + json_encode(final_review_out) + "\n```\n\n";
report += "## Final verdict\n\n";
report += "- reason: " + verdict_reason + "\n";
report += "- final review passed: " + final_review_passed.to_string() + "\n";
report += "- review repair rounds used: " + review_round.to_string() + " (maximum 2)\n";
report += "- quality-gate repair used: " + quality_repair_used.to_string() + " (maximum 1)\n";
report += "- ordinary success budget: 8 agents; maximum path: 21 agents when two review repairs, one quality-gate repair, and the post-quality review panel are all used.\n";
let budget_snapshot = budget();
report += "- agents spent in this run: " + budget_snapshot.spent.to_string() + "\n";
report += "- agent instructions: agents were instructed not to create branches, commits, pushes, PRs, or nested workflows; a blocked run may leave uncommitted modifications, so inspect final git status --porcelain=v1 --untracked-files=all and git diff before relying on this report.\n";
let report_path = write_scratch_file("implement-approval-workflow-stage-report.md", report);
complete(#{
    status: report_status,
    path: report_path,
    stage_id: stage_id,
    branch: if scope_ok { stage_plan.branch } else { "" },
    summary: verdict_reason,
    review_repair_rounds: review_round,
    quality_repair_used: quality_repair_used,
    agents_spent: budget_snapshot.spent,
    agent_budget: budget_snapshot,
});
