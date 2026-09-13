// Execution contract and coverage matrix: complete-org-data-scope.md.
// Dependency-ready groups use bounded parallel panels; shared writes and gates are barriers.
let meta = #{
    name: "complete-org-data-scope",
    description: "Implement selected S2/S3 scope with dependency gates, audited read/write manifests, bounded parallel repair and independent final verification; handover and S4 require explicit selection",
    when_to_use: "Local implementation and evidence collection only; never a production acceptance or deployment gate.",
    phases: [
        #{title: "Inventory", detail: "complete requirement and evidence ledger"},
        #{title: "Plan", detail: "parallel plans for dependency-ready groups"},
        #{title: "Implement", detail: "bounded parallel writers with disjoint read/write footprints"},
        #{title: "Merge", detail: "shared code and API contracts before review"},
        #{title: "LocalChecks", detail: "focused checks after every writer and merge complete"},
        #{title: "Review", detail: "independent batch review"},
        #{title: "Repair", detail: "bounded repairs followed by fresh checks"},
        #{title: "Quality", detail: "all required local gates; no source edits"},
        #{title: "Verify", detail: "final snapshot and per-ID local verdict"},
        #{title: "Docs", detail: "delivery status only; no production code changes"},
        #{title: "Report", detail: "local status, held items and external acceptance"},
    ],
};

let inventory_schema = #{"type": "object", "required": ["items", "unmapped_requirements", "summary"], "properties": #{"items": #{"type": "array", "items": #{"type": "object", "required": ["id", "state", "evidence", "pending_acceptance"], "properties": #{"id": #{"type": "string", "minLength": 1}, "state": #{"type": "string", "enum": ["code_gap", "local_ready", "acceptance_pending", "blocked"]}, "evidence": #{"type": "string", "minLength": 1}, "pending_acceptance": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}}, "additionalProperties": false}}, "unmapped_requirements": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "summary": #{"type": "string", "minLength": 1}}, "additionalProperties": false};
let plan_schema = #{"type": "object", "required": ["group", "implementation_ids", "ready_ids", "blocked_ids", "dependencies_ready", "files_to_touch", "shared_files", "read_files", "evidence", "pending_acceptance", "blockers"], "properties": #{"group": #{"type": "string", "minLength": 1}, "implementation_ids": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "ready_ids": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "blocked_ids": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "dependencies_ready": #{"type": "boolean"}, "read_files": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "files_to_touch": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "shared_files": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "evidence": #{"type": "string", "minLength": 1}, "pending_acceptance": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "blockers": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}}, "additionalProperties": false};
let change_schema = #{"type": "object", "required": ["group", "files_changed", "needs_common", "checks_run", "blockers", "summary"], "properties": #{"group": #{"type": "string", "minLength": 1}, "files_changed": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "needs_common": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "checks_run": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "blockers": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "summary": #{"type": "string", "minLength": 1}}, "additionalProperties": false};
let verdict_schema = #{"type": "object", "required": ["group", "passed", "verified_ids", "held_ids", "evidence", "issues", "pending_acceptance"], "properties": #{"group": #{"type": "string", "minLength": 1}, "passed": #{"type": "boolean"}, "verified_ids": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "held_ids": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "evidence": #{"type": "string", "minLength": 1}, "issues": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "pending_acceptance": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}}, "additionalProperties": false};
let quality_schema = #{"type": "object", "required": ["passed", "unit_tests_passed", "commands", "remaining_issues", "summary"], "properties": #{"passed": #{"type": "boolean"}, "unit_tests_passed": #{"type": "boolean"}, "commands": #{"type": "array", "items": #{"type": "object", "required": ["id", "exit_code", "evidence"], "properties": #{"id": #{"type": "string", "minLength": 1}, "exit_code": #{"type": "integer"}, "evidence": #{"type": "string", "minLength": 1}}, "additionalProperties": false}}, "remaining_issues": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "summary": #{"type": "string", "minLength": 1}}, "additionalProperties": false};
let snapshot_schema = #{"type": "object", "required": ["head", "index", "files", "fingerprint"], "properties": #{"head": #{"type": "string", "minLength": 1}, "index": #{"type": "string", "minLength": 1}, "files": #{"type": "object", "additionalProperties": #{"type": "string", "minLength": 1}}, "fingerprint": #{"type": "string", "minLength": 1}}, "additionalProperties": false};
let docs_schema = #{"type": "object", "required": ["files_changed", "remaining", "summary"], "properties": #{"files_changed": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "remaining": #{"type": "array", "items": #{"type": "string", "minLength": 1}, "uniqueItems": true}, "summary": #{"type": "string", "minLength": 1}}, "additionalProperties": false};

let all_groups = [
    #{
        name: "scope-foundation",
        item_ids: ["ORG-BASE"],
        contract_refs: ["S2", "contract:4", "contract:5", "contract:9"],
        modules: [],
        acceptance_ids: ["A02", "A03", "A04", "A05", "A06", "A09", "A18", "A19", "A27", "A28", "A32"],
        dependencies: [],
        mode: "core",
        focus: "Sections 4 and 5 in full: org tree, membership validity [from,to) in UTC, single primary org, management assignment tied to the eligible role, descendants default false, impact authorization, optimistic locks, audit and versions; typed v2 resource/action registration and required dimensions; public resolver and ports; bounded expansion and cache expiry at known validity boundaries. Preserve customer assignment business-date semantics.",
    },
    #{
        name: "s2-customer-contract",
        item_ids: ["S2-01", "S2-02"],
        contract_refs: ["S2-01", "S2-02"],
        modules: ["M01", "M02"],
        acceptance_ids: ["A01", "A07", "A08", "A15", "A16", "A21", "A27", "A28"],
        dependencies: ["scope-foundation"],
        mode: "core",
        focus: "Current customer assignment primary/collaborator versus contract historical operator; customer handover lists open documents/tasks but does not silently hand over orders. End-to-end customer/contract v2 commands and reads, attachments, print, candidates and exports. Include customer_profile processes and ordinary web-api handlers/DTOs.",
    },
    #{
        name: "s2-sales-purchase",
        item_ids: ["S2-03", "S2-04", "ATTRIBUTION-CORRECTION"],
        contract_refs: ["S2-03", "S2-04", "contract:6.2.2"],
        modules: ["M04", "M05"],
        acceptance_ids: ["A01", "A07", "A09", "A10", "A12", "A14", "A15", "A16", "A17", "A21", "A24", "A27", "A28", "A30"],
        dependencies: ["scope-foundation", "s2-customer-contract"],
        mode: "core",
        focus: "Sales and purchase Service/DTO/entity/repository/index/handler/process/frontend paths; business organization, source customer/contract authorization, change orders, sales/purchase returns and associated refunds/reversals via reverse_flow and returns_center. First formalization atomically freezes owner/org/names/ancestor path/version; explicit versioned attribution correction with evidence, reason, before/after and impact. Do not enable sales-owner handover or change acceptance-task source in this batch.",
    },
    #{
        name: "org-pages",
        item_ids: ["S2-08"],
        contract_refs: ["S2-08"],
        modules: [],
        acceptance_ids: ["A02", "A03", "A06", "A09", "A15", "A16", "A19", "A32"],
        dependencies: ["scope-foundation"],
        mode: "core",
        focus: "Organization/member/management/scope configuration pages, route pages in erp-client/app, navigation/registry and generated permission consumers; before/after impact preview, concurrency/error handling, URL and Query synchronization, applied filters, narrow-screen and keyboard/dirty/error recovery. Follow erp-client/AGENTS.md and existing list-workspace components.",
    },
    #{
        name: "entry-mapping",
        item_ids: ["S2-09"],
        contract_refs: ["S2-09"],
        modules: [],
        acceptance_ids: ["A17", "A30", "A31"],
        dependencies: ["scope-foundation", "s2-sales-purchase"],
        mode: "core",
        focus: "Enumerate every ERP/import/mall/external creation and formalization entry actually present. Explicit responsible user/org mapping and provable first effective source; missing mapping enters existing exception processing, never system-account fallback. Never rebuild removed mall/phase-two business. Absent or deliberately closed entries stay explicitly pending/not launched with evidence; do not invent mapping policy.",
    },
    #{
        name: "s3-selection",
        item_ids: ["S3-02"],
        contract_refs: ["S3-02"],
        modules: ["M03"],
        acceptance_ids: ["A01", "A07", "A15", "A16", "A17", "A28"],
        dependencies: ["scope-foundation", "s2-customer-contract"],
        mode: "core",
        focus: "Explicit selection-book sales owner and business org, creation/validation/handover/audit where applicable, proposal inheritance of book responsibility, list/summary/detail/candidates/export/commands and frontend. Customer submitter is not internal sales owner.",
    },
    #{
        name: "s3-funds",
        item_ids: ["S3-03"],
        contract_refs: ["S3-03"],
        modules: ["M07", "M08", "M09"],
        acceptance_ids: ["A11", "A12", "A13", "A15", "A16", "A29"],
        dependencies: ["scope-foundation", "s2-sales-purchase"],
        mode: "core",
        focus: "All receivable/payable/invoice entities, DTOs, owned repositories, services, processes, handlers and frontend; forward/reverse allocation ledgers, refunds and reversal paths. Register whole-document and unallocated-balance authorization basis per resource. Partial reads return null restricted fields, deny incompatible print/attachment/download, no subtraction leakage. Distinguish authorized share from optional user filters; no doubled header amounts.",
    },
    #{
        name: "s3-fulfill-workbench",
        item_ids: ["S2-10", "S3-04"],
        contract_refs: ["S2-10", "S3-04"],
        modules: ["M06", "M12"],
        acceptance_ids: ["A13", "A14", "A21", "A22", "A23", "A24", "A25"],
        dependencies: ["scope-foundation", "s2-sales-purchase", "s3-funds"],
        mode: "core",
        focus: "Fulfillment/customer acceptance and task/workbench read models plus fulfillment_execution and workflow-fact adapters. Source sales/purchase dimensions, current handler and historical operator, management versus execution, blocked tasks and recovery, task-specific qualified candidates, atomic purchase-owner/all-open-task cascade with warehouse responsibility keys excluded. Approval tasks cannot be reassigned; no team pool, claim or act-on-behalf flow. Keep sales handover disabled.",
    },
    #{
        name: "s3-quality-profit",
        item_ids: ["S3-05", "S3-06"],
        contract_refs: ["S3-05", "S3-06"],
        modules: ["M10", "M11"],
        acceptance_ids: ["A09", "A11", "A12", "A15", "A16", "A20", "A29", "A30"],
        dependencies: ["scope-foundation", "s2-sales-purchase", "s3-funds"],
        mode: "core",
        focus: "M10 real backend query/service/handler/export implementation as well as client; current-customer business versus historical-order contribution, drilldown, trends and unknown partition. M11 independent sales/cost read authorization, cache invalidation across all consumers, original non-card/ex-tax/cost coverage/period formulas unchanged. Representative index evaluation is pending external acceptance when prohibited; no zero-filled gaps or fabricated procurement revenue allocation.",
    },
    #{
        name: "initialization",
        item_ids: ["S2-11"],
        contract_refs: ["S2-11"],
        modules: [],
        acceptance_ids: ["A18", "A19", "A30", "A31", "A32"],
        dependencies: ["scope-foundation"],
        mode: "core",
        focus: "Initialization code, explicit per-role/resource/action scope seeds, revocation/disable/soft-delete preservation and idempotence; current-model indexes and composition-root registration. Unit evidence must be separate from prohibited real database rerun, concurrency and query-plan acceptance. Inspect actual reads and resolver, not merely seed exit status.",
    },
    #{
        name: "cross-resource-audit",
        item_ids: ["S2-05", "S2-06", "S2-07", "S3-01"],
        contract_refs: ["S2-05", "S2-06", "S2-07", "S3-01"],
        modules: [],
        acceptance_ids: ["A01", "A04", "A05", "A06", "A07", "A15", "A16", "A21", "A27", "A28", "A32"],
        dependencies: ["s2-customer-contract", "s2-sales-purchase", "org-pages", "entry-mapping", "initialization", "s3-selection", "s3-funds", "s3-fulfill-workbench", "s3-quality-profit"],
        mode: "core",
        focus: "Exhaustive caller/route registry audit of ALL old DataScope readers, workflow-fact adapters and registered resources, including uncovered consumers. Register unsupported resources as closed to v2 rather than grant fallback. scope_summary/ownership_basis/as_of/all versions, same snapshot, cache linkage, cross-page conflict and export generation/download final recheck for every applicable core resource. Cross-check S3-01 dependency ledger against original S2-01 through S2-11; no leftover aliases or omitted items.",
    },
    #{
        name: "s3-handover",
        item_ids: ["S3-07"],
        contract_refs: ["S3-07"],
        modules: [],
        acceptance_ids: ["A14", "A19", "A24", "A25", "A26", "A30"],
        dependencies: ["cross-resource-audit", "s3-fulfill-workbench"],
        mode: "handover",
        focus: "DEDICATED batch: explicit sales owner/business-org handover, target qualification, reason/idempotency/version/audit, atomic open-acceptance set and future-task responsibility source. Preserve approvals, completed acceptance and frozen contribution. Update fulfillment responsibility contract BEFORE review. Keep route/feature activation disabled until independent business acceptance; local tests never enable production handover.",
    },
    #{
        name: "s4-m13",
        item_ids: ["S4-M13"],
        contract_refs: ["S4", "contract:3"],
        modules: ["M13"],
        acceptance_ids: ["A01", "A07", "A13", "A15", "A16", "A17", "A19", "A31", "A32"],
        dependencies: ["cross-resource-audit"],
        mode: "s4",
        focus: "Supplier overall maintenance owner versus offering capability owner; owner creation/handover/audit. End-to-end applicable APIs, responsibility commands, UI, candidates, summaries, exports and audit. Public dictionaries do not require fabricated business owners.",
    },
    #{
        name: "s4-m14",
        item_ids: ["S4-M14"],
        contract_refs: ["S4", "contract:3"],
        modules: ["M14"],
        acceptance_ids: ["A01", "A07", "A13", "A15", "A16", "A17", "A19", "A31", "A32"],
        dependencies: ["cross-resource-audit"],
        mode: "s4",
        focus: "Product, catalog and supply/offerings maintenance owners versus procurement-rule-resolved procurement owner; include catalog and sourcing domains. End-to-end applicable APIs, responsibility commands, UI, candidates, summaries, exports and audit. Public dictionaries do not require fabricated business owners.",
    },
    #{
        name: "s4-m15",
        item_ids: ["S4-M15"],
        contract_refs: ["S4", "contract:3"],
        modules: ["M15"],
        acceptance_ids: ["A01", "A07", "A13", "A15", "A16", "A17", "A19", "A31", "A32"],
        dependencies: ["cross-resource-audit"],
        mode: "s4",
        focus: "API supplier orders internal follow-up owner and exception handler; external contact is not internal owner. End-to-end applicable APIs, responsibility commands, UI, candidates, summaries, exports and audit. Public dictionaries do not require fabricated business owners.",
    },
    #{
        name: "s4-m16",
        item_ids: ["S4-M16"],
        contract_refs: ["S4", "contract:3"],
        modules: ["M16"],
        acceptance_ids: ["A01", "A07", "A13", "A15", "A16", "A17", "A19", "A31", "A32"],
        dependencies: ["cross-resource-audit"],
        mode: "s4",
        focus: "Supplier settlement reconciliation owner, discrepancy handler and actual reviewer, including supply_settlement processes and finance facts. End-to-end applicable APIs, responsibility commands, UI, candidates, summaries, exports and audit. Public dictionaries do not require fabricated business owners.",
    },
    #{
        name: "s4-m17",
        item_ids: ["S4-M17"],
        contract_refs: ["S4", "contract:3"],
        modules: ["M17"],
        acceptance_ids: ["A01", "A07", "A13", "A15", "A16", "A17", "A19", "A31", "A32"],
        dependencies: ["cross-resource-audit"],
        mode: "s4",
        focus: "Integration exceptions and business differences current/historical handler and authorized operations. End-to-end applicable APIs, responsibility commands, UI, candidates, summaries, exports and audit. Public dictionaries do not require fabricated business owners.",
    },
    #{
        name: "s4-m18",
        item_ids: ["S4-M18"],
        contract_refs: ["S4", "contract:3"],
        modules: ["M18"],
        acceptance_ids: ["A01", "A07", "A13", "A15", "A16", "A17", "A19", "A31", "A32"],
        dependencies: ["cross-resource-audit"],
        mode: "s4",
        focus: "Inventory movement/adjustment actual operator, applicant and current approver; warehouse/org intersection; no invented inventory-balance owner. End-to-end applicable APIs, responsibility commands, UI, candidates, summaries, exports and audit. Public dictionaries do not require fabricated business owners.",
    },
    #{
        name: "release-readiness",
        item_ids: ["RELEASE-READINESS"],
        contract_refs: ["contract:10", "contract:12.1"],
        modules: [],
        acceptance_ids: ["A18", "A19", "A26", "A30", "A31"],
        dependencies: ["cross-resource-audit"],
        mode: "audit",
        focus: "READ-ONLY release checklist for contract section 10 and section 12.1: opened resource/API/task inventory, initialization/model/index versions, authoritative responsibility source registry, first effective atomic facts, representative accounts and all pending HTTP/browser/database/concurrency/index/business acceptance. Never deploy, initialize production or claim release approval. Record handover and S4 exclusions explicitly.",
    }
];

let contract_block = "Source of truth: docs/organization-data-scope-contract.md sections 5, 6, 8, 9, 10, 12 plus docs/organization-data-scope-s2.md section 4 and docs/organization-data-scope-s3.md sections 3 and 5. ";
contract_block += "Re-check code before trusting the static baseline table. DataScope v2 only: schema_version 2 required, explicit resource plus actions, typed target_dimension, dynamic own_org and managed_orgs for internal org only, no name-based identity, no target wildcard. ";
contract_block += "Computation: eligible roles with full action permission, per-role scope union, participant objects for read only with eligible roles, intersect user limit and mandatory business boundary and requested filters. ";
contract_block += "Hard bans: no parallel auth service, no created_by fallback for owner, no withdrawn name-param compat (reject with validation error), no silent drop of unknown params, no Company backfill for missing scope, no sales handover command until the dedicated handover batch passes, no approval-task auto reassignment, attribution snapshot is write-once with versioned correction command only. ";
contract_block += "Lists: same-query snapshot for list plus count plus grouping plus metrics, stable sort with id, scope_version pages 2 plus with DATA_SCOPE_CHANGED conflict, export re-verifies after last page and before download. ";
contract_block += "Amounts: authoritative allocation facts only, no client header recompute, no double count, unknown-attribution partition for reconcilable totals, partial reads return null restricted fields never zero, whole-document needs explicit basis. ";
contract_block += "Candidates come from the full visible scope, never the current page; resigned owners with visible data stay selectable; assignment pickers stay qualified-effective-only. ";

contract_block += "Read the WHOLE main contract, sections 3 through 12.1, S1/S2/S3 implementation contracts and both AGENTS.md files. Each group covers all applicable list/detail/summary/trend/drilldown/candidate/export/print/attachment/command/task paths. Unsupported operations require concrete not-applicable evidence, never an unexplained omission. ";
contract_block += "Query rules: same-field OR, different-field AND, deduplicate stable IDs, at most 100 submitted IDs per field, no truncation of server expansion. Do not return internal authorization proofs or invisible org/person sets. Distinguish historical selectors, current-owner selectors and qualified assignment selectors. ";
contract_block += "Preserve unrelated dirty work. Never stage, commit, reset, checkout, restore, clean or revert. Never deploy, change live production configuration, run production initialization, modify workflow controls, or rebuild removed phase-two business. Never add/modify/run integration tests, --test targets, --include-ignored, real Mongo/ERP_TEST_MONGO_URI, docker or dev-mongo. Use meaningful inline Rust unit tests and frontend unit tests. ";

fn is_text(value) { type_of(value) == "string" && value != "" }
fn is_true(value) { type_of(value) == "bool" && value }
fn texts(value) {
    if type_of(value) != "array" { return false; }
    for v in value { if !is_text(v) { return false; } }
    true
}
fn has(items, wanted) {
    if type_of(items) != "array" { return false; }
    for v in items { if v == wanted { return true; } }
    false
}
fn unique(items) {
    let out = [];
    for v in items { if !has(out, v) { out.push(v); } }
    out
}
fn subset(items, allowed) {
    if !texts(items) { return false; }
    for v in items { if !has(allowed, v) { return false; } }
    unique(items).len() == items.len()
}
fn partition(left, right, expected) {
    if !subset(left, expected) || !subset(right, expected) { return false; }
    for v in left { if has(right, v) { return false; } }
    left.len() + right.len() == expected.len()
}
fn run_agent(prompt, label, mode, schema) {
    try {
        if budget().remaining < 1 { return (); }
        let r = agent(prompt, #{label: label, capability_mode: mode, output_schema: schema});
        if type_of(r) == "map" && is_true(r.success) && type_of(r.output) == "map" { return r.output; }
    } catch (e) { log("Agent failed: " + label + ": " + e); }
    ()
}
fn safe_path(path) {
    if !is_text(path) || path.starts_with("/") || path.contains("\\") { return false; }
    for part in path.split("/") { if part == "" || part == "." || part == ".." { return false; } }
    for prefix in ["backend/crates/", "backend/apps/", "backend/scripts/", "erp-client/features/", "erp-client/app/", "erp-client/components/", "erp-client/lib/", "erp-client/tests/", "erp-client/scripts/", "docs/"] {
        if path.starts_with(prefix) { return true; }
    }
    has(["backend/Cargo.toml", "backend/Cargo.lock", "erp-client/package.json", "erp-client/package-lock.json"], path)
}
fn manifest_valid(plan) {
    if !texts(plan.files_to_touch) || !texts(plan.shared_files) || !texts(plan.read_files) { return false; }
    let combined = plan.files_to_touch + plan.shared_files;
    if combined.len() != unique(combined).len() { return false; }
    for path in unique(combined + plan.read_files) { if !safe_path(path) { return false; } }
    true
}
fn snapshot(schema, label) {
    let result = run_agent("Run only python3 .grok/scripts/org-data-scope-snapshot.py from repository root. Return its JSON verbatim. This hashes HEAD, index and actual tracked/untracked changes including pre-existing dirty files; do not modify anything or omit paths.", label, "read-only", schema);
    if result == () { return (); }
    if !is_text(result.head) || !is_text(result.index) || !is_text(result.fingerprint) || type_of(result.files) != "map" { return (); }
    for key in result.files.keys() { if !is_text(result.files[key]) { return (); } }
    result
}
fn delta_allowed(before, after, allowed) {
    if before == () || after == () { return false; }
    if before.head != after.head || before.index != after.index { return false; }
    for path in unique(before.files.keys() + after.files.keys()) {
        if before.files[path] != after.files[path] && !has(allowed, path) { return false; }
    }
    true
}
fn unchanged(before, after) {
    delta_allowed(before, after, []) && before.fingerprint == after.fingerprint
}
fn verdict_valid(v, g) {
    if v == () { return false; }
    if v.group != g.name || !is_true(v.passed) || !is_text(v.evidence) { return false; }
    if !texts(v.issues) || v.issues.len() != 0 || !texts(v.pending_acceptance) { return false; }
    partition(v.verified_ids, v.held_ids, g.item_ids) && v.held_ids.len() == 0
}
fn change_valid(v, group, allowed) {
    if v == () { return false; }
    v.group == group && subset(v.files_changed, allowed) && texts(v.blockers) && v.blockers.len() == 0 && texts(v.needs_common) && texts(v.checks_run)
}
fn review_prompt(g, context, contract) {
    "Independently review CURRENT code, staged/unstaged diff and newly created untracked files. Group and EXACT expected IDs: " + json_encode(g) + ". " + contract
    + " Read the whole entry-to-entity/repository/frontend dependency chain even outside the changed files. Refute missing callers, permission leaks, stale caches, incorrect historical attribution, amount leakage and task side effects. verified_ids/held_ids must exactly partition expected item_ids. passed=true only if all IDs have concrete code plus local test evidence, issues is empty and held_ids is empty. Do not execute builds/tests/codegen or formatters during a concurrent read-only review; inspect the completed check evidence. Local evidence is not business acceptance: list pending HTTP/browser/account/database/concurrency/index checks separately. No edits. Context: " + json_encode(context)
}
fn quality_valid(q, required) {
    if q == () { return false; }
    if !is_true(q.passed) || !is_true(q.unit_tests_passed) || !texts(q.remaining_issues) || q.remaining_issues.len() != 0 || type_of(q.commands) != "array" { return false; }
    let ids = [];
    for c in q.commands {
        if type_of(c) != "map" { return false; }
        if !is_text(c.id) || !is_text(c.evidence) || type_of(c.exit_code) != "i64" || c.exit_code != 0 { return false; }
        ids.push(c.id);
    }
    partition(ids, [], required)
}

// parallel() is a barrier. Preserve one output slot per input, including failures.
fn run_panel(jobs, limit) {
    let outputs = [];
    let offset = 0;
    while offset < jobs.len() {
        let chunk = [];
        while offset < jobs.len() && chunk.len() < limit {
            chunk.push(jobs[offset]);
            offset += 1;
        }
        let raw = [];
        if budget().remaining >= chunk.len() {
            try { raw = parallel(chunk); }
            catch (e) { log("Panel failed; do not replay possible writes: " + e); }
        } else { log("Insufficient agent budget; holding " + chunk.len().to_string() + " jobs"); }
        for index in 0..chunk.len() {
            let output = ();
            if type_of(raw) == "array" && index < raw.len() {
                let r = raw[index];
                if type_of(r) == "map" && is_true(r.success) && type_of(r.output) == "map" { output = r.output; }
            }
            outputs.push(output);
        }
    }
    outputs
}
fn intersects(left, right) {
    for value in left { if has(right, value) { return true; } }
    false
}
fn writes(plan) { plan.files_to_touch + plan.shared_files }
fn stable_reads(plan) {
    let out = [];
    for path in plan.read_files { if !has(writes(plan), path) { out.push(path); } }
    out
}
fn can_share_wave(left, right) {
    // Common/common writes are deferred to ONE merger after both writers finish.
    // Domain/domain, domain/common and either writer/stable-reader conflict.
    !intersects(left.files_to_touch, writes(right))
        && !intersects(right.files_to_touch, writes(left))
        && !intersects(writes(left), stable_reads(right))
        && !intersects(writes(right), stable_reads(left))
}
fn names(groups) {
    let out = [];
    for g in groups { out.push(g.name); }
    out
}
fn plan_valid(plan, g) {
    if plan == () { return false; }
    plan.group == g.name && texts(plan.implementation_ids) && texts(plan.ready_ids) && texts(plan.blocked_ids)
        && partition(plan.implementation_ids + plan.ready_ids, plan.blocked_ids, g.item_ids)
        && manifest_valid(plan) && texts(plan.blockers) && texts(plan.pending_acceptance) && is_text(plan.evidence)
}
fn plan_jobs_for(groups, inventory, reviews, contract, schema) {
    let jobs = [];
    for g in groups {
        let p = "Use read_file/grep on CURRENT code and dependencies; plan this group: " + json_encode(g) + ". " + contract;
        p += "Inventory: " + json_encode(inventory) + ". Earlier batch reviews: " + json_encode(reviews) + ". ";
        p += "implementation_ids, ready_ids and blocked_ids must exactly partition item_ids. Pending external acceptance alone is ready_ids plus pending_acceptance, never a code gap. Independently verify unselected dependencies; do not implement them. ";
        p += "List exact repo-relative files_to_touch for domain writes and shared_files for deferred module/route registration, API glue, shared cache, generated permissions, OpenAPI and related contracts. Include anticipated Cargo.lock and permissions.generated.ts changes in shared_files, not a parallel writer manifest. read_files lists implementation-critical interfaces/facts outside your own write set that must stay stable while you implement. Include transitive changed interfaces and any discovered dependency on another group; if it cannot be expressed with known stable paths, report blockers. Do not list every exploratory file as a dependency. ";
        p += "Only filenames, no directories/globs/traversal/symlink escapes. Include all needed Handler/Service/DTO/entity/repository/process/frontend paths. No .grok or AGENTS.md edits. A ready-only or release-readiness group must return empty write lists. New files or missing business policies require blockers/replanning. Shared-file patches will be merged by ONE agent after the concurrent domain writers finish. No build/test/format/code-generation commands in Plan; read-only.";
        jobs.push(#{prompt: p, label: "plan:" + g.name, capability_mode: "read-only", output_schema: schema});
    }
    jobs
}
fn review_panel(groups, context, contract, schema, prefix, limit) {
    let jobs = [];
    for g in groups {
        jobs.push(#{prompt: review_prompt(g, context, contract), label: prefix + g.name,
            capability_mode: "read-only", output_schema: schema});
    }
    run_panel(jobs, limit)
}

// One catalog drives inventory, selection, dependency order, verification and reports.
let all_ids = [];
for g in all_groups { all_ids += g.item_ids; }
let apply_changes = true;
let include_s4 = false;
let only_group = "";
let max_repairs = 1;
let max_parallel = 3;
let arg_errors = [];
if args != () && type_of(args) != "map" { arg_errors.push("args must be an object"); }
if type_of(args) == "map" {
    for key in args.keys() {
        if !has(["apply", "include_s4", "only", "max_repairs", "max_parallel", "agent_budget", "effort"], key) { arg_errors.push("Unknown arg: " + key); }
    }
    if args.apply != () {
        if type_of(args.apply) != "bool" { arg_errors.push("apply must be boolean"); } else { apply_changes = args.apply; }
    }
    if args.include_s4 != () {
        if type_of(args.include_s4) != "bool" { arg_errors.push("include_s4 must be boolean"); } else { include_s4 = args.include_s4; }
    }
    if args.only != () {
        if !is_text(args.only) { arg_errors.push("only must be a non-empty group name"); } else { only_group = args.only; }
    }
    if args.max_parallel != () {
        if type_of(args.max_parallel) != "i64" { arg_errors.push("max_parallel must be an integer"); }
        else if args.max_parallel < 1 || args.max_parallel > 8 { arg_errors.push("max_parallel must be 1..8"); }
        else { max_parallel = args.max_parallel; }
    }
    if args.max_repairs != () {
        if type_of(args.max_repairs) != "i64" { arg_errors.push("max_repairs must be an integer"); }
        else if args.max_repairs < 0 || args.max_repairs > 2 { arg_errors.push("max_repairs must be 0..2"); }
        else { max_repairs = args.max_repairs; }
    }
}
let groups = [];
let excluded_ids = [];
for g in all_groups {
    let selected = g.mode != "handover" && (g.mode != "s4" || include_s4);
    if only_group != "" { selected = g.name == only_group && (g.mode != "s4" || include_s4); }
    if selected { groups.push(g); } else { excluded_ids += g.item_ids; }
}
if groups.len() == 0 { arg_errors.push("No group selected; s4 groups also require include_s4=true. See complete-org-data-scope.md for exact names. infra-handover was split; select scope-foundation or s3-handover explicitly."); }
if arg_errors.len() > 0 {
    complete(#{summary: "Invalid arguments; no agents launched", errors: arg_errors, local_scope_closed: false, contract_closed: false});
    return;
}
let selected_ids = [];
let selected_names = names(groups);
for index in 0..groups.len() {
    if groups[index].name == "release-readiness" {
        let deps = groups[index].dependencies;
        for name in selected_names { if name != "release-readiness" && !has(deps, name) { deps.push(name); } }
        groups[index].dependencies = deps;
    }
    selected_ids += groups[index].item_ids;
}

phase("Inventory");
let baseline = snapshot(snapshot_schema, "snapshot:baseline");
let inventory = run_agent("Read actual repository code and ALL current contracts. " + contract_block
    + "Return exactly one item row for EVERY ID in this catalog: " + json_encode(all_groups)
    + ". code_gap means concrete implementation or local-test gaps; acceptance_pending means code/local evidence exists but real acceptance remains; local_ready means local code/tests have evidence; blocked means a missing business decision or unavailable mandatory dependency. Do not turn absent real acceptance into a code gap. For every clause in sections 3 through 12.1 and A01-A32 identify its catalog owner; report unmapped_requirements if anything lacks an owner. For removed mall entries record absence plus launch exclusion; never silently close S2-09. Read-only.",
    "inventory:requirements", "read-only", inventory_schema);
let ledger = #{};
let inventory_ok = baseline != () && inventory != ();
if inventory_ok {
    inventory_ok = type_of(inventory.items) == "array" && texts(inventory.unmapped_requirements) && inventory.unmapped_requirements.len() == 0;
    if inventory_ok {
        let ids = [];
        for row in inventory.items {
            if type_of(row) != "map" { inventory_ok = false; }
            else if !is_text(row.id) || !is_text(row.evidence) || !has(["code_gap", "local_ready", "acceptance_pending", "blocked"], row.state) || !texts(row.pending_acceptance) { inventory_ok = false; }
            else { ids.push(row.id); ledger[row.id] = row; }
        }
        if !partition(ids, [], all_ids) { inventory_ok = false; }
    }
}
if !inventory_ok {
    complete(#{summary: "Inventory failed or requirements are unmapped; no implementation", inventory: inventory, held_ids: selected_ids, excluded_ids: excluded_ids, local_scope_closed: false, contract_closed: false});
    return;
}

let plans = #{};
let reviews = #{};
let changes = [];
let ready_groups = [];
let failed_groups = [];
let run_errors = [];
let allowed_files = [];
let last_snapshot = baseline;
let stop_writes = false;
let pending_acceptance = ["本 workflow 不执行真实账号 HTTP/浏览器业务验收、真实数据库/并发/初始化重跑/索引执行计划及正式上线验收；本地通过不得核销这些条件。"];
for row in inventory.items { pending_acceptance += row.pending_acceptance; }

let wave_schedule = [];

// Dry runs plan all selected groups concurrently, then return before every writer.
if !apply_changes {
    phase("Plan");
    let results = run_panel(plan_jobs_for(groups, inventory, reviews, contract_block, plan_schema), max_parallel);
    for index in 0..groups.len() {
        let g = groups[index];
        let plan = results[index];
        plans[g.name] = plan;
        if !plan_valid(plan, g) { run_errors.push("Invalid plan: " + g.name); }
    }
    let after = snapshot(snapshot_schema, "snapshot:dry-run");
    if !unchanged(baseline, after) { run_errors.push("Read-only inventory/plan changed the worktree or snapshot failed"); }
    let report = "# 组织与数据范围执行计划\n\n执行模式：只读并发计划。不得登记实施完成或业务验收通过。\n\n";
    report += "最大并发：" + max_parallel.to_string() + "\n\n任务清单：\n```json\n" + json_encode(groups) + "\n```\n";
    report += "逐组计划：\n```json\n" + json_encode(plans) + "\n```\n阻塞：\n```json\n" + json_encode(run_errors) + "\n```\n";
    let report_path = write_scratch_file("org-data-scope-plan.md", report);
    complete(#{summary: "Read-only parallel plans; no implementation", report_path: report_path, plans: plans, errors: run_errors,
        max_parallel: max_parallel, excluded_ids: excluded_ids, local_scope_closed: false, contract_closed: false});
    return;
}

let remaining_groups = groups;
while remaining_groups.len() > 0 && !stop_writes {
    // Only release a dependent group after all selected predecessors pass batch review.
    let frontier = [];
    for g in remaining_groups {
        let blocked = false;
        let waiting = false;
        for dep in g.dependencies {
            if has(failed_groups, dep) { blocked = true; }
            if has(selected_names, dep) && !has(ready_groups, dep) { waiting = true; }
        }
        if blocked {
            failed_groups.push(g.name);
            run_errors.push("Failed dependency: " + g.name);
        } else if !waiting { frontier.push(g); }
    }
    if frontier.len() == 0 {
        for g in remaining_groups {
            if !has(failed_groups, g.name) { failed_groups.push(g.name); run_errors.push("Unresolved dependency: " + g.name); }
        }
        break;
    }

    phase("Plan");
    let plan_results = run_panel(plan_jobs_for(frontier, inventory, reviews, contract_block, plan_schema), max_parallel);
    let wave = [];
    for index in 0..frontier.len() {
        let g = frontier[index];
        let plan = plan_results[index];
        plans[g.name] = plan;
        let plan_ok = plan_valid(plan, g);
        if plan_ok {
            pending_acceptance += plan.pending_acceptance;
            plan_ok = is_true(plan.dependencies_ready) && plan.blocked_ids.len() == 0 && plan.blockers.len() == 0;
            for id in g.item_ids { if ledger[id].state == "blocked" { plan_ok = false; } }
            if plan.implementation_ids.len() == 0 && writes(plan).len() != 0 { plan_ok = false; }
            if plan.implementation_ids.len() > 0 && writes(plan).len() == 0 { plan_ok = false; }
            if g.mode == "audit" && (writes(plan).len() > 0 || plan.implementation_ids.len() > 0) { plan_ok = false; }
        }
        if !plan_ok {
            failed_groups.push(g.name);
            run_errors.push("Plan/dependency blocked: " + g.name);
        } else {
            let compatible = wave.len() < max_parallel;
            for peer in wave { if !can_share_wave(plan, plans[peer.name]) { compatible = false; } }
            if compatible { wave.push(g); }
            else { log("Defer and replan after this wave (limit or read/write conflict): " + g.name); }
        }
    }
    if wave.len() > 0 {
        let wave_names = names(wave);
        let wave_plans = #{};
        let wave_files = [];
        let domain_files = [];
        let common_files = [];
        for g in wave {
            let plan = plans[g.name];
            wave_plans[g.name] = plan;
            domain_files += plan.files_to_touch;
            common_files += plan.shared_files;
            wave_files += writes(plan);
            for id in plan.implementation_ids { ledger[id].state = "code_gap"; ledger[id].evidence = plan.evidence; }
        }
        domain_files = unique(domain_files);
        common_files = unique(common_files);
        wave_files = unique(wave_files);
        allowed_files = unique(allowed_files + wave_files);
        wave_schedule.push(#{groups: wave_names, domain_files: domain_files, shared_files: common_files});
        log("Concurrent implementation wave: " + json_encode(wave_names));
        let before = snapshot(snapshot_schema, "snapshot:wave-before");
        if !unchanged(last_snapshot, before) {
            stop_writes = true;
            run_errors.push("Unexpected changes during planning or missing wave snapshot");
        }
        last_snapshot = before;
        let wave_ok = false;
        let wave_checks = ();
        let wave_changes = [];
        let can_repair = true;
        for attempt in 0..(max_repairs + 1) {
            if !wave_ok && !stop_writes && can_repair {
                if attempt == 0 { phase("Implement"); } else { phase("Repair"); }
                let writer_jobs = [];
                let writer_groups = [];
                for g in wave {
                    let plan = plans[g.name];
                    if plan.implementation_ids.len() > 0 && plan.files_to_touch.len() > 0 {
                        let prompt = "Implement this dependency-ready group: " + json_encode(g) + ". " + contract_block;
                        if attempt > 0 { prompt = "Repair only concrete findings in this group: " + json_encode(g) + ". " + contract_block; }
                        prompt += "Exact domain files you own: " + json_encode(plan.files_to_touch) + ". Stable inputs: " + json_encode(stable_reads(plan)) + ". ";
                        prompt += "Other agents concurrently own: " + json_encode(wave_names) + ". You are not alone; preserve their and pre-existing edits. Do NOT write shared_files: " + json_encode(plan.shared_files) + ". Return needed shared changes in needs_common. ";
                        prompt += "Do not run ANY Cargo/npm/build/test/formatter/codegen commands while peer writers are active; write meaningful unit tests, then checks run centrally AFTER merge. checks_run must list only checks actually run, so [] is valid during implementation. New paths or business decisions are blockers; never widen this manifest. ";
                        prompt += "Plan: " + json_encode(plan) + ". Previous review: " + json_encode(reviews[g.name]) + ". Focused-check findings: " + json_encode(wave_checks) + ".";
                        let label = "implement:" + g.name;
                        if attempt > 0 { label = "repair:batch:" + g.name; }
                        writer_jobs.push(#{prompt: prompt, label: label, capability_mode: "read-write", output_schema: change_schema});
                        writer_groups.push(g);
                    }
                }
                let implemented = run_panel(writer_jobs, max_parallel);
                let implementation_ok = true;
                for index in 0..writer_groups.len() {
                    let g = writer_groups[index];
                    let result = implemented[index];
                    wave_changes.push(result);
                    changes.push(result);
                    if !change_valid(result, g.name, plans[g.name].files_to_touch) { implementation_ok = false; }
                    else if result.needs_common.len() > 0 && common_files.len() == 0 { implementation_ok = false; }
                }
                let after = snapshot(snapshot_schema, "snapshot:wave-implemented");
                // Audit the whole concurrent panel once, never compare one child against peer changes.
                if !delta_allowed(last_snapshot, after, domain_files) {
                    stop_writes = true;
                    run_errors.push("Parallel writers exceeded wave manifest or changed HEAD/index");
                }
                last_snapshot = after;
                if !implementation_ok {
                    can_repair = false;
                    run_errors.push("Failed/incomplete implementation output: " + json_encode(wave_names));
                }
                if implementation_ok && !stop_writes {
                    let merge_ok = true;
                    if common_files.len() > 0 {
                        phase("Merge");
                        let merged = run_agent("You are the SINGLE shared-file merger for this completed writer panel. " + contract_block
                            + "Group in output must be wave-merge. Exact shared files: " + json_encode(common_files)
                            + ". Merge all compatible domain requests and planned shared-only changes; preserve every group's additions to the same registration/cache/OpenAPI file. Source plans: " + json_encode(wave_plans)
                            + ". Participating groups: " + json_encode(wave_names) + ". Implementer requests: " + json_encode(wave_changes)
                            + ". Prior focused checks: " + json_encode(wave_checks) + ". Prior reviews: " + json_encode(reviews)
                            + ". No other writer is active now. Do not touch domain files or widen scope. No builds/tests yet; generate/build only in the following LocalChecks stage. Return needs_common=[] only when ALL requests resolve; conflicts or missing manifests remain blockers.",
                            "merge:wave", "read-write", change_schema);
                        changes.push(merged);
                        merge_ok = change_valid(merged, "wave-merge", common_files);
                        if merge_ok { merge_ok = merged.needs_common.len() == 0; }
                        after = snapshot(snapshot_schema, "snapshot:wave-merged");
                        if !delta_allowed(last_snapshot, after, common_files) {
                            stop_writes = true;
                            run_errors.push("Shared merger exceeded manifest");
                        }
                        last_snapshot = after;
                    }
                    if !merge_ok { can_repair = false; run_errors.push("Shared merge blocked: " + json_encode(wave_names)); }
                    if merge_ok && !stop_writes {
                        let focused_ok = true;
                        if wave_files.len() > 0 {
                            phase("LocalChecks");
                            wave_checks = run_agent("Run focused local checks only AFTER all panel writers and shared merge have finished. " + contract_block
                                + "Groups: " + json_encode(wave_names) + ". Planned changed files: " + json_encode(wave_files)
                                + ". Source edits/format fixes are forbidden here. Run touched-crate env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib and Clippy/fmt checks, then affected Vitest AND Node suites and TypeScript as applicable. Commands within this agent are sequential; do not overlap backend permission generation with frontend reads. Missing applicable checks cannot pass. Return commands with exactly one id=focused-checks, exit_code=0 only when ALL applicable commands passed, and evidence containing the exact commands/outcomes; unit_tests_passed requires actual applicable unit tests. Build/codegen outputs in the repository are allowed ONLY in the listed shared/generated files: " + json_encode(common_files) + ". No staging or committing. No edits to the snapshot/workflow controls.",
                                "checks:wave", "execute", quality_schema);
                            focused_ok = quality_valid(wave_checks, ["focused-checks"]);
                            after = snapshot(snapshot_schema, "snapshot:wave-checks");
                            if !delta_allowed(last_snapshot, after, common_files) {
                                stop_writes = true;
                                run_errors.push("Focused checks changed unapproved source or index/HEAD");
                            }
                            last_snapshot = after;
                        }
                        if !stop_writes {
                            phase("Review");
                            let verdicts = review_panel(wave, #{plans: wave_plans, checks: wave_checks, changes: wave_changes}, contract_block,
                                verdict_schema, "review:wave:", max_parallel);
                            let review_ok = true;
                            for index in 0..wave.len() {
                                let g = wave[index];
                                reviews[g.name] = verdicts[index];
                                if !verdict_valid(verdicts[index], g) { review_ok = false; }
                            }
                            after = snapshot(snapshot_schema, "snapshot:wave-reviewed");
                            if !unchanged(last_snapshot, after) {
                                stop_writes = true;
                                run_errors.push("Read-only parallel review changed the worktree or snapshot failed");
                            }
                            last_snapshot = after;
                            wave_ok = focused_ok && review_ok && !stop_writes;
                        }
                    }
                }
            }
        }
        for g in wave {
            if wave_ok { ready_groups.push(g.name); pending_acceptance += reviews[g.name].pending_acceptance; }
            else { failed_groups.push(g.name); run_errors.push("Wave held: " + g.name); }
        }
    }
    // Deferred candidates are freshly planned against the next immutable worktree state.
    let next_groups = [];
    for g in remaining_groups {
        if !has(ready_groups, g.name) && !has(failed_groups, g.name) { next_groups.push(g); }
    }
    remaining_groups = next_groups;
}

let required_checks = ["rust-fmt", "rust-check", "rust-clippy", "rust-unit", "bpm-boundaries", "domain-boundaries", "permissions-drift", "typescript", "client-lint", "vitest", "node-unit"];
let quality_prompt = "Run ALL required local gates without editing source, tests, docs, HEAD or index. " + contract_block
    + "Each commands entry requires its exact id, numeric exit_code and real command/output evidence; failed/unavailable commands cannot pass. From backend: rust-fmt=cargo fmt --all --check; rust-check=cargo check --workspace; rust-clippy=cargo clippy --workspace --all-targets --all-features -- -D warnings; rust-unit=env -u ERP_TEST_MONGO_URI cargo test --workspace --lib; bpm-boundaries=./scripts/check-bpm-boundaries.sh; domain-boundaries=./scripts/check-domain-boundaries.sh; permissions-drift=./scripts/check-permissions-drift.sh. From erp-client: typescript=npx tsc --noEmit; client-lint=npm run lint; vitest=npm run test:vitest; node-unit=npm run test:node. All 11 mandatory; no slow-suite omission. Permission drift may fail on legitimate uncommitted generated changes: record failure, NEVER stage/commit or weaken the script to make it pass. Build-generated changes invalidate this check snapshot and require a fresh review/check iteration within the already approved manifest.";
let quality = ();
let checks_passed = false;
let review_all_ok = false;
let repair_rounds = 0;
// Repair only a fully implemented set; blocked dependency batches need a fresh plan/run.
let eligible_for_gates = !stop_writes && ready_groups.len() == groups.len();
if eligible_for_gates {
    for attempt in 0..(max_repairs + 1) {
        if !checks_passed && !stop_writes {
            if attempt > 0 {
                phase("Repair");
                let repaired = run_agent("Fix only the concrete final gate/review failures from this run. " + contract_block
                    + "Exact approved files: " + json_encode(allowed_files) + ". No new file scope, no stage/commit, no weakening checks. Group in output must be quality-repair. Quality: " + json_encode(quality) + ". Reviews: " + json_encode(reviews) + ". All selected groups will be independently re-reviewed and ALL gates rerun.",
                    "repair:quality", "all", change_schema);
                changes.push(repaired);
                repair_rounds += 1;
                let after = snapshot(snapshot_schema, "snapshot:repair");
                if !change_valid(repaired, "quality-repair", allowed_files) || !delta_allowed(last_snapshot, after, allowed_files) {
                    stop_writes = true;
                    run_errors.push("Repair failed or exceeded manifest");
                }
                last_snapshot = after;
            }
            if !stop_writes {
                // A later batch may edit a shared file used by an earlier batch.
                phase("Review");
                review_all_ok = true;
                let final_reviews = review_panel(groups, changes, contract_block, verdict_schema, "review:final:", max_parallel);
                for index in 0..groups.len() {
                    let g = groups[index];
                    let r = final_reviews[index];
                    reviews[g.name] = r;
                    if !verdict_valid(r, g) { review_all_ok = false; }
                }
                phase("Quality");
                quality = run_agent(quality_prompt, "quality:gates", "execute", quality_schema);
                let after = snapshot(snapshot_schema, "snapshot:quality");
                if !delta_allowed(last_snapshot, after, allowed_files) {
                    stop_writes = true;
                    run_errors.push("Checks/review changed unapproved files, index or HEAD");
                }
                checks_passed = review_all_ok && quality_valid(quality, required_checks) && unchanged(last_snapshot, after);
                last_snapshot = after;
            }
        }
    }
}

phase("Verify");
let verified_ids = [];
let held_ids = [];
let verifies = #{};
if checks_passed && !stop_writes {
    let verdicts = review_panel(groups, #{quality: quality, snapshot: last_snapshot, reviews: reviews},
        contract_block + " FINAL verification: all writers, merges and checks have finished. Verify each exact ID and all changed shared dependencies against this immutable snapshot. No edits or build commands.",
        verdict_schema, "verify:", max_parallel);
    for index in 0..groups.len() {
        let g = groups[index];
        let v = verdicts[index];
        verifies[g.name] = v;
        if verdict_valid(v, g) {
            verified_ids += g.item_ids;
            pending_acceptance += v.pending_acceptance;
            for id in g.item_ids { ledger[id].state = "acceptance_pending"; ledger[id].evidence = v.evidence; }
        } else { held_ids += g.item_ids; }
    }
} else { held_ids = selected_ids; }
let after_verify = snapshot(snapshot_schema, "snapshot:verified");
if !unchanged(last_snapshot, after_verify) {
    verified_ids = [];
    held_ids = selected_ids;
    stop_writes = true;
    run_errors.push("Final verification snapshot changed or unavailable");
}
last_snapshot = after_verify;

// Docs changes are restricted to delivery status, then independently audited.
phase("Docs");
let docs_files = ["docs/organization-data-scope-contract.md", "docs/organization-data-scope-s1.md", "docs/organization-data-scope-s2.md", "docs/organization-data-scope-s3.md"];
let docs = ();
let docs_ok = false;
if !stop_writes {
    docs = run_agent("Update delivery status only in these exact Markdown files: " + json_encode(docs_files) + ". " + contract_block
        + "Write contract-style execution instructions, never a discussion/review diary. Do not alter target requirements, exit conditions, A01-A32 or evidence previously recorded for unrelated batches. Selected IDs: " + json_encode(selected_ids) + "; locally verified IDs: " + json_encode(verified_ids) + "; held IDs: " + json_encode(held_ids) + "; excluded IDs: " + json_encode(excluded_ids)
        + ". Checks: " + json_encode(quality) + "; verifies: " + json_encode(verifies) + "; pending acceptance: " + json_encode(unique(pending_acceptance))
        + ". Use explicit per-resource operation coverage and source evidence. State code/local-check/held/not-executed acceptance separately. S2/S3 cannot become accepted; handover stays disabled. S4 remains not selected unless explicitly in selected IDs. Do not modify production code, OpenAPI or registry now. Return changed files and all remaining work.",
        "docs:status", "all", docs_schema);
    let after_docs = snapshot(snapshot_schema, "snapshot:docs");
    docs_ok = docs != ();
    if docs_ok { docs_ok = subset(docs.files_changed, docs_files) && texts(docs.remaining); }
    docs_ok = docs_ok && delta_allowed(last_snapshot, after_docs, docs_files);
    if docs_ok {
        let audit_group = #{name: "docs-audit", item_ids: ["DOCS-STATUS"], focus: "Final documentation accuracy"};
        let audit = run_agent("Read final delivery Markdown changes. Ensure ONLY status/evidence changed, requirements and exit criteria unchanged, all held/excluded/unexecuted acceptance still explicit, no production acceptance or handover enablement claim. Compare with verified IDs " + json_encode(verified_ids)
            + ", held " + json_encode(held_ids) + ", excluded " + json_encode(excluded_ids) + ", actual checks " + json_encode(quality)
            + ". Output group=docs-audit; verified_ids/held_ids must partition [DOCS-STATUS]; no edits.",
            "verify:docs", "read-only", verdict_schema);
        docs_ok = verdict_valid(audit, audit_group);
    }
    let final_snapshot = snapshot(snapshot_schema, "snapshot:final");
    docs_ok = docs_ok && unchanged(after_docs, final_snapshot);
    if !docs_ok {
        run_errors.push("Documentation audit failed; no close-out claim");
        if !delta_allowed(last_snapshot, final_snapshot, docs_files) {
            verified_ids = [];
            held_ids = selected_ids;
        }
    }
    last_snapshot = final_snapshot;
}

phase("Report");
verified_ids = unique(verified_ids);
held_ids = unique(held_ids);
let local_scope_closed = checks_passed && docs_ok && held_ids.len() == 0 && partition(verified_ids, held_ids, selected_ids) && !stop_writes;
let report = "# 组织与数据范围交付记录\n\n"
    + "本次本地范围闭合：" + local_scope_closed.to_string() + "。合同业务验收：未完成。正式业务开放：未授权。\n\n"
    + "已选任务：" + json_encode(selected_ids) + "\n\n本地验证通过：" + json_encode(verified_ids)
    + "\n\n保留未完成：" + json_encode(held_ids) + "\n\n本次未选：" + json_encode(excluded_ids)
    + "\n\n质量门禁：" + checks_passed.to_string() + "；文档核验：" + docs_ok.to_string() + "。\n\n"
    + "未执行验收必须保留：\n```json\n" + json_encode(unique(pending_acceptance)) + "\n```\n\n"
    + "实际并发批次：\n```json\n" + json_encode(wave_schedule) + "\n```\n\n"
    + "需求与责任清单：\n```json\n" + json_encode(all_groups) + "\n```\n\n"
    + "盘点及计划证据：\n```json\n" + json_encode(#{inventory: inventory, ledger: ledger, plans: plans}) + "\n```\n\n"
    + "检查及最终验证证据：\n```json\n" + json_encode(#{quality: quality, verifies: verifies, docs: docs, snapshot: last_snapshot}) + "\n```\n\n"
    + "阻塞处理：修复下列问题后重新盘点；不得将预算耗尽、失败输出或缺失证据计为完成。\n```json\n" + json_encode(run_errors) + "\n```\n";
let report_path = write_scratch_file("org-data-scope-report.md", report);
complete(#{summary: verified_ids.len().to_string() + " locally verified; " + held_ids.len().to_string() + " held; local scope closed=" + local_scope_closed.to_string() + "; business acceptance pending",
    report_path: report_path, verified_ids: verified_ids, held_ids: held_ids, excluded_ids: excluded_ids,
    checks_passed: checks_passed, docs_passed: docs_ok, local_scope_closed: local_scope_closed, contract_closed: false,
    pending_acceptance: unique(pending_acceptance), errors: run_errors, repair_rounds: repair_rounds,
    max_parallel: max_parallel, waves: wave_schedule});
