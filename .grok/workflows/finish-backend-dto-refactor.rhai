let meta = #{
    name: "finish-backend-dto-refactor",
    description: "Finish the failed DTO split batch, run fail-closed Rust gates, and independently review contract preservation",
    phases: [
        #{ title: "Repair", detail: "finish catalog split and restore exact visibility" },
        #{ title: "Verify", detail: "run full workspace gates and contract checks" },
        #{ title: "Review", detail: "independent plan and behavior review" },
        #{ title: "Report", detail: "produce an evidence-based result" },
    ],
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
let gate_schema = #{
    "type": "object", "required": ["passed", "commands", "evidence", "failures"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "commands": #{ "type": "array", "items": #{ "type": "string" } },
        "evidence": #{ "type": "string" },
        "failures": #{ "type": "array", "items": #{ "type": "string" } },
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

phase("Repair");
let repair_prompt = "Work in the current shared repository. The first workflow partially completed a behavior-preserving DTO split. Read backend/AGENTS.md, ";
repair_prompt += "/Users/huangjiajiang/.claude/skills/rust-coding-standards/references/file-organization.md, rust-code-performance.md, testing-review-gates.md, and docs/split/backend/services/src/catalog/dto.rs/plan.md. ";
repair_prompt += "Inspect the current git diff before editing. Finish the exact approved split of backend/services/src/catalog/dto.rs into a thin facade plus dto/common.rs, category.rs, brand_unit.rs, attribute.rs, product.rs, and voucher.rs. ";
repair_prompt += "Move declarations, existing tests, normalize implementations, validator functions, constants, and From conversions without changing tokens or behavior except import/module paths. Preserve catalog root exports and all crate::catalog::dto imports. ";
repair_prompt += "Also restore the exact original facade visibility for integration_ops PageParams and SortDir by publicly re-exporting them from its private dto facade; keep normalize_sort crate-visible. ";
repair_prompt += "Do not invent new APIs, DTOs, business rules, tests that claim unavailable behavior, or unrelated cleanup. Do not commit, reset, stash, or modify files outside the three DTO modules and their existing docs/split plans. ";
repair_prompt += "Use responsibility-named private child modules, minimal visibility, and run cargo fmt plus targeted services checks/tests. Report exact files and real command evidence.";
let implementation = agent(repair_prompt, #{
    label: "finish-dto-splits", capability_mode: "all", output_schema: implementation_schema,
});

phase("Verify");
let verify_prompt = "Fail-closed verification of the current backend DTO refactor. Read backend/AGENTS.md, all three docs/split plans, HEAD versions of the original dto.rs files, and the full git diff. ";
verify_prompt += "Confirm all three original >=1000-line DTO files are now thin facades with cohesive child files below 800 lines. Confirm no DTO/type/field/derive/serde/validator/default/normalization/From conversion/test was lost or behaviorally changed. ";
verify_prompt += "Confirm exact root and dto-module visibility/re-export shape is preserved, especially integration_ops PageParams/SortDir and public mall_sync::dto. Confirm no unrelated production edits. ";
verify_prompt += "Run actual commands: `git diff --check`; from backend `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `cargo test --workspace`. ";
verify_prompt += "Also run targeted services DTO tests and line counts. Set passed=true only if every required command exits zero and manual contract comparison finds no drift. Do not edit files and never fabricate success.";
let verification = agent(verify_prompt, #{
    label: "dto-full-verification", capability_mode: "execute", output_schema: gate_schema,
});

if verification == () || !verification.success || verification.output.passed != true {
    let correction_prompt = "Repair only failures introduced by this DTO split, using this verifier evidence: ";
    correction_prompt += if verification != () && verification.success { json_encode(verification.output) } else { "verification agent failed" };
    correction_prompt += ". Inspect HEAD and current diff. Keep public/serde/validator/normalization behavior exact and edits within the three DTO modules. Run targeted checks. Do not commit/reset/stash.";
    let correction = agent(correction_prompt, #{
        label: "dto-correction", capability_mode: "all", output_schema: implementation_schema,
    });
    let reverify_prompt = "Re-run fail-closed verification after correction. Inspect HEAD versus current declarations and full diff; run `git diff --check` and from backend `cargo fmt --all -- --check`, `cargo check --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo test --workspace`, targeted DTO tests, and line counts. ";
    reverify_prompt += "Require all three DTO facades complete, each child below 800 lines, exact API/serde/validator/normalization/conversion/test preservation, and no unrelated production edits. Do not edit files. Set passed=true only with real zero exits and concrete comparison evidence.";
    verification = agent(reverify_prompt, #{
        label: "dto-reverification", capability_mode: "execute", output_schema: gate_schema,
    });
}

phase("Review");
let dimensions = [
    "approved split-plan adherence and coherent file boundaries",
    "public and crate-visible API plus serde-validator behavior preservation",
    "test preservation, Rust quality, and regression risk",
];
let jobs = [];
for dimension in dimensions {
    let prompt = "Independently review the complete current git diff for " + dimension + ". Read actual current files, their HEAD versions, backend/AGENTS.md, and all three docs/split plans. ";
    prompt += "Use verification evidence: " + if verification != () && verification.success { json_encode(verification.output) } else { "missing" };
    prompt += ". Treat missing evidence as failure. Confirm all three targets are implemented, no behavior shape changed, no tests disappeared, visibility is exact, and no unrelated production edits exist. Do not edit files.";
    jobs.push(#{ prompt: prompt, label: "review:" + dimension, capability_mode: "read-only", output_schema: review_schema });
}
let reviews = parallel(jobs);
let reviews_passed = true;
for review in reviews {
    if review == () || !review.success || review.output.passed != true { reviews_passed = false; }
}
let gates_passed = verification != () && verification.success && verification.output.passed == true;
let implementation_passed = implementation != () && implementation.success && implementation.output.changed == true;
let status = if gates_passed && reviews_passed && implementation_passed { "passed" } else { "failed" };

phase("Report");
let report = "# DTO 大文件拆分收尾报告\n\n";
report += "最终状态：**" + status + "**\n\n";
report += "## 实施\n\n" + if implementation != () && implementation.success { implementation.output.summary + "\n\n" + implementation.output.evidence } else { "实施代理失败。" };
report += "\n\n## 验证\n\n" + if verification != () && verification.success { verification.output.evidence } else { "缺少有效验证结果。" };
report += "\n\n## Review\n\n" + json_encode(reviews);
let path = write_scratch_file("finish-dto-refactor-report.md", report);
complete(#{
    status: status,
    report_path: path,
    report: report,
    implementation: if implementation != () && implementation.success { implementation.output } else { () },
    verification: if verification != () && verification.success { verification.output } else { () },
    reviews: reviews,
});