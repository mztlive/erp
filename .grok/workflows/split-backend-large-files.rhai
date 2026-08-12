let meta = #{
    name: "split-backend-large-files",
    description: "Analyze and safely split oversized Rust backend files without changing behavior or existing tests",
    when_to_use: "Use when backend Rust files need responsibility-based splitting with baseline tests, lightweight implementation, and general-purpose review.",
    phases: [
        #{ title: "Discover", detail: "lightweight agent finds oversized backend files" },
        #{ title: "Baseline", detail: "lightweight agent proves the existing test baseline" },
        #{ title: "Analyze", detail: "general-purpose agent designs responsibility-based splits" },
        #{ title: "Implement", detail: "lightweight agents apply plans sequentially" },
        #{ title: "Review", detail: "general-purpose agents review every split" },
        #{ title: "Verify", detail: "full quality gates and final independent review" },
    ],
};

let discovery_schema = #{
    "type": "object",
    "required": ["candidates", "method"],
    "properties": #{
        "method": #{ "type": "string" },
        "candidates": #{
            "type": "array",
            "maxItems": 10,
            "items": #{
                "type": "object",
                "required": ["file", "lines", "responsibilities", "risk"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "lines": #{ "type": "integer" },
                    "responsibilities": #{ "type": "array", "maxItems": 10, "items": #{ "type": "string" } },
                    "risk": #{ "type": "string" },
                },
            },
        },
    },
};

let baseline_schema = #{
    "type": "object",
    "required": ["baseline_green", "commands", "summary"],
    "properties": #{
        "baseline_green": #{ "type": "boolean" },
        "commands": #{
            "type": "array",
            "maxItems": 8,
            "items": #{
                "type": "object",
                "required": ["command", "passed", "summary"],
                "properties": #{
                    "command": #{ "type": "string" },
                    "passed": #{ "type": "boolean" },
                    "summary": #{ "type": "string" },
                },
            },
        },
        "summary": #{ "type": "string" },
    },
};

let plans_schema = #{
    "type": "object",
    "required": ["plans", "analysis_summary"],
    "properties": #{
        "analysis_summary": #{ "type": "string" },
        "plans": #{
            "type": "array",
            "maxItems": 3,
            "items": #{
                "type": "object",
                "required": ["file", "objective", "target_structure", "symbol_moves", "invariants", "targeted_commands"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "objective": #{ "type": "string" },
                    "target_structure": #{ "type": "array", "maxItems": 12, "items": #{ "type": "string" } },
                    "symbol_moves": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
                    "invariants": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
                    "targeted_commands": #{ "type": "array", "maxItems": 10, "items": #{ "type": "string" } },
                },
            },
        },
    },
};

let implementation_schema = #{
    "type": "object",
    "required": ["completed", "summary", "files_changed", "commands"],
    "properties": #{
        "completed": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "files_changed": #{ "type": "array", "maxItems": 30, "items": #{ "type": "string" } },
        "commands": #{ "type": "array", "maxItems": 10, "items": #{ "type": "string" } },
    },
};

let review_schema = #{
    "type": "object",
    "required": ["approved", "behavior_preserved", "tests_unchanged", "scope_clean", "evidence", "issues"],
    "properties": #{
        "approved": #{ "type": "boolean" },
        "behavior_preserved": #{ "type": "boolean" },
        "tests_unchanged": #{ "type": "boolean" },
        "scope_clean": #{ "type": "boolean" },
        "evidence": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "issues": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
    },
};

let verification_schema = #{
    "type": "object",
    "required": ["all_pass", "tests_unchanged", "scope_clean", "commands", "summary"],
    "properties": #{
        "all_pass": #{ "type": "boolean" },
        "tests_unchanged": #{ "type": "boolean" },
        "scope_clean": #{ "type": "boolean" },
        "commands": #{
            "type": "array",
            "maxItems": 10,
            "items": #{
                "type": "object",
                "required": ["command", "passed", "summary"],
                "properties": #{
                    "command": #{ "type": "string" },
                    "passed": #{ "type": "boolean" },
                    "summary": #{ "type": "string" },
                },
            },
        },
        "summary": #{ "type": "string" },
    },
};

let root = "backend";
let standards = "/Users/huangjiajiang/.claude/skills/rust-coding-standards";
let smoke = if args == () { false } else { args.smoke };
if smoke == true {
    complete(#{ status: "validated", summary: "Smoke-check path compiled successfully." });
}

phase("Discover");
let discovery_prompt = "Work as a file-search specialist. Inspect only the backend/ subtree of the current repository. ";
discovery_prompt += "First read backend/AGENTS.md and the Rust file organization standard at ";
discovery_prompt += standards + "/references/file-organization.md. ";
discovery_prompt += "Use terminal file counting plus grep/read_file to find oversized Rust source files, excluding target/, generated artifacts, uploads, and frontend paths. ";
discovery_prompt += "Prioritize files above roughly 800 lines that mix multiple business use cases, query/command responsibilities, DTO groups, state transitions, or unrelated helpers. ";
discovery_prompt += "Read enough of every reported file to identify concrete responsibility groups; line count alone is not sufficient. ";
discovery_prompt += "Return at most 10 candidates, all as repository-relative paths beginning with backend/. Do not edit any file and do not inspect implementation outside backend except repository instructions.";
let discovery = agent(discovery_prompt, #{
    label: "lightweight:backend-file-search",
    agent_type: "lightweight",
    capability_mode: "read-only",
    output_schema: discovery_schema,
});
if discovery == () || !discovery.success {
    complete(#{ status: "blocked", summary: "Backend large-file discovery failed before any edits." });
}

let candidates = [];
for c in discovery.output.candidates {
    if c.file.starts_with("backend/") && !c.file.contains("..") {
        candidates.push(c);
    } else {
        log("Dropped out-of-scope discovery result: " + c.file);
    }
}
if candidates.len() == 0 {
    complete(#{ status: "no-op", summary: "No eligible oversized Rust files were found under backend/." });
}
log(candidates.len().to_string() + " backend candidates survived scope filtering");

phase("Baseline");
let baseline_prompt = "Establish the pre-refactor backend baseline without editing files. Work only in backend/. ";
baseline_prompt += "Read backend/AGENTS.md. Record git status, then run from backend: cargo fmt --all -- --check, cargo check --workspace, and cargo test --workspace. ";
baseline_prompt += "Do not fix anything, do not modify tests, do not create commits, and do not push. Mark baseline_green=true only if all three quality commands pass. ";
baseline_prompt += "If infrastructure or an external dependency prevents a command, report it as not passed with the exact concise reason.";
let baseline = agent(baseline_prompt, #{
    label: "lightweight:baseline-tests",
    agent_type: "lightweight",
    capability_mode: "execute",
    output_schema: baseline_schema,
});
if baseline == () || !baseline.success || !baseline.output.baseline_green {
    let blocked_report = "# Backend large-file split\n\nStatus: blocked before edits because the original backend baseline was not green.\n\n";
    if baseline != () && baseline.success {
        blocked_report += "```json\n" + json_encode(baseline.output) + "\n```\n";
    }
    let blocked_path = write_scratch_file("backend-large-file-split-report.md", blocked_report);
    complete(#{ status: "blocked", summary: "Original backend checks/tests did not pass; no refactor was attempted.", path: blocked_path });
}

phase("Analyze");
let analysis_prompt = "Act as the senior Rust refactoring architect. Analyze only backend/. Read backend/AGENTS.md, ";
analysis_prompt += standards + "/references/file-organization.md, and ";
analysis_prompt += standards + "/references/testing-review-gates.md. ";
analysis_prompt += "Independently inspect the actual candidate files with read_file/grep before planning. Candidate data: ";
analysis_prompt += json_encode(candidates) + ". ";
analysis_prompt += "Select at most 3 high-value, reasonably independent files that can be split safely in this run. Plan responsibility-based module extraction, never mechanical line slicing. ";
analysis_prompt += "The public API, type names, visibility contract, serialization, route behavior, database behavior, ordering, error semantics, logging semantics, and existing tests must remain unchanged. ";
analysis_prompt += "Existing test files and assertions must not be edited, removed, ignored, weakened, or replaced. Prefer moving existing impl blocks/functions/tests with minimal textual changes. ";
analysis_prompt += "For each plan, identify exact destination files, symbol groups, invariants, and targeted verification commands. Do not edit code.";
let analysis = agent(analysis_prompt, #{
    label: "general:split-architecture",
    agent_type: "general-purpose",
    capability_mode: "read-only",
    output_schema: plans_schema,
});
if analysis == () || !analysis.success {
    complete(#{ status: "blocked", summary: "General-purpose architectural analysis failed before edits." });
}

let plans = [];
for p in analysis.output.plans {
    let allowed = false;
    for c in candidates {
        if p.file == c.file { allowed = true; }
    }
    if allowed && p.file.starts_with("backend/") && !p.file.contains("..") {
        plans.push(p);
    } else {
        log("Dropped a plan that was not tied to an approved backend candidate: " + p.file);
    }
}
if plans.len() == 0 {
    complete(#{ status: "no-op", summary: "Analysis found no safe responsibility-based split to apply." });
}
log(plans.len().to_string() + " split plans approved for sequential implementation");

let implementation_summaries = [];
let review_summaries = [];
let blocked = false;
let blocked_reason = "";

let plan_index = 0;
for p in plans {
    phase("Implement");
    let implementation_prompt = "Implement this approved backend-only Rust file split: " + json_encode(p) + ". ";
    implementation_prompt += "Read backend/AGENTS.md and " + standards + "/references/file-organization.md before editing. ";
    implementation_prompt += "Use grep/read_file for symbol and call-site search, then edit only backend/ files required by this exact split. ";
    implementation_prompt += "Preserve functionality exactly: no public API changes, no business-rule changes, no DTO/serde shape changes, no query/filter/order changes, no error/logging behavior changes, and no dependency changes. ";
    implementation_prompt += "Do not edit existing tests, snapshots, assertions, fixtures, ignore attributes, or test configuration. Move tests with their owning implementation only when Rust module mechanics require it, without changing test bodies. ";
    implementation_prompt += "Keep mod.rs focused on declarations/types/re-exports and split by business responsibility. Preserve visibility as narrowly as possible. ";
    implementation_prompt += "All added or modified methods must satisfy the repository documentation-comment rules. Prefer pure moves over rewrites. ";
    implementation_prompt += "Run cargo fmt --all and the plan's targeted checks from backend. Do not commit or push. Return completed=false if exact behavior preservation cannot be established.";
    let implementation = agent(implementation_prompt, #{
        label: "lightweight:implement:" + plan_index.to_string(),
        agent_type: "lightweight",
        capability_mode: "all",
        output_schema: implementation_schema,
    });
    if implementation == () || !implementation.success || !implementation.output.completed {
        blocked = true;
        blocked_reason = "Lightweight implementation failed for " + p.file;
        break;
    }
    implementation_summaries.push(implementation.output);

    phase("Review");
    let review_prompt = "Perform an independent, adversarial code review of the current working tree for the split of " + p.file + ". ";
    review_prompt += "Read backend/AGENTS.md, " + standards + "/references/file-organization.md, and " + standards + "/references/testing-review-gates.md. ";
    review_prompt += "Inspect git diff and actual moved code/call sites. Compare the approved plan " + json_encode(p) + " against the implementation summary " + json_encode(implementation.output) + ". ";
    review_prompt += "Fail closed unless you can provide concrete evidence that behavior shape is preserved, public APIs and visibility contracts remain compatible, no logic was dropped/duplicated, module initialization/order is unchanged, and existing tests were not semantically changed. ";
    review_prompt += "Also inspect git diff --name-only and fail scope_clean if any changed path is outside backend/. Do not edit files.";
    let review = agent(review_prompt, #{
        label: "general:review:" + plan_index.to_string(),
        agent_type: "general-purpose",
        capability_mode: "read-only",
        output_schema: review_schema,
    });
    if review == () || !review.success {
        blocked = true;
        blocked_reason = "General-purpose review failed for " + p.file;
        break;
    }

    if !review.output.approved {
        phase("Implement");
        let repair_prompt = "Repair only the issues found by the independent reviewer for the current backend split. Review findings: ";
        repair_prompt += json_encode(review.output) + ". ";
        repair_prompt += "Keep the original approved plan and exact behavior-preservation constraints. Inspect current diff before editing. ";
        repair_prompt += "Do not change existing test semantics, do not touch paths outside backend/, do not add dependencies, and do not commit or push. Run cargo fmt and targeted checks after repairs.";
        let repair = agent(repair_prompt, #{
            label: "lightweight:repair:" + plan_index.to_string(),
            agent_type: "lightweight",
            capability_mode: "all",
            resume_from: implementation.agent_id,
            output_schema: implementation_schema,
        });
        if repair == () || !repair.success || !repair.output.completed {
            blocked = true;
            blocked_reason = "Lightweight repair failed for " + p.file;
            break;
        }
        implementation_summaries.push(repair.output);

        phase("Review");
        let rereview_prompt = "Re-review the repaired split of " + p.file + " independently. Original plan: " + json_encode(p) + ". ";
        rereview_prompt += "Previous findings: " + json_encode(review.output) + ". Inspect the actual current git diff and code. ";
        rereview_prompt += "Approve only with concrete evidence for behavior preservation, unchanged existing test semantics, and backend-only scope. Do not edit files.";
        let rereview = agent(rereview_prompt, #{
            label: "general:rereview:" + plan_index.to_string(),
            agent_type: "general-purpose",
            capability_mode: "read-only",
            output_schema: review_schema,
        });
        if rereview == () || !rereview.success || !rereview.output.approved {
            blocked = true;
            blocked_reason = "The repaired split did not pass general-purpose review for " + p.file;
            if rereview != () && rereview.success { review_summaries.push(rereview.output); }
            break;
        }
        review_summaries.push(rereview.output);
    } else {
        review_summaries.push(review.output);
    }
    plan_index += 1;
}

if blocked {
    let partial_report = "# Backend large-file split\n\nStatus: blocked during implementation/review.\n\nReason: " + blocked_reason + "\n\n";
    partial_report += "## Plans\n\n```json\n" + json_encode(plans) + "\n```\n\n";
    partial_report += "## Implementations\n\n```json\n" + json_encode(implementation_summaries) + "\n```\n\n";
    partial_report += "## Reviews\n\n```json\n" + json_encode(review_summaries) + "\n```\n";
    let partial_path = write_scratch_file("backend-large-file-split-report.md", partial_report);
    complete(#{ status: "blocked", summary: blocked_reason, path: partial_path });
}

phase("Verify");
let verification_prompt = "Run the final backend quality gate and inspect scope without changing files. Work from the repository root and backend/. ";
verification_prompt += "First inspect git diff --name-only and git diff -- backend. Confirm every changed path is under backend/ and that existing tests, assertions, fixtures, snapshots, ignore attributes, and test configuration have not been semantically modified. ";
verification_prompt += "Then run from backend: cargo fmt --all -- --check; cargo check --workspace; cargo clippy --workspace --all-targets --all-features -- -D warnings; cargo test --workspace. ";
verification_prompt += "Set all_pass=true only if every command passes. Set tests_unchanged and scope_clean only with diff evidence. Do not fix, commit, or push.";
let verification = agent(verification_prompt, #{
    label: "lightweight:full-quality-gate",
    agent_type: "lightweight",
    capability_mode: "execute",
    output_schema: verification_schema,
});
if verification == () || !verification.success {
    complete(#{ status: "blocked", summary: "Final lightweight quality-gate execution failed." });
}

if !verification.output.all_pass || !verification.output.tests_unchanged || !verification.output.scope_clean {
    phase("Implement");
    let final_repair_prompt = "Fix only refactor-induced failures reported by the final backend verification: " + json_encode(verification.output) + ". ";
    final_repair_prompt += "Inspect the current diff and command output. Preserve behavior exactly, keep all changes under backend/, and do not modify existing test semantics or dependencies. ";
    final_repair_prompt += "Run cargo fmt and the failing commands. Do not commit or push. If a failure is environmental or pre-existing, do not mask it; report completed=false.";
    let final_repair = agent(final_repair_prompt, #{
        label: "lightweight:final-repair",
        agent_type: "lightweight",
        capability_mode: "all",
        output_schema: implementation_schema,
    });
    if final_repair == () || !final_repair.success || !final_repair.output.completed {
        complete(#{ status: "blocked", summary: "Final verification failed and could not be safely repaired.", verification: verification.output });
    }
    implementation_summaries.push(final_repair.output);

    phase("Verify");
    verification = agent(verification_prompt, #{
        label: "lightweight:quality-gate-rerun",
        agent_type: "lightweight",
        capability_mode: "execute",
        output_schema: verification_schema,
    });
    if verification == () || !verification.success || !verification.output.all_pass || !verification.output.tests_unchanged || !verification.output.scope_clean {
        complete(#{ status: "blocked", summary: "Backend refactor did not pass the complete final quality gate.", verification: if verification == () { () } else { verification.output } });
    }
}

let final_review_prompt = "Conduct the final independent acceptance review for the backend-only large-file split. ";
final_review_prompt += "Read backend/AGENTS.md and both Rust standards for file organization and testing/review. Inspect the complete git diff, not summaries alone. ";
final_review_prompt += "Plans: " + json_encode(plans) + ". Verification evidence: " + json_encode(verification.output) + ". ";
final_review_prompt += "Fail closed unless the refactor is responsibility-based, functionality shape is unchanged, public APIs/serde/database/error/logging behavior remain compatible, no code was lost or duplicated, existing tests are semantically unchanged, all changes are under backend/, and all quality gates passed. ";
final_review_prompt += "Do not edit files.";
let final_review = agent(final_review_prompt, #{
    label: "general:final-acceptance-review",
    agent_type: "general-purpose",
    capability_mode: "read-only",
    output_schema: review_schema,
});
if final_review == () || !final_review.success {
    complete(#{ status: "blocked", summary: "Final general-purpose acceptance review failed to execute." });
}

let final_status = if final_review.output.approved { "completed" } else { "blocked" };
let report = "# Backend large-file split\n\nStatus: " + final_status + "\n\n";
report += "## Scope\n\nOnly `backend/` was allowed. Existing test semantics were required to remain unchanged.\n\n";
report += "## Baseline\n\n```json\n" + json_encode(baseline.output) + "\n```\n\n";
report += "## Analysis and plans\n\n```json\n" + json_encode(analysis.output) + "\n```\n\n";
report += "## Implementation summaries\n\n```json\n" + json_encode(implementation_summaries) + "\n```\n\n";
report += "## Per-split reviews\n\n```json\n" + json_encode(review_summaries) + "\n```\n\n";
report += "## Final verification\n\n```json\n" + json_encode(verification.output) + "\n```\n\n";
report += "## Final general-purpose review\n\n```json\n" + json_encode(final_review.output) + "\n```\n";
let report_path = write_scratch_file("backend-large-file-split-report.md", report);
complete(#{
    status: final_status,
    summary: if final_review.output.approved { "Backend large-file splits passed baseline comparison, full quality gates, and independent review." } else { "Backend changes remain blocked by the final independent review." },
    path: report_path,
    final_review: final_review.output,
});
