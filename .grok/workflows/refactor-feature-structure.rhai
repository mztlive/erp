let meta = #{
    name: "refactor-feature-structure",
    description: "Reorganize one erp-client feature directory into hooks/components/lib/api with single-responsibility files, without changing behavior",
    when_to_use: "Use when a frontend feature mixes UI and logic at the root and needs a self-describing hooks/components/lib layout.",
    phases: [
        #{ title: "Inventory", detail: "map files, exports, and external importers" },
        #{ title: "Plan", detail: "design target tree, moves, and splits" },
        #{ title: "Relocate", detail: "move files into named directories and fix imports" },
        #{ title: "Split", detail: "extract mixed-concern files by responsibility" },
        #{ title: "Review", detail: "adversarial check of structure and behavior" },
        #{ title: "Verify", detail: "typecheck, lint, and scope gates" },
    ],
};

let inventory_schema = #{
    "type": "object",
    "required": ["files", "external_importers", "empty_dirs", "summary"],
    "properties": #{
        "summary": #{ "type": "string" },
        "empty_dirs": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "external_importers": #{
            "type": "array",
            "maxItems": 40,
            "items": #{
                "type": "object",
                "required": ["file", "imported"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "imported": #{ "type": "string" },
                },
            },
        },
        "files": #{
            "type": "array",
            "maxItems": 80,
            "items": #{
                "type": "object",
                "required": ["path", "lines", "kind"],
                "properties": #{
                    "path": #{ "type": "string" },
                    "lines": #{ "type": "integer" },
                    "kind": #{ "type": "string" },
                    "mixed": #{ "type": "boolean" },
                    "notes": #{ "type": "string" },
                },
            },
        },
    },
};

let plan_schema = #{
    "type": "object",
    "required": ["analysis_summary", "target_tree", "moves", "splits", "clusters", "invariants", "allowed_external_paths"],
    "properties": #{
        "analysis_summary": #{ "type": "string" },
        "target_tree": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "invariants": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "allowed_external_paths": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "moves": #{
            "type": "array",
            "maxItems": 80,
            "items": #{
                "type": "object",
                "required": ["from", "to", "reason"],
                "properties": #{
                    "from": #{ "type": "string" },
                    "to": #{ "type": "string" },
                    "reason": #{ "type": "string" },
                },
            },
        },
        "splits": #{
            "type": "array",
            "maxItems": 24,
            "items": #{
                "type": "object",
                "required": ["file", "extract_to", "responsibility"],
                "properties": #{
                    "file": #{ "type": "string" },
                    "extract_to": #{ "type": "string" },
                    "responsibility": #{ "type": "string" },
                    "notes": #{ "type": "string" },
                },
            },
        },
        "clusters": #{
            "type": "array",
            "maxItems": 5,
            "items": #{
                "type": "object",
                "required": ["id", "title", "objective", "files"],
                "properties": #{
                    "id": #{ "type": "string" },
                    "title": #{ "type": "string" },
                    "objective": #{ "type": "string" },
                    "files": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
                },
            },
        },
    },
};

let implementation_schema = #{
    "type": "object",
    "required": ["completed", "summary", "files_changed"],
    "properties": #{
        "completed": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
        "files_changed": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "leftover_issues": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
    },
};

let review_schema = #{
    "type": "object",
    "required": ["approved", "behavior_preserved", "structure_ok", "scope_clean", "evidence", "issues"],
    "properties": #{
        "approved": #{ "type": "boolean" },
        "behavior_preserved": #{ "type": "boolean" },
        "structure_ok": #{ "type": "boolean" },
        "scope_clean": #{ "type": "boolean" },
        "evidence": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "issues": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
    },
};

let verification_schema = #{
    "type": "object",
    "required": ["all_pass", "scope_clean", "behavior_shape_unchanged", "commands", "summary"],
    "properties": #{
        "all_pass": #{ "type": "boolean" },
        "scope_clean": #{ "type": "boolean" },
        "behavior_shape_unchanged": #{ "type": "boolean" },
        "summary": #{ "type": "string" },
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
    },
};

fn in_scope(path, root) {
    if type_of(path) != "string" { return false; }
    if path.contains("..") { return false; }
    if path == root { return true; }
    path.starts_with(root + "/")
}

let smoke = false;
let requested = ();
if args != () {
    if args.smoke == true { smoke = true; }
    requested = args.target;
}
if smoke == true {
    complete(#{ status: "validated", summary: "Smoke-check path compiled successfully." });
}

let target = "erp-client/features/master-data";
if requested != () && type_of(requested) == "string" {
    requested.trim();
    if requested != "" { target = requested; }
}
if !target.starts_with("erp-client/") || target.contains("..") || target.contains("node_modules") {
    pause("verification", "Pass args.target as a path under erp-client/, without .. or node_modules.");
}
log("Refactor target: " + target);

phase("Inventory");
let inventory_prompt = "Work as a file-search specialist. Inventory the frontend feature directory ";
inventory_prompt += target;
inventory_prompt += " in the current repository. ";
inventory_prompt += "First read erp-client/AGENTS.md. Then use run_terminal_command (find, wc, grep) plus read_file/grep. ";
inventory_prompt += "Do not edit any file. ";
inventory_prompt += "List every .ts/.tsx file under the target with line counts. Classify kind as one of: api, hook, component, lib, types, mixed. ";
inventory_prompt += "Set mixed=true when a file contains both React UI and non-trivial data/query/form/url logic. ";
inventory_prompt += "Find leftover empty directories. ";
inventory_prompt += "Find every importer OUTSIDE the target directory that references @/features/... paths from this feature, including app/ routes and other features. ";
inventory_prompt += "All reported paths must be repository-relative. Do not inspect backend/ except to confirm you did not need it.";
let inventory = agent(inventory_prompt, #{
    label: "inventory",
    agent_type: "lightweight",
    capability_mode: "read-only",
    output_schema: inventory_schema,
});
if inventory == () || !inventory.success || inventory.output == () {
    complete(#{ status: "blocked", summary: "Inventory failed before any edits.", target: target });
}

let files = [];
if inventory.output.files != () {
    for f in inventory.output.files {
        if f.path != () && in_scope(f.path, target) {
            files.push(f);
        } else if f.path != () {
            log("Dropped out-of-scope inventory file: " + f.path);
        }
    }
}
if files.len() == 0 {
    complete(#{ status: "no-op", summary: "No in-scope source files were found under " + target + ".", target: target });
}
log(files.len().to_string() + " in-scope files inventoried");

phase("Plan");
let plan_prompt = "Act as the senior frontend refactoring architect for a Next.js SPA feature. ";
plan_prompt += "Read erp-client/AGENTS.md first. Independently inspect the actual files with read_file/grep before planning. ";
plan_prompt += "Target directory: " + target + ". ";
plan_prompt += "Inventory JSON: " + json_encode(#{ files: files, external_importers: inventory.output.external_importers, empty_dirs: inventory.output.empty_dirs }) + ". ";
plan_prompt += "Design a self-describing layout so a reader knows a file's job from its path. Required skeleton: ";
plan_prompt += "api/ (HTTP adapters, DTO contracts, mappers; no React); ";
plan_prompt += "hooks/ (React hooks only: TanStack Query, URL/search-param state, column-definition hooks); ";
plan_prompt += "components/ (UI only, consume hooks; nest as pages/, list/, product/, supplier/, category/, shared/ when those domains exist); ";
plan_prompt += "lib/ (pure TS: copy, models, formatters, summaries, field builders, export helpers); ";
plan_prompt += "types.ts or types/ (types and constants only). ";
plan_prompt += "Keep a thin root api.ts barrel if other features already import it. ";
plan_prompt += "Do not leave mixed page/dialog/hook/model files at the feature root except api.ts and types.ts. ";
plan_prompt += "Delete leftover empty directories. Prefer git mv / exact file moves over rewrites. ";
plan_prompt += "Split only files that mix responsibilities. Do not invent new behavior, new UI, new query keys, or new URL params. ";
plan_prompt += "Functional shape must stay identical: request paths, payloads, query keys, invalidation, form defaults/validators/submit, visible copy, CSS/layout, permission checks, and routing. ";
plan_prompt += "allowed_external_paths must list every non-target file you will update (app routes and other features that import this module). ";
plan_prompt += "Group splits into at most 5 independent clusters. Skip a split if it cannot be done as a pure move/extract. Do not edit files.";
let plan = agent(plan_prompt, #{
    label: "plan-architecture",
    agent_type: "general-purpose",
    capability_mode: "read-only",
    output_schema: plan_schema,
});
if plan == () || !plan.success || plan.output == () {
    complete(#{ status: "blocked", summary: "Architectural plan failed before any edits.", target: target });
}

let moves = [];
if plan.output.moves != () {
    for m in plan.output.moves {
        if m.from != () && m.to != () && in_scope(m.from, target) && in_scope(m.to, target) {
            moves.push(m);
        } else if m.from != () {
            log("Dropped a move that escaped the target: " + m.from);
        }
    }
}
let clusters = [];
if plan.output.clusters != () {
    for c in plan.output.clusters {
        if clusters.len() < 5 {
            clusters.push(c);
        } else {
            log("Dropped extra cluster beyond cap: " + c.id);
        }
    }
}
let allowed_external = [];
if plan.output.allowed_external_paths != () {
    for p in plan.output.allowed_external_paths {
        if type_of(p) == "string" && p.starts_with("erp-client/") && !p.contains("..") && !in_scope(p, target) {
            allowed_external.push(p);
        } else if type_of(p) == "string" {
            log("Dropped a disallowed external path: " + p);
        }
    }
}
if moves.len() == 0 && clusters.len() == 0 {
    complete(#{ status: "no-op", summary: "Plan produced no in-scope moves or splits.", target: target, analysis: plan.output.analysis_summary });
}
log(moves.len().to_string() + " moves and " + clusters.len().to_string() + " split clusters accepted");

let implementation_summaries = [];
let last_impl_id = ();

phase("Relocate");
let relocate_prompt = "Implement ONLY the mechanical file relocation for ";
relocate_prompt += target;
relocate_prompt += ". Read erp-client/AGENTS.md first. Approved plan: ";
relocate_prompt += json_encode(#{ target_tree: plan.output.target_tree, moves: moves, allowed_external_paths: allowed_external, invariants: plan.output.invariants }) + ". ";
relocate_prompt += "Use git mv when possible. After each move, update every import (inside the target and the allowed external files) with grep. ";
relocate_prompt += "Do not rewrite business logic, JSX structure, styles, copy, query keys, or API payloads. ";
relocate_prompt += "Do not perform the split/extract work yet. Delete leftover empty directories under the target. ";
relocate_prompt += "Do not edit backend/, do not add dependencies, do not commit or push. ";
relocate_prompt += "After edits, from erp-client run: npx tsc --noEmit --pretty false ; npx oxfmt on the files you touched. ";
relocate_prompt += "Set completed=false if typecheck fails or a move would change behavior.";
let relocate = agent(relocate_prompt, #{
    label: "relocate",
    agent_type: "general-purpose",
    capability_mode: "all",
    output_schema: implementation_schema,
});
if relocate == () || !relocate.success || relocate.output == () || relocate.output.completed != true {
    complete(#{ status: "blocked", summary: "Relocation failed or was incomplete; no further splits were attempted.", target: target });
}
implementation_summaries.push(relocate.output);
last_impl_id = relocate.agent_id;

let split_index = 0;
for cluster in clusters {
    phase("Split");
    let split_prompt = "Implement ONLY this approved split cluster after the files have already been relocated. ";
    split_prompt += "Target: " + target + ". Cluster: " + json_encode(cluster) + ". ";
    split_prompt += "Related split rows from the plan: " + json_encode(plan.output.splits) + ". ";
    split_prompt += "Read erp-client/AGENTS.md. Inspect the CURRENT files with grep/read_file (paths may have changed). ";
    split_prompt += "Extract mixed UI+logic into hooks/ (hooks only) and lib/ (pure TS) and keep components/ as presentational consumers. ";
    split_prompt += "Single file, single responsibility. Prefer moving existing functions/JSX over rewriting. ";
    split_prompt += "Functional shape must stay identical: no query-key, URL-param, mutation, validator, payload, copy, permission, or layout changes. ";
    split_prompt += "Do not touch backend/. Only edit files under the target plus these allowed external paths: ";
    split_prompt += json_encode(allowed_external) + ". ";
    split_prompt += "Do not add dependencies, do not commit or push. After edits run npx tsc --noEmit --pretty false from erp-client and oxfmt on touched files. ";
    split_prompt += "If a split cannot preserve behavior exactly, skip that file and report it in leftover_issues. Set completed=false only if you left the tree broken.";
    let split_opts = #{
        label: "split:" + split_index.to_string(),
        agent_type: "general-purpose",
        capability_mode: "all",
        output_schema: implementation_schema,
    };
    if last_impl_id != () {
        split_opts.resume_from = last_impl_id;
    }
    let split_result = agent(split_prompt, split_opts);
    if split_result == () || !split_result.success || split_result.output == () || split_result.output.completed != true {
        complete(#{
            status: "blocked",
            summary: "Split cluster failed: " + cluster.id,
            target: target,
            implementations: implementation_summaries,
        });
    }
    implementation_summaries.push(split_result.output);
    last_impl_id = split_result.agent_id;
    split_index += 1;
}

phase("Review");
let review_prompt = "Perform an independent adversarial review of the current working tree for the feature refactor of ";
review_prompt += target + ". ";
review_prompt += "Read erp-client/AGENTS.md. Inspect git status, git diff --name-only, and the actual moved/split files — do not trust summaries. ";
review_prompt += "Plan: " + json_encode(#{ target_tree: plan.output.target_tree, moves: moves, clusters: clusters, invariants: plan.output.invariants }) + ". ";
review_prompt += "Implementation summaries: " + json_encode(implementation_summaries) + ". ";
review_prompt += "Approve only with concrete evidence that: (1) directories/file names are self-describing (hooks, components, lib, api); ";
review_prompt += "(2) UI is separated from hooks/pure logic; (3) files have a single responsibility; ";
review_prompt += "(4) functional shape is unchanged (query keys, URLs, mutations, validators, payloads, copy, permissions, layout); ";
review_prompt += "(5) no logic was dropped or duplicated; (6) scope is only the target plus allowed external import updates ";
review_prompt += json_encode(allowed_external) + "; (7) leftover empty dirs are gone. ";
review_prompt += "Fail closed if evidence is missing. Do not edit files.";
let review = agent(review_prompt, #{
    label: "review",
    agent_type: "general-purpose",
    capability_mode: "read-only",
    output_schema: review_schema,
});
if review == () || !review.success || review.output == () {
    complete(#{ status: "blocked", summary: "Independent review failed to execute.", target: target });
}

if review.output.approved != true {
    phase("Split");
    let repair_prompt = "Repair only the issues found by the independent reviewer. Review findings: ";
    repair_prompt += json_encode(review.output) + ". ";
    repair_prompt += "Keep the approved plan and exact behavior-preservation constraints. Target: " + target + ". ";
    repair_prompt += "Allowed external paths: " + json_encode(allowed_external) + ". ";
    repair_prompt += "Inspect the current git diff before editing. Do not add dependencies, do not commit or push. ";
    repair_prompt += "Run npx tsc --noEmit --pretty false from erp-client and oxfmt on touched files. ";
    repair_prompt += "Set completed=false if behavior cannot be preserved.";
    let repair_opts = #{
        label: "repair",
        agent_type: "general-purpose",
        capability_mode: "all",
        output_schema: implementation_schema,
    };
    if last_impl_id != () {
        repair_opts.resume_from = last_impl_id;
    }
    let repair = agent(repair_prompt, repair_opts);
    if repair == () || !repair.success || repair.output == () || repair.output.completed != true {
        complete(#{ status: "blocked", summary: "Repair after review failed.", target: target, review: review.output });
    }
    implementation_summaries.push(repair.output);
    last_impl_id = repair.agent_id;

    phase("Review");
    let rereview_prompt = "Re-review the repaired feature refactor of " + target + " independently. ";
    rereview_prompt += "Previous findings: " + json_encode(review.output) + ". Inspect the current git diff and code. ";
    rereview_prompt += "Approve only with concrete evidence for structure, behavior preservation, and scope. Do not edit files.";
    review = agent(rereview_prompt, #{
        label: "rereview",
        agent_type: "general-purpose",
        capability_mode: "read-only",
        output_schema: review_schema,
    });
    if review == () || !review.success || review.output == () || review.output.approved != true {
        complete(#{
            status: "blocked",
            summary: "Repaired tree did not pass independent review.",
            target: target,
            review: if review == () { () } else { review.output },
        });
    }
}

phase("Verify");
let verify_prompt = "Run final quality gates for the feature refactor of " + target + " without changing files unless a command is read-only. ";
verify_prompt += "First inspect git status and git diff --name-only. ";
verify_prompt += "scope_clean=true only if every changed path is under " + target + " or in this allowed list: ";
verify_prompt += json_encode(allowed_external) + ". ";
verify_prompt += "behavior_shape_unchanged=true only after reading the diff and confirming no query-key/URL/mutation/validator/payload/copy/permission/layout changes. ";
verify_prompt += "Then from erp-client run: npx tsc --noEmit --pretty false ; npm run lint ; npm run format:check. ";
verify_prompt += "Set all_pass=true only if every command passes AND scope_clean AND behavior_shape_unchanged. Do not commit or push.";
let verification = agent(verify_prompt, #{
    label: "verify",
    agent_type: "lightweight",
    capability_mode: "execute",
    output_schema: verification_schema,
});
if verification == () || !verification.success || verification.output == () {
    complete(#{ status: "blocked", summary: "Final verification failed to execute.", target: target });
}

if verification.output.all_pass != true {
    phase("Split");
    let final_repair_prompt = "Fix only refactor-induced failures from final verification: ";
    final_repair_prompt += json_encode(verification.output) + ". ";
    final_repair_prompt += "Target: " + target + ". Allowed external paths: " + json_encode(allowed_external) + ". ";
    final_repair_prompt += "Preserve behavior exactly. Do not touch backend/. Do not commit or push. ";
    final_repair_prompt += "Re-run the failing commands from erp-client. If a failure is pre-existing or environmental, set completed=false.";
    let final_repair_opts = #{
        label: "final-repair",
        agent_type: "general-purpose",
        capability_mode: "all",
        output_schema: implementation_schema,
    };
    if last_impl_id != () {
        final_repair_opts.resume_from = last_impl_id;
    }
    let final_repair = agent(final_repair_prompt, final_repair_opts);
    if final_repair == () || !final_repair.success || final_repair.output == () || final_repair.output.completed != true {
        complete(#{ status: "blocked", summary: "Final verification failed and could not be safely repaired.", verification: verification.output, target: target });
    }
    implementation_summaries.push(final_repair.output);

    phase("Verify");
    verification = agent(verify_prompt, #{
        label: "verify-rerun",
        agent_type: "lightweight",
        capability_mode: "execute",
        output_schema: verification_schema,
    });
    if verification == () || !verification.success || verification.output == () || verification.output.all_pass != true {
        complete(#{
            status: "blocked",
            summary: "Feature refactor did not pass the final quality gate.",
            target: target,
            verification: if verification == () { () } else { verification.output },
        });
    }
}

let report = "# Feature structure refactor\n\n";
report += "Status: completed\n\n";
report += "Target: `" + target + "`\n\n";
report += "## Inventory\n\n```json\n" + json_encode(inventory.output) + "\n```\n\n";
report += "## Plan\n\n```json\n" + json_encode(plan.output) + "\n```\n\n";
report += "## Implementations\n\n```json\n" + json_encode(implementation_summaries) + "\n```\n\n";
report += "## Review\n\n```json\n" + json_encode(review.output) + "\n```\n\n";
report += "## Verification\n\n```json\n" + json_encode(verification.output) + "\n```\n";
let report_path = write_scratch_file("feature-structure-refactor-report.md", report);
complete(#{
    status: "completed",
    summary: "Feature directory reorganized into self-describing hooks/components/lib/api without changing functional shape.",
    target: target,
    path: report_path,
    review: review.output,
    verification: verification.output,
});
