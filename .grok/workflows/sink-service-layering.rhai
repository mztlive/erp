let meta = #{
    name: "sink-service-layering",
    description: "Scan backend services for logic that belongs in repositories or entities, apply safe refactors, and verify the result",
    when_to_use: "Use when backend/services needs a full layering audit and behavior-preserving repository/entity extraction.",
    phases: [
        #{ title: "Scan", detail: "parallel domain-by-domain evidence scan" },
        #{ title: "Verify", detail: "independent confirmation and safe candidate selection" },
        #{ title: "Adjust", detail: "parallel non-overlapping layering refactors" },
        #{ title: "Review", detail: "architecture and correctness review" },
        #{ title: "Quality", detail: "integrate fixes and run Rust quality gates" },
    ],
};

let finding_schema = #{
    "type": "object",
    "required": [
        "id", "source_file", "symbol", "target_layer", "proposed_target",
        "reason", "evidence", "proposed_change", "risk"
    ],
    "properties": #{
        "id": #{ "type": "string" },
        "source_file": #{ "type": "string" },
        "symbol": #{ "type": "string" },
        "target_layer": #{ "type": "string" },
        "proposed_target": #{ "type": "string" },
        "reason": #{ "type": "string" },
        "evidence": #{ "type": "string" },
        "proposed_change": #{ "type": "string" },
        "risk": #{ "type": "string" },
    },
};

let scan_schema = #{
    "type": "object",
    "required": [
        "shard", "files_reviewed", "repository_count", "entities_count",
        "findings", "truncated", "summary"
    ],
    "properties": #{
        "shard": #{ "type": "string" },
        "files_reviewed": #{ "type": "integer" },
        "repository_count": #{ "type": "integer" },
        "entities_count": #{ "type": "integer" },
        "findings": #{ "type": "array", "maxItems": 16, "items": finding_schema },
        "truncated": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
    },
};

let verified_finding_schema = #{
    "type": "object",
    "required": [
        "id", "source_file", "symbol", "target_layer", "proposed_target",
        "reason", "evidence", "proposed_change", "risk", "confidence"
    ],
    "properties": #{
        "id": #{ "type": "string" },
        "source_file": #{ "type": "string" },
        "symbol": #{ "type": "string" },
        "target_layer": #{ "type": "string" },
        "proposed_target": #{ "type": "string" },
        "reason": #{ "type": "string" },
        "evidence": #{ "type": "string" },
        "proposed_change": #{ "type": "string" },
        "risk": #{ "type": "string" },
        "confidence": #{ "type": "string" },
    },
};

let verify_schema = #{
    "type": "object",
    "required": [
        "shard", "repository_count", "entities_count", "selected",
        "rejected_count", "truncated", "summary"
    ],
    "properties": #{
        "shard": #{ "type": "string" },
        "repository_count": #{ "type": "integer" },
        "entities_count": #{ "type": "integer" },
        "selected": #{ "type": "array", "maxItems": 3, "items": verified_finding_schema },
        "rejected_count": #{ "type": "integer" },
        "truncated": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
    },
};

let implementation_schema = #{
    "type": "object",
    "required": [
        "shard", "applied_repository_count", "applied_entities_count",
        "skipped_count", "files_changed", "checks_run", "summary"
    ],
    "properties": #{
        "shard": #{ "type": "string" },
        "applied_repository_count": #{ "type": "integer" },
        "applied_entities_count": #{ "type": "integer" },
        "skipped_count": #{ "type": "integer" },
        "files_changed": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "checks_run": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

let review_schema = #{
    "type": "object",
    "required": ["dimension", "passed", "issues", "summary"],
    "properties": #{
        "dimension": #{ "type": "string" },
        "passed": #{ "type": "boolean" },
        "issues": #{
            "type": "array", "maxItems": 20, "items": #{
                "type": "object",
                "required": ["file", "issue", "severity", "evidence", "fix"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "issue": #{ "type": "string" },
                    "severity": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                    "fix": #{ "type": "string" },
                },
            },
        },
        "summary": #{ "type": "string" },
    },
};

let quality_schema = #{
    "type": "object",
    "required": ["passed", "commands", "files_changed", "remaining_issues", "summary"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "commands": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "files_changed": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "remaining_issues": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

fn path_in_scope(path, prefixes) {
    if type_of(path) != "string" { return false; }
    for prefix in prefixes {
        if path.starts_with(prefix) { return true; }
    }
    false
}

fn path_is_dirty(path, dirty_paths) {
    if type_of(path) != "string" { return true; }
    for dirty in dirty_paths {
        if path == dirty || path.starts_with(dirty) { return true; }
    }
    false
}

fn integer_or_zero(value) {
    if type_of(value) == "i64" { value } else { 0 }
}

fn boolean_or_false(value) {
    if type_of(value) == "bool" { value } else { false }
}

let root = if args == () { () } else { args.root };
if root == () { pause("verification", "Pass args.root, for example backend/services."); }
let apply_changes = if args.apply == () { true } else { args.apply };
let dirty_paths = if args.dirty_paths == () { [] } else { args.dirty_paths };

let shards = [
    #{
        name: "access-and-core",
        prefixes: [
            "backend/services/src/access_control/", "backend/services/src/audit/",
            "backend/services/src/auth/", "backend/services/src/bulk_job/",
            "backend/services/src/card_instance/", "backend/services/src/document_registry/",
            "backend/services/src/file_asset/", "backend/services/src/iam/",
            "backend/services/src/source_registry/", "backend/services/src/warehouse/",
            "backend/services/src/account_support.rs", "backend/services/src/approval_action_registry.rs",
            "backend/services/src/owned_task.rs", "backend/services/src/page.rs",
            "backend/services/src/pending_file_assets.rs", "backend/services/src/query.rs",
            "backend/services/src/transaction.rs"
        ],
    },
    #{ name: "approval", prefixes: ["backend/services/src/approval/"] },
    #{
        name: "catalog-customer-party",
        prefixes: [
            "backend/services/src/catalog/", "backend/services/src/customer/",
            "backend/services/src/inventory/", "backend/services/src/party/"
        ],
    },
    #{
        name: "fulfillment-and-returns",
        prefixes: ["backend/services/src/fulfillment/", "backend/services/src/returns/"],
    },
    #{ name: "purchase-order", prefixes: ["backend/services/src/purchase_order/"] },
    #{
        name: "sales",
        prefixes: ["backend/services/src/sales_order/", "backend/services/src/sales_review/"],
    },
    #{
        name: "finance",
        prefixes: [
            "backend/services/src/cost/", "backend/services/src/payable/",
            "backend/services/src/receivable/"
        ],
    },
    #{
        name: "integration-and-projection",
        prefixes: [
            "backend/services/src/integration_ops/", "backend/services/src/legacy_import/",
            "backend/services/src/procurement_responsibility/", "backend/services/src/projection/",
            "backend/services/src/publication/"
        ],
    },
    #{
        name: "mall",
        prefixes: [
            "backend/services/src/mall_after_sales/", "backend/services/src/mall_backfill/",
            "backend/services/src/mall_order/", "backend/services/src/mall_sync/"
        ],
    },
    #{
        name: "supplier",
        prefixes: [
            "backend/services/src/supplier/", "backend/services/src/supplier_api/",
            "backend/services/src/supplier_fulfillment/", "backend/services/src/supplier_offering/",
            "backend/services/src/supplier_settlement/"
        ],
    },
    #{ name: "work-item", prefixes: ["backend/services/src/work_item/"] },
];

phase("Scan");
let scan_jobs = [];
for shard in shards {
    let prompt = "Audit the Rust service code in this exact shard for misplaced layering logic. ";
    prompt += "Repository target means only persistence/query mechanics or reusable data-fact queries; never move business decisions, transaction boundaries, or orchestration into a repository. ";
    prompt += "Entities target means pure domain invariants, state/type rules, normalization, deterministic construction, matching, deduplication, sorting, prioritization, or validation that needs no database/external I/O. ";
    prompt += "DTO protocol normalization and response-view formatting are not entity findings unless they encode a stable domain rule. BPM state-engine logic must stay in bpm rather than entities. ";
    prompt += "Use grep and read_file to inspect every Rust file under the listed prefixes and inspect matching backend/entities and backend/database repository code before deciding. Do not answer from names alone. ";
    prompt += "Count each independently movable function or coherent logic block once. Report the total repository_count and entities_count even if the detailed findings list is capped at 16; set truncated=true when details are omitted. ";
    prompt += "Every finding needs an exact source file, symbol, concrete code evidence, target type/path, behavior-preserving change, and risk. An empty result is valid only after reading every file in scope. ";
    prompt += "This phase is read-only. Scope name: " + shard.name + ". Allowed service prefixes: " + json_encode(shard.prefixes) + ". Dirty paths to note but still scan read-only: " + json_encode(dirty_paths) + ".";
    scan_jobs.push(#{
        prompt: prompt,
        label: "scan:" + shard.name,
        capability_mode: "read-only",
        output_schema: scan_schema,
    });
}
let scan_results = parallel(scan_jobs);

phase("Verify");
let verify_jobs = [];
let scan_index = 0;
for shard in shards {
    let scan_payload = "scanner unavailable";
    let scan_result = scan_results[scan_index];
    if scan_result != () && scan_result.success {
        scan_payload = json_encode(scan_result.output);
    }
    let prompt = "Independently verify a layering scan for the Rust service shard " + shard.name + ". ";
    prompt += "Read the shipped code yourself with grep/read_file, including relevant entities and database repositories. Treat the scanner payload only as hypotheses and actively reject false positives. ";
    prompt += "Use the strict boundaries: repository owns data access mechanics/fact queries but no business rule or transaction boundary; entities/value objects own reusable pure domain rules with no I/O; service keeps orchestration, transaction coordination, repository-result-dependent decisions, query/filter composition, and view mapping. ";
    prompt += "Compute repository_count and entities_count as the independently confirmed total for the entire shard, not merely the number selected for editing. ";
    prompt += "Select at most 3 highest-confidence, behavior-preserving, domain-contained candidates for immediate adjustment. Prefer existing entity/repository types and candidates with clear unit-test seams. Exclude anything whose source is in the dirty paths, anything requiring a database migration/public API change, and anything that would touch shared cross-shard root modules. ";
    prompt += "Set truncated=true when confirmed candidates exceed the selected list. Evidence must name the inspected code. Allowed prefixes: " + json_encode(shard.prefixes) + ". Dirty paths: " + json_encode(dirty_paths) + ". Scanner payload: " + scan_payload + ".";
    verify_jobs.push(#{
        prompt: prompt,
        label: "verify:" + shard.name,
        capability_mode: "read-only",
        output_schema: verify_schema,
    });
    scan_index += 1;
}
let verify_results = parallel(verify_jobs);

let confirmed_repository_total = 0;
let confirmed_entities_total = 0;
let selected_total = 0;
let filtered_plans = [];
let verify_index = 0;
for shard in shards {
    let selected = [];
    let verified = verify_results[verify_index];
    if verified != () && verified.success {
        confirmed_repository_total += integer_or_zero(verified.output.repository_count);
        confirmed_entities_total += integer_or_zero(verified.output.entities_count);
        if type_of(verified.output.selected) == "array" {
            for finding in verified.output.selected {
                if path_in_scope(finding.source_file, shard.prefixes)
                    && !path_is_dirty(finding.source_file, dirty_paths) {
                    selected.push(finding);
                    selected_total += 1;
                } else if type_of(finding.source_file) == "string" {
                    log("Dropped out-of-scope or dirty-path candidate: " + finding.source_file);
                }
            }
        }
    }
    filtered_plans.push(#{ name: shard.name, prefixes: shard.prefixes, selected: selected });
    verify_index += 1;
}
log(confirmed_repository_total.to_string() + " repository and " + confirmed_entities_total.to_string() + " entity candidates independently confirmed; " + selected_total.to_string() + " selected for safe adjustment");

let implementation_results = [];
if apply_changes && selected_total > 0 {
    phase("Adjust");
    let implementation_jobs = [];
    for plan in filtered_plans {
        if plan.selected.len() > 0 {
            let prompt = "Apply only the verified layering candidates in this plan to the Rust workspace. Before editing, inspect git status and the actual source. ";
            prompt += "Never use git reset, checkout, restore, clean, or any command that discards work. Preserve all pre-existing changes and do not modify any dirty path, especially backend/services/src/customer/profile.rs and backend/services/src/customer/profile/**. ";
            prompt += "For an entities candidate, move the pure rule into the matching entity/value object, remove the service duplicate, call the domain method from service, and add direct unit tests covering normal, failure, and boundary behavior as applicable. ";
            prompt += "For a repository candidate, add a narrowly named data-access or data-fact query to the existing matching repository, keep business meaning and transaction boundaries in service, accept executor as the final parameter, and avoid raw MongoDB in service. ";
            prompt += "Keep public APIs, serialized shapes, errors, database schemas, and behavior unchanged. Modify only service files under the plan prefixes and corresponding domain-owned backend/entities or backend/database repository modules; skip candidates that require shared cross-shard root edits or prove unsafe. ";
            prompt += "Every added or modified method, public or private, must have a multi-line Rust doc comment covering purpose, parameters, return, errors, and key constraints. Do not run whole-workspace gates; run only focused checks that do not interfere with other parallel editors. ";
            prompt += "Return exact applied counts, skipped count, changed files, and commands actually run. Shard: " + plan.name + ". Prefixes: " + json_encode(plan.prefixes) + ". Verified plan: " + json_encode(plan.selected) + ". Dirty paths: " + json_encode(dirty_paths) + ".";
            implementation_jobs.push(#{
                prompt: prompt,
                label: "adjust:" + plan.name,
                capability_mode: "all",
                output_schema: implementation_schema,
            });
        }
    }
    implementation_results = parallel(implementation_jobs);
}

let applied_repository_total = 0;
let applied_entities_total = 0;
let skipped_total = 0;
for result in implementation_results {
    if result != () && result.success {
        applied_repository_total += integer_or_zero(result.output.applied_repository_count);
        applied_entities_total += integer_or_zero(result.output.applied_entities_count);
        skipped_total += integer_or_zero(result.output.skipped_count);
    }
}

phase("Review");
let review_jobs = [];
let review_dimensions = [
    "layering architecture, repository purity, entity ownership, transaction boundaries, and required documentation/tests",
    "behavior preservation, error/API compatibility, compile risks, duplicate rule sources, and accidental edits outside the selected domains"
];
for dimension in review_dimensions {
    let prompt = "Review the current git diff after the service-layering workflow for " + dimension + ". ";
    prompt += "Use git diff, grep, and read_file to inspect actual changes. Exclude the pre-existing dirty paths from judgments unless workflow changes clearly damaged them. ";
    prompt += "Fail closed: passed=true only when there is no concrete blocker. Report exact file evidence and a specific fix for every issue. Do not edit files. ";
    prompt += "Pre-existing dirty paths: " + json_encode(dirty_paths) + ". Applied result summaries: " + json_encode(implementation_results) + ".";
    review_jobs.push(#{
        prompt: prompt,
        label: "review:" + dimension,
        capability_mode: "read-only",
        output_schema: review_schema,
    });
}
let review_results = parallel(review_jobs);

phase("Quality");
let quality_prompt = "Integrate and verify the current Rust layering refactor. Inspect git status and diff first. Preserve all pre-existing user edits; never use git reset, checkout, restore, clean, or revert dirty paths. ";
quality_prompt += "Read the review findings, fix only concrete problems introduced by this workflow, and keep repository/entity/service boundaries strict. Ensure every added or modified method has the required multi-line documentation and entity rules have direct unit tests. ";
quality_prompt += "Then run from backend: cargo fmt --all; cargo check -p entities -p database -p services; cargo clippy -p entities -p database -p services --all-targets --all-features -- -D warnings; cargo test -p entities -p services --lib; ./scripts/check-service-boundaries.sh; ./scripts/check-bpm-boundaries.sh. ";
quality_prompt += "If a command fails because of a workflow change, fix and rerun it. If it fails for a verified pre-existing or environmental reason, do not hide it: record the exact blocker. Do not make database migrations or public API changes. ";
quality_prompt += "Dirty paths that must not be semantically changed: " + json_encode(dirty_paths) + ". Review findings: " + json_encode(review_results) + ".";
let quality_result = agent(quality_prompt, #{
    label: "quality:integrate-and-verify",
    capability_mode: "all",
    output_schema: quality_schema,
});

let quality_passed = quality_result != ()
    && quality_result.success
    && boolean_or_false(quality_result.output.passed);
let report = "# Service layering workflow report\n\n";
report += "- Confirmed repository candidates: " + confirmed_repository_total.to_string() + "\n";
report += "- Confirmed entity/value-object candidates: " + confirmed_entities_total.to_string() + "\n";
report += "- Selected safe candidates: " + selected_total.to_string() + "\n";
report += "- Applied repository adjustments: " + applied_repository_total.to_string() + "\n";
report += "- Applied entity/value-object adjustments: " + applied_entities_total.to_string() + "\n";
report += "- Skipped during implementation: " + skipped_total.to_string() + "\n";
report += "- Quality gates passed: " + quality_passed.to_string() + "\n\n";
report += "## Independent verification\n\n```json\n" + json_encode(verify_results) + "\n```\n\n";
report += "## Implementation\n\n```json\n" + json_encode(implementation_results) + "\n```\n\n";
report += "## Review\n\n```json\n" + json_encode(review_results) + "\n```\n\n";
report += "## Quality\n\n```json\n" + json_encode(quality_result) + "\n```\n";
let report_path = write_scratch_file("service-layering-report.md", report);

complete(#{
    summary: confirmed_repository_total.to_string() + " repository and " + confirmed_entities_total.to_string() + " entity candidates confirmed; " + applied_repository_total.to_string() + " repository and " + applied_entities_total.to_string() + " entity adjustments applied",
    report_path: report_path,
    confirmed_repository_count: confirmed_repository_total,
    confirmed_entities_count: confirmed_entities_total,
    selected_count: selected_total,
    applied_repository_count: applied_repository_total,
    applied_entities_count: applied_entities_total,
    skipped_count: skipped_total,
    quality_passed: quality_passed,
});
