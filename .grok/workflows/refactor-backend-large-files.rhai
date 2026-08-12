let meta = #{
    name: "refactor-backend-large-files",
    description: "Analyze backend large Rust files, execute a bounded split batch, run real quality gates, and independently review behavior preservation",
    phases: [
        #{ title: "Baseline", detail: "run original tests before edits" },
        #{ title: "Analyze", detail: "count files and inspect API/test shape" },
        #{ title: "Plan", detail: "produce a concrete split plan" },
        #{ title: "Implement", detail: "split independent targets in parallel" },
        #{ title: "Verify", detail: "run formatting, compile, clippy, and tests" },
        #{ title: "Review", detail: "independent plan, API, and quality reviews" },
        #{ title: "Report", detail: "derive an evidence-based final result" },
    ],
};

let gate_schema = #{
    "type": "object", "required": ["passed", "commands", "evidence", "failures"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "commands": #{ "type": "array", "items": #{ "type": "string" } },
        "evidence": #{ "type": "string" },
        "failures": #{ "type": "array", "items": #{ "type": "string" } },
    },
};
let analysis_schema = #{
    "type": "object", "required": ["line_count_report", "target_assessment", "api_shape", "test_strategy"],
    "properties": #{
        "line_count_report": #{ "type": "string" },
        "target_assessment": #{ "type": "string" },
        "api_shape": #{ "type": "string" },
        "test_strategy": #{ "type": "string" },
    },
};
let plan_schema = #{
    "type": "object", "required": ["plan_markdown", "targets", "global_invariants"],
    "properties": #{
        "plan_markdown": #{ "type": "string" },
        "targets": #{ "type": "array", "maxItems": 6, "items": #{
            "type": "object", "required": ["target", "steps", "invariants", "tests"],
            "properties": #{
                "target": #{ "type": "string" },
                "steps": #{ "type": "array", "items": #{ "type": "string" } },
                "invariants": #{ "type": "array", "items": #{ "type": "string" } },
                "tests": #{ "type": "array", "items": #{ "type": "string" } },
            },
        }},
        "global_invariants": #{ "type": "array", "items": #{ "type": "string" } },
    },
};
let implementation_schema = #{
    "type": "object", "required": ["changed", "summary", "files", "evidence", "risks"],
    "properties": #{
        "changed": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "files": #{ "type": "array", "items": #{ "type": "string" } },
        "evidence": #{ "type": "string" },
        "risks": #{ "type": "array", "items": #{ "type": "string" } },
    },
};
let review_schema = #{
    "type": "object", "required": ["passed", "findings", "evidence"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "findings": #{ "type": "array", "items": #{ "type": "string" } },
        "evidence": #{ "type": "string" },
    },
};
let report_schema = #{
    "type": "object", "required": ["summary", "markdown"],
    "properties": #{
        "summary": #{ "type": "string" },
        "markdown": #{ "type": "string" },
    },
};

if args == () { pause("verification", "Pass repo_root, backend_root, threshold, and targets."); }
let repo_root = args.repo_root;
let backend_root = args.backend_root;
let threshold = args.threshold;
let targets = args.targets;
if repo_root == () || backend_root == () || targets == () || targets.len() == 0 {
    pause("verification", "repo_root, backend_root, and at least one target are required.");
}

phase("Baseline");
let baseline_prompt = "Work in repository " + repo_root + ". Read backend/AGENTS.md before doing anything. ";
baseline_prompt += "Do not edit files. Establish the real pre-refactor baseline by running `git status --short` and `cd backend && cargo test --workspace`. ";
baseline_prompt += "Set passed=true only if the cargo test command actually exits successfully. Include command output facts such as test counts or the exact failing target. Never infer or fabricate success.";
let baseline = agent(baseline_prompt, #{
    label: "baseline-tests", capability_mode: "execute", output_schema: gate_schema,
});
if baseline == () || !baseline.success || baseline.output.passed != true {
    complete(#{
        status: "blocked-baseline",
        summary: "Original workspace tests did not produce a verified passing baseline; no refactor was attempted.",
        baseline: if baseline != () && baseline.success { baseline.output } else { () },
    });
}
log("Original cargo test baseline passed with concrete command evidence.");

phase("Analyze");
let analysis_prompt = "Analyze the Rust backend under " + backend_root + " using actual repository tools. Read backend/AGENTS.md, ";
analysis_prompt += "the Rust file-organization and testing-review standards under /Users/huangjiajiang/.claude/skills/rust-coding-standards/references/, and docs/split/README.md. ";
analysis_prompt += "Run a line-count scan for all .rs files excluding target and report every file at or above " + threshold.to_string() + " lines, summarized by layer and risk. ";
analysis_prompt += "Deeply inspect these bounded targets: " + json_encode(targets) + ". For each, inspect its existing split plan, current exports/re-exports, downstream imports, validation attributes, tests, and module visibility. ";
analysis_prompt += "Explain why this batch is lower risk than simultaneously splitting core transactional services. Document the public and crate-visible API shape that must remain unchanged and a real verification strategy. Do not edit files.";
let analysis = agent(analysis_prompt, #{
    label: "large-file-analysis", capability_mode: "read-only", output_schema: analysis_schema,
});
if analysis == () || !analysis.success {
    complete(#{ status: "blocked-analysis", summary: "Large-file analysis failed; no edits were made." });
}

phase("Plan");
let plan_prompt = "Create an executable split plan for the bounded targets in " + json_encode(targets) + ". ";
plan_prompt += "Use this verified analysis: " + json_encode(analysis.output) + ". ";
plan_prompt += "Read each target and its docs/split plan directly before finalizing. Plan facade modules and responsibility-named child files, exact re-exports and visibility, test migration, and verification commands. ";
plan_prompt += "The plan must preserve all existing public DTO names, serde/validator behavior, From conversions, normalized query behavior, import paths, and downstream service/handler compilation. ";
plan_prompt += "Do not invent API improvements or business changes. Do not edit files.";
let plan = agent(plan_prompt, #{
    label: "split-planner", capability_mode: "read-only", output_schema: plan_schema,
});
if plan == () || !plan.success || plan.output.targets == () || plan.output.targets.len() == 0 {
    complete(#{ status: "blocked-plan", summary: "No executable split plan was produced; no edits were made.", analysis: analysis.output });
}

phase("Implement");
let implementation_jobs = [];
for target in targets {
    let implementation_prompt = "Implement only the split for " + target + " in repository " + repo_root + ". ";
    implementation_prompt += "Read backend/AGENTS.md and the Rust file-organization/performance/testing-review standards first. ";
    implementation_prompt += "Use the complete approved plan: " + json_encode(plan.output) + ". ";
    implementation_prompt += "Also read the target's docs/split plan and actual source. Edit only this target module and its matching docs/split plan status if and only if implementation is complete. ";
    implementation_prompt += "Preserve public and crate-visible names, import paths, serde representation, validator behavior, conversion behavior, default sort/page semantics, and tests. ";
    implementation_prompt += "Use responsibility-named child modules and a thin dto facade; do not change business logic. Do not commit, reset, stash, delete unrelated files, or claim tests passed without running them. ";
    implementation_prompt += "Run rustfmt on touched Rust files and at least a relevant package check/test when practical. Report exact files and command evidence.";
    implementation_jobs.push(#{
        prompt: implementation_prompt,
        label: "implement:" + target,
        capability_mode: "all",
        output_schema: implementation_schema,
    });
}
let implementations = parallel(implementation_jobs);
let implementation_failures = [];
let implementation_index = 0;
for implementation in implementations {
    if implementation == () || !implementation.success || implementation.output.changed != true {
        implementation_failures.push(targets[implementation_index]);
    }
    implementation_index += 1;
}
if implementation_failures.len() > 0 {
    log("Some implementation agents did not report completed changes: " + json_encode(implementation_failures));
}

phase("Verify");
let verify_prompt = "Independently verify the current shared worktree after the DTO splits. Read backend/AGENTS.md and inspect `git diff --stat`, `git diff --check`, and the full diff. ";
verify_prompt += "Confirm edits are confined to the bounded targets " + json_encode(targets) + " and their docs/split plans, with no public/API shape drift. ";
verify_prompt += "Run these exact real gates from backend: `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`. ";
verify_prompt += "Set passed=true only if every command exits zero and the diff inspection finds no behavior or contract change. Include exact failure evidence; never fabricate success. Do not edit files.";
let verification = agent(verify_prompt, #{
    label: "full-verification", capability_mode: "execute", output_schema: gate_schema,
});

if verification == () || !verification.success || verification.output.passed != true {
    log("Initial verification failed; starting one bounded repair cycle.");
    let repair_prompt = "Repair only defects introduced by the current bounded DTO split. Read the full git diff and this verification result: ";
    repair_prompt += if verification != () && verification.success { json_encode(verification.output) } else { "verification agent failed" };
    repair_prompt += ". Keep all fixes inside " + json_encode(targets) + " and their child modules; do not alter behavior or public API. ";
    repair_prompt += "Run targeted checks while repairing. Do not commit, reset, stash, or touch unrelated files. Report exact evidence.";
    let repair = agent(repair_prompt, #{
        label: "bounded-repair", capability_mode: "all", output_schema: implementation_schema,
    });
    let reverify_prompt = "Re-run an independent, fail-closed verification after repair. Inspect git diff and run from backend: ";
    reverify_prompt += "`cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`. ";
    reverify_prompt += "Check public names, re-exports, serde/validator attributes, conversions, normalization defaults, and test preservation for " + json_encode(targets) + ". ";
    reverify_prompt += "Set passed=true only if all commands really exit zero and no functional-shape drift is found. Do not edit files.";
    verification = agent(reverify_prompt, #{
        label: "full-reverification", capability_mode: "execute", output_schema: gate_schema,
    });
}

phase("Review");
let review_dimensions = [
    "plan adherence and file responsibility boundaries",
    "public API, serde, validator, conversion, and behavior-shape preservation",
    "Rust quality, visibility discipline, tests, and regression risk",
];
let review_jobs = [];
for dimension in review_dimensions {
    let review_prompt = "Independently review the current git diff for " + dimension + ". Read actual files, the plan " + json_encode(plan.output) + ", and verification evidence ";
    review_prompt += if verification != () && verification.success { json_encode(verification.output) } else { "missing verification" };
    review_prompt += ". Treat missing evidence as failure. Confirm no unrelated edits, no contract drift, no lost tests, and that each child file has a coherent responsibility. ";
    review_prompt += "Do not edit files. Set passed=true only with concrete file/diff evidence; list every finding, including blockers.";
    review_jobs.push(#{
        prompt: review_prompt,
        label: "review:" + dimension,
        capability_mode: "read-only",
        output_schema: review_schema,
    });
}
let reviews = parallel(review_jobs);
let reviews_passed = true;
for review in reviews {
    if review == () || !review.success || review.output.passed != true {
        reviews_passed = false;
    }
}
let gates_passed = verification != () && verification.success && verification.output.passed == true;
let final_status = if gates_passed && reviews_passed && implementation_failures.len() == 0 { "passed" } else { "failed" };

phase("Report");
let report_prompt = "Write a concise Chinese Markdown report for this backend refactor workflow. The hard-derived status is " + final_status + ". ";
report_prompt += "Summarize large-file analysis, bounded target choice, executed split plan, changed-file shape, baseline evidence, exact verification commands/results, and independent review findings. ";
report_prompt += "Never override a failed hard status or claim success without evidence. Analysis: " + json_encode(analysis.output) + ". Plan: " + json_encode(plan.output) + ". ";
report_prompt += "Implementations: " + json_encode(implementations) + ". Verification: ";
report_prompt += if verification != () && verification.success { json_encode(verification.output) } else { "missing" };
report_prompt += ". Reviews: " + json_encode(reviews) + ".";
let report = agent(report_prompt, #{
    label: "final-report", capability_mode: "read-only", output_schema: report_schema,
});
let report_text = if report != () && report.success { report.output.markdown } else { "报告代理失败；请以结构化结果中的门禁和 Review 证据为准。" };
let report_path = write_scratch_file("refactor-report.md", report_text);
complete(#{
    status: final_status,
    summary: if report != () && report.success { report.output.summary } else { "Workflow finished; inspect evidence." },
    report_path: report_path,
    report: report_text,
    baseline: baseline.output,
    analysis: analysis.output,
    plan: plan.output,
    implementations: implementations,
    implementation_failures: implementation_failures,
    verification: if verification != () && verification.success { verification.output } else { () },
    reviews: reviews,
});