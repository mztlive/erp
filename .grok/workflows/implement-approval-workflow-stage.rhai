let meta = #{
    name: "implement-approval-workflow-stage",
    description: "根据 docs/dev-plan/_meta.json 精确实现一个审批工作流阶段，包括 stages[] 对象或 perDocumentTypeStages[].p3/.p4 对象。所有子代理均为 general-purpose。范围与依赖证明为 general-purpose execute 工作；凡项目文件增删改、测试/文档变更、评审修复或质量门禁修复，一律且仅由 capability all 的 general-purpose 执行。基线与质量门禁执行为 general-purpose execute 工作，绝不是评审。合同、架构、安全、正确性、范围、测试与最终验收评审均为 general-purpose execute 工作，且不得改文件。排除 MongoDB 集成测试与浏览器 E2E 测试。普通成功路径使用 8 个代理；最大路径在两次评审修复、一次质量门禁修复及强制的质量后评审面板后为 21 个代理。所有代理必须使用简体中文思考与作答。",
    when_to_use: "仅在已准备好的干净阶段分支上使用，传入 args.stage_id 与可选 args.base_ref（默认 main）；代理被要求不得创建分支、提交、推送、PR 或嵌套工作流；被阻断的运行可能留下未提交修改，需要最终检查 status/diff。",
    phases: [
        #{ title: "Scope", detail: "解析精确元数据阶段，并证明分支、依赖、所有权与前置条件安全" },
        #{ title: "Baseline", detail: "在不改文件的前提下运行适用的阶段前检查" },
        #{ title: "Implement", detail: "仅由 general-purpose/all 负责实现或修复" },
        #{ title: "Review", detail: "运行独立的 general-purpose 只读评审面板" },
        #{ title: "Repair", detail: "将 findings 与门禁失败仅交给 general-purpose/all" },
        #{ title: "Verify", detail: "运行元数据验收与全部适用的合并门禁" },
        #{ title: "Final Review", detail: "执行独立的 general-purpose 最终验收" },
        #{ title: "Report", detail: "写入 scratch Markdown 证据报告" },
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
    let assessed = #{ usable: false, passed: false, findings: [], reason: "评审输出缺失" };
    if output != () {
        if type_of(output) == "map" {
            let shape_ok = is_nonempty_text(output.reviewer_kind) && is_bool_value(output.approved) && is_bool_value(output.scope_clean) && is_bool_value(output.metadata_match) && is_bool_value(output.contract_ok) && is_bool_value(output.tests_sufficient) && is_nonempty_text_array(output.evidence) && is_text_array(output.findings) && is_nonempty_text(output.summary);
            if shape_ok {
                if output.reviewer_kind != expected_kind {
                    assessed.reason = "reviewer_kind 与指派的评审类型不匹配";
                } else {
                    let findings_ok = true;
                    for finding in output.findings {
                        if !finding_valid(finding) {
                            findings_ok = false;
                        }
                    }
                    let core_ok = output.scope_clean == true && output.metadata_match == true && output.contract_ok == true && output.tests_sufficient == true;
                    if !findings_ok {
                        assessed.reason = "评审 findings 格式无效";
                    } else if output.findings.len() == 0 && (output.approved != true || !core_ok) {
                        assessed.reason = "评审未批准且未给出具体 finding";
                    } else {
                        assessed.usable = true;
                        assessed.findings = output.findings;
                        assessed.passed = output.approved == true && core_ok && output.findings.len() == 0;
                        if assessed.passed {
                            assessed.reason = "评审通过";
                        } else {
                            assessed.reason = "评审 findings 或失败证明必须修复";
                        }
                    }
                }
            } else {
                assessed.reason = "评审输出缺少必需的非空证据字段";
            }
        }
    }
    assessed
}

fn assess_review_panel(results, expected_count) {
    let assessed = #{ usable: false, passed: false, findings: [], summaries: [], reasons: [] };
    if results == () {
        assessed.reasons.push("并行评审返回 unit");
    } else if !is_text_array(results) {
        assessed.reasons.push("并行评审返回了非数组");
    } else if results.len() != expected_count {
        assessed.reasons.push("评审面板返回了非预期数量的结果");
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
            let result_assessment = #{ usable: false, passed: false, findings: [], reason: "评审代理失败" };
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
                assessed.reasons.push("评审者 " + index.to_string() + "：" + result_assessment.reason);
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
    let assessed = #{ usable: false, passed: false, environment_blocked: false, reason: "验证输出缺失" };
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
                    assessed.reason = "全部精确质量门禁与验收证据均已通过";
                } else if environment_blocked {
                    assessed.reason = "必需命令的运行环境被阻断";
                } else {
                    assessed.reason = "质量门禁或精确验收证据失败";
                }
            } else {
                assessed.reason = "验证输出缺少必需的非空证据字段";
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
    let common = "角色：只读评审代理。agent_type 为 general-purpose，capability_mode 为 execute。";
    common += "必须全程使用简体中文思考与作答：summary、evidence、findings 中的 issue/evidence/remediation，以及所有说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
    common += "这是只读评审，禁止编辑、格式化、生成、再生成、写入、删除、提交、推送、建分支、修复或改动任何项目文件。不得编写测试或文档。";
    common += "使用 read_file、grep/rg 与终端工具；回答前必须检查当前真实代码与调用点。禁止运行 MongoDB 集成测试或浏览器 E2E 测试。";
    common += "每次下结论前必须精确执行：";
    common += status_command;
    common += "、git branch --show-current、";
    common += diff_command;
    common += "、";
    common += cached_diff_command;
    common += "，以及 ";
    common += "git diff --cached --name-status --";
    common += "。用 ";
    common += base_commit_command;
    common += " 证明基线提交。";
    common += "读取所有显示为 ?? 的路径（含全部未跟踪文件），并逐项对照 owns、ownsWithin 符号范围、creates、deletes、forbids、冻结文件限制与生成文件来源，对每个新增/修改/删除/重命名路径分类。禁止只依赖 files_changed 或实现摘要。";
    common += "重新阅读 docs/dev-plan/_meta.json、docs/dev-plan/conventions.md、docs/dev-plan/approval-workflow.md、精确的 planDoc 以及相关仓库指令。";
    common += "依赖证明必须使用元数据中精确的依赖分支 tip，并将阶段 dependsOnGroups 与项目 dependencyGroups 视为同一份完整依赖组契约；证明每个 dependsOn 阶段及每个被引用组成员都是已解析 base_ref 提交与 HEAD 的祖先。缺失引用、部分 cherry-pick、内容相似或仅分支存在，都不能作为证明。";
    common += "请求的阶段计划：";
    common += json_encode(plan);
    common += "。实现与修复证据：";
    common += json_encode(implementation_evidence);
    common += "。基线引用：";
    common += base_ref_json;
    common += "。";
    common += panel_note;
    common += " 返回具体的文件/行号或命令证据。MongoDB 集成测试执行与浏览器 E2E 执行不在本工作流范围内，不得将其作为批准条件。";

    let contract_prompt = common;
    contract_prompt += "评审类型为 contract。审查合同与业务语义、架构、安全、授权、DataScope 时序、事务/CAS/幂等行为、BPM/ERP 依赖方向、失败即关闭、强类型动作、无 fallback/双写/Noop、无第二定义源，以及精确阶段计划。检查真实代码、测试与调用点。任何违规须 approved=false 并给出具体 findings；approved=false 且无 findings 视为不可用。禁止改文件。";
    jobs.push(#{
        label: label_prefix + "-contract-architecture-security-correctness",
        agent_type: "general-purpose",
        capability_mode: "execute",
        prompt: contract_prompt,
        output_schema: review_schema,
    });

    let ownership_prompt = common;
    ownership_prompt += "评审类型为 ownership。reviewer_kind 必须精确返回 \"ownership\"。对照原始 _meta.json 与 conventions.md，逐项审查完整 status 以及 base/cached name-status 列表：精确的 stages[] 或 perDocumentTypeStages[].p3/.p4 身份、branch、dependsOn、dependencyGroups、owns、ownsWithin 符号范围、creates、deletes、forbids、冻结文件限制与生成文件来源。读取每一个 ?? 文件。任何超出精确所有权的路径、未登记的删除/创建、禁止/冻结/生成文件违规，或依赖边界不匹配，均为阻断性 finding。禁止改文件。";
    jobs.push(#{
        label: label_prefix + "-ownership-scope",
        agent_type: "general-purpose",
        capability_mode: "execute",
        prompt: ownership_prompt,
        output_schema: review_schema,
    });

    let test_prompt = common;
    test_prompt += "评审类型为 tests。审查实际变更代码与未排除测试，覆盖成功路径、失败、边界、回归、调用点遗漏、生成文件漂移、定向命令证据、适用的未排除合并门禁，以及精确的未排除验收结果。禁止运行或要求 MongoDB 集成测试或浏览器 E2E 测试。对剩余的单元、服务、API 契约、静态分析、构建或脚本覆盖缺口，须 tests_sufficient=false 并给出具体 findings。禁止编辑或修复。";
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
    let invalid_report = "# implement-approval-workflow-stage\n\n状态：在调用代理前已阻断。\n\n";
    invalid_report += "## 请求的阶段\n\n";
    if stage_id == () {
        invalid_report += "- stage_id：缺失\n";
    } else {
        invalid_report += "- stage_id：" + json_encode(stage_id) + "\n";
    }
    invalid_report += "- base_ref：" + json_encode(base_ref) + "\n";
    invalid_report += "- 原因：stage_id 为必填项，且必须是精确、安全的元数据标识符；base_ref 必须是精确、安全的 Git 引用，不得包含选项前缀、修订表达式、空白、shell 元字符或 Git 修订危险字符。\n";
    let invalid_path = write_scratch_file("implement-approval-workflow-stage-report.md", invalid_report);
    complete(#{ status: "blocked", path: invalid_path, stage_id: if stage_id == () { "" } else { stage_id }, summary: "stage_id/base_ref 缺失或不安全；未调用任何代理，也未改动任何项目文件。" });
}

let stage_is_p6_pilot = stage_id == "P6-PILOT";
let stage_is_p6_final = stage_id == "P6-FINAL";
let stage_is_p6 = stage_is_p6_pilot || stage_is_p6_final;
let base_ref_json = json_encode(base_ref);
let base_commit_command = "git rev-parse --verify --end-of-options " + base_ref_json + "^{commit}";
let base_diff_command = "git diff --name-status " + base_ref_json + " --";
let cached_base_diff_command = "git diff --cached --name-status " + base_ref_json + " --";

phase("Scope");
let scope_prompt = "角色：只读范围与实现计划代理。agent_type 为 general-purpose，capability_mode 为 execute。";
scope_prompt += "必须全程使用简体中文思考与作答：evidence、blockers、implementation_steps、acceptance 中的说明文字，以及所有结论必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
scope_prompt += "这是阻断性只读范围证明。禁止编辑、格式化、生成、再生成、写入、删除、提交、推送、建分支、修复或改动任何项目文件、测试、文档、生成文件或工作流。";
scope_prompt += "使用 read_file、grep/rg 与终端证据。先阅读 docs/dev-plan/README.md、docs/dev-plan/conventions.md、docs/dev-plan/domains.md、docs/dev-plan/_meta.json、docs/dev-plan/approval-workflow.md、docs/approval-workflow-implementation-plan/README.md、docs/approval-workflow-contract.md、docs/erp-phase-1.md、docs/erp-phase-2.md，以及 docs/approval-workflow-implementation-plan/11-integration-and-acceptance.md。";
scope_prompt += "然后阅读从元数据选出的精确 planDoc，包括 stages[] 对应的计划文件，或 perDocumentTypeStages[].p3/.p4 对象的默认计划文件。从 _meta.json 精确解析请求的 stage_id：在 projects.approval-workflow.stages[] 中找一个精确对象，或在 projects.approval-workflow.perDocumentTypeStages[] 下找一个精确 p3/p4 对象。禁止发明、合并、重命名或推断阶段模型。";
scope_prompt += "返回精确的 id、source、规范化 stage_kind、documentType、approvalRequirement、branch、dependsOn、dependencyGroups、owns、ownsWithin、creates、deletes、forbids、acceptance、实现步骤，以及完整适用的 backend、frontend、docs 合并门禁列表。stage_source 必须精确为下列三者之一，不得加 projects.approval-workflow. 前缀：stages[]、perDocumentTypeStages[].p3、perDocumentTypeStages[].p4。stage_kind 是脚本规范化字段，不是元数据源字段：当 stage_source 为 stages[] 时必须为 top_level；当 stage_source 为 perDocumentTypeStages[].p3 时必须为 p3；当 stage_source 为 perDocumentTypeStages[].p4 时必须为 p4。stages[] 可将 documentType 与 approvalRequirement 返回为空字符串；仅 perDocumentTypeStages[].p3/.p4 必须将二者都返回为非空字符串。";
scope_prompt += "下任何结论前先运行：git status --porcelain=v1 --untracked-files=all；";
scope_prompt += base_diff_command;
scope_prompt += "；";
scope_prompt += cached_base_diff_command;
scope_prompt += "；git diff --cached --name-status --；以及 ";
scope_prompt += base_commit_command;
scope_prompt += "。安全加引号的基线引用必须按所示原样使用，且每个路径列表都要使用 --。读取所有显示为 ?? 的路径（即使不在 files_changed 中），并逐项对照 owns、ownsWithin 符号范围、creates、deletes、forbids、冻结文件限制与生成文件来源，对每个新增/修改/删除/重命名路径分类。";
scope_prompt += "启动工作区必须为空，且当前分支必须等于精确的元数据分支。owns 或 ownsWithin 至少返回一项；当元数据没有对应条目时，creates、deletes 或 forbids 为空列表是合法的。";
scope_prompt += "将阶段 dependsOnGroups 与项目 dependencyGroups 视为同一份完整依赖组契约。对每个 dependsOn 阶段以及每个被引用 dependencyGroups 组的每个成员，用 git rev-parse --verify --end-of-options <safe-dependency-branch>^{commit} 从原始元数据解析精确依赖分支；再用 git merge-base --is-ancestor 证明该依赖 tip 是已解析 base_ref 提交与 HEAD 的祖先。缺失引用、部分 cherry-pick、内容相似或仅分支存在均视为失败。不得接受任何更弱的依赖声明。";
scope_prompt += "核验 DOC 门禁、P0 前置条件、试点门禁、冻结文件、生成文件规则以及每一条计划前置条件。任何未证明的依赖须将 dependencies_satisfied=false。acceptance 与合并门禁列表必须忠实于来源，但本工作流排除 MongoDB 集成测试与浏览器 E2E 测试的执行：不要把它们放进 baseline_commands 或实现校验，并在 evidence 中记录排除原因。";
scope_prompt += "对于 P6-PILOT 与 P6-FINAL，须显式包含这些未排除的 backend 命令：cargo fmt --all -- --check、cargo check --workspace、cargo clippy --workspace --all-targets --all-features -- -D warnings、cargo test --workspace、cargo test -p bpm --lib，以及 ./scripts/check-bpm-boundaries.sh。同时包含完整的未排除 frontend 门禁以及 npm run openapi:lint。每条结论都必须返回非空 evidence。没有阻断项时，精确返回 blockers: []，不要使用占位阻断字符串。";
scope_prompt += "仅当全部元数据、分支、依赖、干净启动、前置条件、所有权与门禁证明都具体可核验时，才返回 scope_safe=true。请求的 stage_id：";
scope_prompt += json_encode(stage_id);
scope_prompt += "。请求的 base_ref：";
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

let scope_details = "未返回可用的范围结果。";
if scope_out != () {
    scope_details = json_encode(scope_out);
}
let scope_ok = false;
if scope_out != () {
    if type_of(scope_out) == "map" {
        let stage_source = scope_out.stage_source;
        if stage_source == "projects.approval-workflow.stages[]" {
            stage_source = "stages[]";
        } else if stage_source == "projects.approval-workflow.perDocumentTypeStages[].p3" {
            stage_source = "perDocumentTypeStages[].p3";
        } else if stage_source == "projects.approval-workflow.perDocumentTypeStages[].p4" {
            stage_source = "perDocumentTypeStages[].p4";
        }
        scope_out.stage_source = stage_source;
        let source_ok = stage_source == "stages[]" || stage_source == "perDocumentTypeStages[].p3" || stage_source == "perDocumentTypeStages[].p4";
        let stage_kind_ok = false;
        if stage_source == "stages[]" {
            stage_kind_ok = scope_out.stage_kind == "top_level";
        } else if stage_source == "perDocumentTypeStages[].p3" {
            stage_kind_ok = scope_out.stage_kind == "p3";
        } else if stage_source == "perDocumentTypeStages[].p4" {
            stage_kind_ok = scope_out.stage_kind == "p4";
        }
        let document_fields_ok = type_of(scope_out.document_type) == "string" && type_of(scope_out.approval_requirement) == "string";
        if stage_source == "perDocumentTypeStages[].p3" || stage_source == "perDocumentTypeStages[].p4" {
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
    blocked_reason = "范围/计划证明失败，或请求的阶段不安全。未启动基线或可写代理。";
}
if !blocked {
    if baseline_commands.len() == 0 {
        blocked = true;
        blocked_reason = "剔除 MongoDB 集成测试与浏览器 E2E 测试后，没有剩余未排除的基线命令。";
    }
}
if !blocked {
    let scope_budget = budget();
    let minimum_remaining = 7;
    if scope_budget.remaining < minimum_remaining {
        blocked = true;
        blocked_reason = "范围阶段后剩余代理预算不足；普通路径至少还需要 " + minimum_remaining.to_string() + " 个代理。";
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
        blocked_reason = "范围阶段未能解析出任何适用的可执行合并门禁或验收命令。";
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
        blocked_reason = "启动 general-purpose 基线前剩余代理预算不足。";
    } else {
        let baseline_prompt = "角色：基线执行代理。agent_type 为 general-purpose，capability_mode 为 execute。这不是评审，禁止编辑、格式化、生成、再生成、写入、删除、提交、推送、建分支、修复或改动源码、测试、文档、生成文件或工作流。";
        baseline_prompt += "必须全程使用简体中文思考与作答：summary、evidence、blocked_reason 以及所有说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
        baseline_prompt += "阅读精确阶段计划并复核原始元数据/约定。先运行 git status --porcelain=v1 --untracked-files=all 与 git branch --show-current。同时运行 ";
        baseline_prompt += base_diff_command;
        baseline_prompt += "、";
        baseline_prompt += cached_base_diff_command;
        baseline_prompt += "、git diff --cached --name-status --，以及 ";
        baseline_prompt += base_commit_command;
        baseline_prompt += "。读取每一个 ?? 文件并确认启动工作区干净；不要信任 files_changed 摘要。";
        baseline_prompt += "在正确的仓库目录中运行下列每一条只读基线命令。MongoDB 集成测试与浏览器 E2E 测试已从此列表移除，禁止运行或重新加入。不得修复失败、不得跳过剩余命令，也不得把已失败的既有命令标为通过。对每条过滤后的基线命令恰好返回一条命令结果，且不得有其他命令结果；每条结果必须保留精确命令原文并附简要证据。无阻断时 blocked_reason 记录为非空的 none。";
        baseline_prompt += "阶段计划：";
        baseline_prompt += json_encode(stage_plan);
        baseline_prompt += "。过滤后的基线命令：";
        baseline_prompt += json_encode(baseline_commands);
        baseline_prompt += "。期望分支：";
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
            blocked_reason = "general-purpose 基线执行未通过、启动工作区/分支不安全，或基线证据不完整。未尝试实现。";
        }
    }
}

if !blocked {
    phase("Implement");
    let implementation_budget = budget();
    if implementation_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "启动实现前剩余代理预算不足。";
    } else {
        let implementation_prompt = "角色：实现代理。agent_type 为 general-purpose，capability_mode 为 all。这是可写实现角色：凡项目文件增删改、测试代码变更、文档编辑、评审修复与质量门禁修复，必须由 general-purpose/all 完成；仅 execute 的评审者不得改文件。";
        implementation_prompt += "必须全程使用简体中文思考与作答：summary、blockers、commands 说明以及所有说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
        implementation_prompt += "编辑前先阅读 backend/AGENTS.md 或其仓库指令间接文件；若涉及 frontend 路径，还要阅读 erp-client/AGENTS.md 与 erp-client/CLAUDE.md；以及相关编码规范、docs/dev-plan/conventions.md、docs/dev-plan/domains.md、docs/dev-plan/_meta.json、docs/dev-plan/approval-workflow.md、docs/approval-workflow-contract.md、docs/erp-phase-1.md、docs/erp-phase-2.md 与精确 planDoc。";
        implementation_prompt += "编辑前运行 git status --porcelain=v1 --untracked-files=all、";
        implementation_prompt += base_diff_command;
        implementation_prompt += "、";
        implementation_prompt += cached_base_diff_command;
        implementation_prompt += "，以及 ";
        implementation_prompt += base_commit_command;
        implementation_prompt += "。读取每一个 ?? 文件，并对照 owns、ownsWithin、creates、deletes、forbids、冻结文件与生成文件规则，对全部实际路径分类。修改入口点前必须做全仓库符号与调用点搜索。";
        implementation_prompt += "只实现下方精确元数据阶段计划；遵守 owns 与 ownsWithin 符号范围，不得触碰冻结或无所有权文件。禁止发明另一套阶段模型、兼容路径、fallback、双写、Noop 成功、默认办理人或未文档化的业务规则。";
        implementation_prompt += "所有新增或修改的非生成 Rust 方法/函数须遵守仓库多行 /// 文档规则；生产 Rust 不得使用禁止的 unwrap/expect/panic。frontend 路径须：业务 UI 使用 Client Components，全部请求使用 TanStack Query，表单使用 useAppForm 加 Zod，新函数/hooks/组件使用 JSDoc，用户可见文案使用术语表，客户端不得出现 BPM 内部类型。生成文件只能由已登记生成器产出，禁止手改。";
        implementation_prompt += "编辑后只运行未排除的阶段定向命令。禁止运行 MongoDB 集成测试或浏览器 E2E 测试，也不得将二者当作校验证据。禁止创建分支、提交、推送、开 PR 或修改本工作流文件。若元数据、依赖、所有权或业务语义无法安全实现，停止并返回 completed=false，不要猜测。";
        implementation_prompt += "精确阶段计划：";
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
            blocked_reason = "general-purpose 实现代理失败、报告了不安全范围，或返回了不完整的实现证据。";
        }
    }
}

while !blocked && !review_passed && review_round <= 2 {
    phase("Review");
    let expected_reviewers = 3;
    let review_jobs = build_review_jobs(stage_plan, base_ref, implementation_summaries, review_schema, "general:approval-review-round-" + review_round.to_string(), "这是常规独立评审面板。");
    let review_budget = budget();
    if review_budget.remaining < expected_reviewers {
        blocked = true;
        blocked_reason = "启动第 " + review_round.to_string() + " 轮评审面板前剩余代理预算不足。";
    } else {
        let review_results = parallel(review_jobs);
        let panel_assessment = assess_review_panel(review_results, expected_reviewers);
        for panel_summary in panel_assessment.summaries {
            review_summaries.push(panel_summary);
        }
        if panel_assessment.usable != true {
            blocked = true;
            blocked_reason = "必需的 general-purpose 评审代理失败或缺少证据：" + json_encode(panel_assessment.reasons);
        } else if panel_assessment.passed == true {
            review_passed = true;
        } else if review_round >= 2 {
            blocked = true;
            blocked_reason = "实现未在允许的两轮评审修复内通过必需的 general-purpose 评审面板。Findings：" + json_encode(panel_assessment.findings);
        } else {
            phase("Repair");
            let repair_budget = budget();
            if repair_budget.remaining < 1 {
                blocked = true;
                blocked_reason = "启动第 " + review_round.to_string() + " 轮评审修复前剩余代理预算不足。";
            } else {
                let repair_prompt = "角色：评审修复代理。agent_type 为 general-purpose，capability_mode 为 all。凡项目文件增删改、测试编写、文档编辑、评审修复与质量门禁修复，只能由 general-purpose/all 完成；这是可写修复角色，不是评审。仅 execute 的评审者不得改文件。";
                repair_prompt += "必须全程使用简体中文思考与作答：summary、blockers 以及所有说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
                repair_prompt += "编辑前阅读当前真实 diff 与代码、原始 docs/dev-plan/_meta.json、conventions.md、approval-workflow.md、精确 planDoc 以及相关指令。运行 git status --porcelain=v1 --untracked-files=all、";
                repair_prompt += base_diff_command;
                repair_prompt += "、";
                repair_prompt += cached_base_diff_command;
                repair_prompt += "，以及 ";
                repair_prompt += base_commit_command;
                repair_prompt += "。读取每一个 ?? 文件，并将每个实际路径对照 owns、ownsWithin、creates、deletes、forbids、冻结与生成文件规则。";
                repair_prompt += "只修复下方对精确元数据阶段有效的具体 findings。禁止扩大范围、触碰无所有权/禁止/冻结/生成路径、发明兼容行为、削弱测试，或改动无关的既有代码。生成文件使用已登记生成器。禁止提交、推送、建分支或修改工作流。修复后只运行未排除的定向命令；禁止运行 MongoDB 集成测试或浏览器 E2E 测试。若某条 finding 无法在 owns/ownsWithin 内安全修复，返回 completed=false，不要猜测。";
                repair_prompt += "精确阶段计划：";
                repair_prompt += json_encode(stage_plan);
                repair_prompt += "。评审 findings：";
                repair_prompt += json_encode(panel_assessment.findings);
                repair_prompt += "。评审证据摘要：";
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
                    blocked_reason = "general-purpose 评审修复失败，或返回了不安全/不完整的证据。";
                } else {
                    review_round += 1;
                }
            }
        }
    }
}
if !review_passed && !blocked {
    blocked = true;
    blocked_reason = "评审循环结束时，没有通过的 general-purpose 评审面板。";
}

if !blocked {
    phase("Verify");
    let verification_prompt = "角色：质量门禁执行代理。agent_type 为 general-purpose，capability_mode 为 execute。这是命令执行，不是评审；禁止编辑、格式化、生成、再生成、写入、删除、提交、推送、建分支、修复或改动文件。凡项目文件变更与门禁修复，一律且仅由 general-purpose/all 负责。";
    verification_prompt += "必须全程使用简体中文思考与作答：summary、evidence、commands/acceptance_results 中的说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
    verification_prompt += "阅读原始 docs/dev-plan/_meta.json、docs/dev-plan/conventions.md、docs/dev-plan/approval-workflow.md、docs/approval-workflow-contract.md、docs/erp-phase-1.md、docs/erp-phase-2.md、精确 planDoc、仓库指令以及当前真实代码。下任何结论前运行 git status --porcelain=v1 --untracked-files=all、git branch --show-current、";
    verification_prompt += base_diff_command;
    verification_prompt += "、";
    verification_prompt += cached_base_diff_command;
    verification_prompt += "、git diff --cached --name-status --，以及 ";
    verification_prompt += base_commit_command;
    verification_prompt += "。读取每一个 ?? 文件，并将全部新增、修改、删除、重命名路径逐项对照 owns、ownsWithin、creates、deletes、forbids、冻结文件与生成文件来源；不要信任 files_changed。";
    verification_prompt += "确认最终范围仍匹配精确元数据阶段与依赖证明。在正确仓库目录中运行下方精确期望命令集，不得替换、别名、截断或重复。";
    verification_prompt += "对每条期望命令恰好返回一条命令结果，且不得有其他命令结果；每条结果都需要非空 command、summary 与 evidence。acceptance_results 必须对 expected_acceptance_checks 的每一项恰好一条，顺序与原文完全一致；expected_acceptance_checks 是未排除的阶段验收，再加上每条不可执行且未排除的合并门禁，并已去重。每条验收必须通过，并在适用处包含人工证据。不要把 DOC 人工相对 Markdown 链接检查或阶段 10 语义扫描写成 shell 命令。";
    verification_prompt += "MongoDB 集成测试与浏览器 E2E 测试已排除。禁止运行它们、把它们加入结果、要求其环境，或将其当作验收证据。仅当精确期望命令集中仍保留的某条命令无法在其所需环境中运行时，才设置 environment_blocked=true。";
    verification_prompt += "精确阶段计划：";
    verification_prompt += json_encode(stage_plan);
    verification_prompt += "。期望命令集：";
    verification_prompt += json_encode(expected_commands);
    verification_prompt += "。期望验收检查：";
    verification_prompt += json_encode(expected_acceptance_checks);
    if stage_is_p6 {
        verification_prompt += "。P6 未排除的可执行 backend 门禁明确为 cargo fmt --all -- --check、cargo check --workspace、cargo clippy --workspace --all-targets --all-features -- -D warnings、cargo test --workspace、cargo test -p bpm --lib，以及 ./scripts/check-bpm-boundaries.sh；未排除的 frontend 门禁为 npm run format:check、npm run lint、npm run test、npm run test:node、npm exec -- tsc --noEmit，以及 npm run build；同时还要运行 npm run openapi:lint。";
    }
    let verification_budget = budget();
    if verification_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "启动质量门禁验证前剩余代理预算不足。";
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
            blocked_reason = "general-purpose 质量门禁代理失败，或返回了不完整的门禁证据。";
        } else if verification_assessment.environment_blocked == true {
            blocked = true;
            blocked_reason = "某条必需且未排除的质量门禁因环境被阻断；工作流失败即关闭，不得宣称成功。";
        } else if verification_assessment.passed != true {
            phase("Repair");
            let quality_repair_budget = budget();
            if quality_repair_budget.remaining < 1 {
                blocked = true;
                blocked_reason = "启动质量门禁修复前剩余代理预算不足。";
            } else {
                let quality_repair_prompt = "角色：质量门禁修复代理。agent_type 为 general-purpose，capability_mode 为 all。凡项目文件变更（含质量门禁失败修复）一律且仅由 general-purpose/all 负责；这不是评审，仅 execute 的代理不得改文件。";
                quality_repair_prompt += "必须全程使用简体中文思考与作答：summary、blockers 以及所有说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
                quality_repair_prompt += "检查当前真实 diff、原始元数据/约定、精确 planDoc、仓库指令以及完整的失败门禁证据。编辑前运行 git status --porcelain=v1 --untracked-files=all、";
                quality_repair_prompt += base_diff_command;
                quality_repair_prompt += "、";
                quality_repair_prompt += cached_base_diff_command;
                quality_repair_prompt += "，以及 ";
                quality_repair_prompt += base_commit_command;
                quality_repair_prompt += "。读取每一个 ?? 文件，并将每个实际路径逐项对照 owns、ownsWithin、creates、deletes、forbids、冻结与生成文件规则。";
                quality_repair_prompt += "只修复本阶段引入的失败，且仅限 owns/ownsWithin 范围内。禁止掩盖既有或环境失败、削弱测试、跳过未排除门禁、手改生成文件、改动禁止/冻结/无所有权路径、提交、推送、建分支或修改工作流。禁止运行 MongoDB 集成测试或浏览器 E2E 测试。若失败是既有的、环境性的或无法安全修复，返回 completed=false。修复后运行失败的未排除定向命令。";
                quality_repair_prompt += "精确阶段计划：";
                quality_repair_prompt += json_encode(stage_plan);
                quality_repair_prompt += "。期望命令集：";
                quality_repair_prompt += json_encode(expected_commands);
                quality_repair_prompt += "。失败的质量证据：";
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
                    blocked_reason = "唯一允许的 general-purpose 质量门禁修复失败，或返回了不安全/不完整的证据。";
                } else {
                    quality_repair_used = true;
                    phase("Verify");
                    let rerun_budget = budget();
                    if rerun_budget.remaining < 1 {
                        blocked = true;
                        blocked_reason = "启动强制质量门禁重跑前剩余代理预算不足。";
                    } else {
                        let verification_rerun_prompt = verification_prompt;
                        verification_rerun_prompt += " 这是 general-purpose 质量修复后的强制全量重跑。必须从零重跑每条期望命令与每条验收结果，返回完全相同的命令集与验收集形态，不得依赖第一次结果。";
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
                            blocked_reason = "在唯一允许的 general-purpose 修复后，完整质量门禁仍未通过，或重跑缺少精确命令与验收证据。";
                        } else {
                            phase("Review");
                            let post_review_expected = 3;
                            let post_review_jobs = build_review_jobs(stage_plan, base_ref, implementation_summaries, review_schema, "general:approval-post-quality-review", "这是质量修复后的强制评审。任何证明失败或非空 findings 立即阻断；本面板之后不允许再修复。");
                            let post_review_budget = budget();
                            if post_review_budget.remaining < post_review_expected {
                                blocked = true;
                                blocked_reason = "启动强制质量后评审面板前剩余代理预算不足。";
                            } else {
                                let post_review_results = parallel(post_review_jobs);
                                let post_panel_assessment = assess_review_panel(post_review_results, post_review_expected);
                                for post_summary in post_panel_assessment.summaries {
                                    review_summaries.push(post_summary);
                                }
                                if post_panel_assessment.usable != true || post_panel_assessment.passed != true {
                                    blocked = true;
                                    blocked_reason = "修复后强制的 general-purpose 质量后评审失败；工作流已阻断。Findings：" + json_encode(post_panel_assessment.findings) + "。原因：" + json_encode(post_panel_assessment.reasons);
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
    let final_review_prompt = "角色：只读最终验收评审者。agent_type 为 general-purpose，capability_mode 为 execute。凡项目文件增删改、测试编写、文档编辑、评审修复与质量门禁修复，一律且仅由 general-purpose/all 负责；禁止编辑、格式化、生成、再生成、写入、删除、提交、推送、建分支、修复或改动任何项目文件。";
    final_review_prompt += "必须全程使用简体中文思考与作答：summary、evidence、findings 中的 issue/evidence/remediation，以及所有说明文字必须为中文；命令原文、路径、标识符与 JSON 字段名保持原样，不要翻译。";
    final_review_prompt += "检查当前完整真实 diff 与 status、实际变更代码与测试、每一个未跟踪 ?? 文件，以及原始 docs/dev-plan/_meta.json、docs/dev-plan/conventions.md、docs/dev-plan/approval-workflow.md、精确 planDoc、仓库指令、基线、实现/修复证据、全部评审证据与完整质量门禁证据。下任何结论前运行 git status --porcelain=v1 --untracked-files=all、git branch --show-current、";
    final_review_prompt += base_diff_command;
    final_review_prompt += "、";
    final_review_prompt += cached_base_diff_command;
    final_review_prompt += "、git diff --cached --name-status --，以及 ";
    final_review_prompt += base_commit_command;
    final_review_prompt += "。读取每一个 ?? 文件，并逐项对照 owns、ownsWithin、creates、deletes、forbids、冻结文件限制与生成文件来源，对每个新增/修改/删除/重命名路径分类。禁止仅凭 files_changed 或摘要批准。";
    final_review_prompt += "证明每个 dependsOn 阶段以及阶段 dependsOnGroups 或项目 dependencyGroups 的每个成员，都已通过精确依赖分支 tip 祖先关系并入 base_ref 与 HEAD；部分 cherry-pick、内容相似与仅分支存在都不能作为证明。检查精确阶段身份与分支、合同与业务语义、架构/安全/正确性、测试充分性、精确未排除命令与验收证据，以及完整未排除门禁证据。MongoDB 集成测试与浏览器 E2E 测试不在本工作流范围内：禁止运行、要求或将其缺失当作 finding。";
    final_review_prompt += "评审类型必须为 final，精确返回 \"final\"。仅当每条必需且未排除的证明都具体可核验、且当前 diff 可作为该精确元数据阶段安全合并时，才返回 approved=true。任何非空 findings 均阻断；approved=false 且 findings 为空视为不可用。返回非空 evidence 与 summary。禁止改文件。";
    final_review_prompt += " 精确阶段计划：";
    final_review_prompt += json_encode(stage_plan);
    final_review_prompt += "。实现与修复证据：";
    final_review_prompt += json_encode(implementation_summaries);
    final_review_prompt += "。评审证据：";
    final_review_prompt += json_encode(review_summaries);
    final_review_prompt += "。质量门禁证据：";
    final_review_prompt += json_encode(verification_out);
    let final_budget = budget();
    if final_budget.remaining < 1 {
        blocked = true;
        blocked_reason = "启动最终验收评审前剩余代理预算不足。";
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
            blocked_reason = "general-purpose 最终验收评审失败，或返回了不完整证据：" + final_assessment.reason;
        } else if !final_review_passed {
            blocked = true;
            blocked_reason = "最终 general-purpose 验收评审发现阻断问题；未返回不安全批准。Findings：" + json_encode(final_assessment.findings);
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
    verdict_reason = "请求阶段的全部精确元数据、所有权、依赖、评审、质量门禁、验收与最终评审证明均已通过。";
}
if !is_nonempty_text(verdict_reason) {
    verdict_reason = "工作流已阻断，因为缺少必需证明。";
}
let report = "# implement-approval-workflow-stage\n\n";
report += "状态：" + report_status + "\n\n";
report += "## 阶段\n\n";
report += "- 请求的 stage_id：" + json_encode(stage_id) + "\n";
report += "- base_ref：" + base_ref_json + "\n";
if scope_ok {
    report += "- 解析到的 stage_id：" + json_encode(stage_plan.stage_id) + "\n";
    report += "- 阶段来源：" + json_encode(stage_plan.stage_source) + "\n";
    report += "- 元数据分支：" + json_encode(stage_plan.branch) + "\n";
    report += "- planDoc：" + json_encode(stage_plan.plan_doc) + "\n";
    report += "- 已排除测试：MongoDB 集成测试与浏览器 E2E\n";
} else {
    report += "- 解析到的阶段：不可用或不安全\n";
}
report += "\n## 范围计划\n\n```json\n" + scope_details + "\n```\n\n";
report += "## 基线\n\n";
if baseline_out == () {
    report += "未运行或无可用结果。\n\n";
} else {
    report += "```json\n" + json_encode(baseline_out) + "\n```\n\n";
}
report += "## 实现与修复\n\n```json\n" + json_encode(implementation_summaries) + "\n```\n\n";
report += "## 评审\n\n```json\n" + json_encode(review_summaries) + "\n```\n\n";
report += "## 修复记录\n\n```json\n" + json_encode(repair_summaries) + "\n```\n\n";
report += "## 质量门禁\n\n";
if verification_out == () {
    report += "未运行或无可用结果。\n\n";
} else {
    report += "```json\n" + json_encode(verification_out) + "\n```\n\n";
}
report += "## 已排除测试\n\n";
report += "- 未执行、也未要求 MongoDB 集成测试。\n";
report += "- 未执行、也未要求浏览器 E2E 测试。\n\n";
report += "## 最终评审\n\n```json\n" + json_encode(final_review_out) + "\n```\n\n";
report += "## 最终裁决\n\n";
report += "- 原因：" + verdict_reason + "\n";
report += "- 最终评审通过：" + final_review_passed.to_string() + "\n";
report += "- 已用评审修复轮数：" + review_round.to_string() + "（最多 2）\n";
report += "- 已用质量门禁修复：" + quality_repair_used.to_string() + "（最多 1）\n";
report += "- 普通成功预算：8 个代理；最大路径：在两次评审修复、一次质量门禁修复及质量后评审面板全部使用时为 21 个代理。\n";
let budget_snapshot = budget();
report += "- 本次运行已消耗代理数：" + budget_snapshot.spent.to_string() + "\n";
report += "- 代理指令：代理已被要求不得创建分支、提交、推送、PR 或嵌套工作流；被阻断的运行可能留下未提交修改，因此在采信本报告前请检查最终 git status --porcelain=v1 --untracked-files=all 与 git diff。\n";
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
