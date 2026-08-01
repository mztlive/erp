// Multi-phase multi-agent pipeline: fill open W01–W30 acceptance gaps in waves.
// Run: /workflow fill-w-gaps  OR  workflow tool with name "fill-w-gaps"
// Args:
//   wave: 1..6 (default 1) — which workspace cluster to process this run
//   max_agents_per_w: optional, default 1 implementer per workspace
//   skip_verify: optional bool, default false

let meta = #{
    name: "fill-w-gaps",
    description: "Multi-phase agents close open W01–W30 acceptance checklist items by wave",
    when_to_use: "When ERP workspace pages need deeper fidelity against docs/ui-workspaces §12",
    phases: [
        #{ title: "Inventory", detail: "Map open checklist items for this wave" },
        #{ title: "Implement", detail: "Parallel implementers per workspace" },
        #{ title: "Verify", detail: "Adversarial check of claimed completions" },
        #{ title: "Integrate", detail: "Build, mark honest [x], write report" },
    ],
};

// --- args ---
let wave = 1;
let skip_verify = false;
if args != () {
    if args.wave != () { wave = args.wave; }
    if args.skip_verify != () { skip_verify = args.skip_verify; }
}

// Workspace waves (priority: core sales/procurement first)
let wave_map = #{
    "1": ["W01", "W02", "W05", "W06", "W07"],
    "2": ["W03", "W04", "W08", "W09", "W10"],
    "3": ["W11", "W12", "W13", "W14"],
    "4": ["W15", "W16", "W17", "W18", "W19"],
    "5": ["W20", "W21", "W22", "W23", "W24"],
    "6": ["W25", "W26", "W27", "W28", "W29", "W30"],
};
let wave_key = wave.to_string();
let workspaces = wave_map[wave_key];
if workspaces == () {
    pause("verification", "Pass args.wave as integer 1..6 (got unsupported wave).");
}

log("fill-w-gaps wave " + wave_key + " workspaces: " + json_encode(workspaces));

// --- schemas ---
let inventory_schema = #{
    "type": "object",
    "required": ["workspaces"],
    "properties": #{
        "workspaces": #{
            "type": "array",
            "items": #{
                "type": "object",
                "required": ["id", "open_count", "priority_items", "current_impl_summary", "doc_path"],
                "properties": #{
                    "id": #{ "type": "string" },
                    "doc_path": #{ "type": "string" },
                    "open_count": #{ "type": "integer" },
                    "priority_items": #{
                        "type": "array",
                        "maxItems": 12,
                        "items": #{ "type": "string" },
                    },
                    "current_impl_summary": #{ "type": "string" },
                    "mode": #{ "type": "string" },
                    "main_route": #{ "type": "string" },
                },
            },
        },
        "notes": #{ "type": "string" },
    },
};

let impl_schema = #{
    "type": "object",
    "required": ["workspace_id", "files_changed", "items_addressed", "items_still_open", "summary"],
    "properties": #{
        "workspace_id": #{ "type": "string" },
        "files_changed": #{ "type": "array", "items": #{ "type": "string" } },
        "items_addressed": #{
            "type": "array",
            "maxItems": 20,
            "items": #{
                "type": "object",
                "required": ["item", "how", "evidence"],
                "properties": #{
                    "item": #{ "type": "string" },
                    "how": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                },
            },
        },
        "items_still_open": #{ "type": "array", "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
        "build_ok": #{ "type": "boolean" },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["workspace_id", "confirmed", "rejected", "notes"],
    "properties": #{
        "workspace_id": #{ "type": "string" },
        "confirmed": #{
            "type": "array",
            "items": #{
                "type": "object",
                "required": ["item", "evidence"],
                "properties": #{
                    "item": #{ "type": "string" },
                    "evidence": #{ "type": "string" },
                },
            },
        },
        "rejected": #{
            "type": "array",
            "items": #{
                "type": "object",
                "required": ["item", "reason"],
                "properties": #{
                    "item": #{ "type": "string" },
                    "reason": #{ "type": "string" },
                },
            },
        },
        "notes": #{ "type": "string" },
    },
};

let integrate_schema = #{
    "type": "object",
    "required": ["report_path", "checkboxes_flipped", "build_ok", "remaining_estimate", "next_wave"],
    "properties": #{
        "report_path": #{ "type": "string" },
        "checkboxes_flipped": #{ "type": "integer" },
        "build_ok": #{ "type": "boolean" },
        "remaining_estimate": #{ "type": "integer" },
        "next_wave": #{ "type": "integer" },
        "summary": #{ "type": "string" },
    },
};

// ========== Phase 1: Inventory ==========
phase("Inventory");

let inv_prompt = "";
inv_prompt += "You are inventorying open ERP workspace acceptance items for wave ";
inv_prompt += wave_key;
inv_prompt += ".\n\n";
inv_prompt += "Workspaces in this wave (MUST cover ALL of them): ";
inv_prompt += json_encode(workspaces);
inv_prompt += "\n\n";
inv_prompt += "Repo root is the current workspace (erp monorepo).\n";
inv_prompt += "For EACH workspace id in the list:\n";
inv_prompt += "1. Read docs/ui-workspaces/README.md index row for mode + main route.\n";
inv_prompt += "2. Find and read the matching docs/ui-workspaces/wXX-*.md file.\n";
inv_prompt += "3. Extract ALL lines matching `- [ ]` under section 12 (验收清单). Count open items.\n";
inv_prompt += "4. Inspect current implementation under erp-client/app/(workspace)/, ";
inv_prompt += "erp-client/features/, erp-client/mock/workspace-pages.ts and specialized features ";
inv_prompt += "(workspace, sales-orders, procurement-confirmation, workspace-kit).\n";
inv_prompt += "5. Pick up to 12 HIGHEST-VALUE open checklist items that are FEASIBLE without a live backend ";
inv_prompt += "(UI layout, labels, filters, mock+Query/Form, client routes, session-mock formal UX). ";
inv_prompt += "Exclude pure server lease/fingerprint/idempotent envelope items unless you can mock the contract honestly.\n";
inv_prompt += "6. Write current_impl_summary: what exists today (shell/fixture vs specialized).\n\n";
inv_prompt += "Use tools: list_dir, grep, read_file. Do NOT invent open items — quote checklist text.\n";
inv_prompt += "Return JSON matching the schema for every workspace in the wave.\n";

let inv = agent(inv_prompt, #{
    label: "inventory-wave-" + wave_key,
    capability_mode: "read-only",
    agent_type: "explore",
    output_schema: inventory_schema,
});

if inv == () || !inv.success || inv.output == () || inv.output.workspaces == () {
    pause("infra", "Inventory agent failed — resume to retry or inspect logs.");
}

let inv_workspaces = inv.output.workspaces;
log("Inventory returned " + inv_workspaces.len().to_string() + " workspace briefs");

// Write inventory artifact
let inv_json = json_encode(inv.output);
let inv_path = write_scratch_file("wave-" + wave_key + "-inventory.json", inv_json);

// ========== Phase 2: Implement (parallel per workspace) ==========
phase("Implement");

let impl_jobs = [];
for brief in inv_workspaces {
    if brief == () || brief.id == () { continue; }
    let wid = brief.id;
    let p = "";
    p += "You are IMPLEMENTING missing default-view and feasible acceptance items for ERP workspace ";
    p += wid;
    p += ".\n\n";
    p += "CONTEXT (from inventory — re-verify in files, do not trust blindly):\n";
    p += json_encode(brief);
    p += "\n\n";
    p += "RULES (mandatory):\n";
    p += "1. Read the full W doc under docs/ui-workspaces/ for ";
    p += wid;
    p += " (layout §4, fields §5, ops §7, acceptance §12).\n";
    p += "2. Read erp-client/AGENTS.md: SPA client pages, TanStack Query for data, TanStack Form for forms, ";
    p += "use components/business primitives, Chinese UI copy, no SSR business fetch.\n";
    p += "3. Prefer extending specialized features under erp-client/features/<domain>/ over bloating ";
    p += "mock/workspace-pages.ts. If still on shared shells, enrich domain fixtures + page composition.\n";
    p += "4. Implement as many priority_items as you can HONESTLY this turn. Quality over checkbox spam.\n";
    p += "5. For formal actions: use session-mock + useMutation + invalidateQueries when no backend; ";
    p += "never claim lease/version/fingerprint/idempotent envelopes unless mock contract is explicit.\n";
    p += "6. 暂挂 must keep tasks in the active queue (held status). Export: if you cannot do server export, ";
    p += "implement an honest client export path OR a mock 'export job' UI with audit label — do not lie in docs.\n";
    p += "7. After code changes, run targeted verification: ";
    p += "`cd erp-client && npm run build` (or tsc) for type safety when feasible.\n";
    p += "8. DO NOT flip `- [ ]` to `- [x]` in W docs yourself — the Integrate agent does that only for verified items.\n";
    p += "9. capability: you MAY edit files under erp-client/ and docs only if needed for fixture labels; ";
    p += "prefer erp-client code.\n\n";
    p += "Deliver code changes on disk. Return structured output: files_changed, items_addressed ";
    p += "(with how + evidence paths/lines), items_still_open, summary, build_ok.\n";

    impl_jobs.push(#{
        prompt: p,
        label: "impl-" + wid,
        capability_mode: "execute",
        agent_type: "general-purpose",
        output_schema: impl_schema,
    });
}

if impl_jobs.len() == 0 {
    complete(#{
        summary: "No workspaces to implement in wave " + wave_key,
        inventory_path: inv_path,
    });
}

log("Launching " + impl_jobs.len().to_string() + " implementers");
let impl_results = parallel(impl_jobs);

let impl_ok = [];
let impl_fail_ids = [];
let i = 0;
for r in impl_results {
    if r != () && r.success && r.output != () {
        impl_ok.push(r.output);
    } else {
        // recover workspace id from brief order
        if i < inv_workspaces.len() && inv_workspaces[i] != () {
            impl_fail_ids.push(inv_workspaces[i].id);
        }
    }
    i += 1;
}
log("Implementers succeeded: " + impl_ok.len().to_string() + "/" + impl_jobs.len().to_string());

let impl_path = write_scratch_file(
    "wave-" + wave_key + "-implement.json",
    json_encode(impl_ok)
);

// ========== Phase 3: Verify (adversarial, parallel) ==========
phase("Verify");

let verify_outputs = [];
if skip_verify {
    log("skip_verify=true — treating implementer claims as unconfirmed");
} else {
    let vjobs = [];
    for out in impl_ok {
        if out == () || out.workspace_id == () { continue; }
        let vp = "";
        vp += "You are an ADVERSARIAL verifier for workspace ";
        vp += out.workspace_id;
        vp += ".\n\n";
        vp += "Implementer claims (untrusted):\n";
        vp += json_encode(out);
        vp += "\n\n";
        vp += "Independently inspect erp-client source and the W doc acceptance list.\n";
        vp += "For EACH items_addressed entry:\n";
        vp += "- confirmed: only if you can point to concrete shipped code behavior that satisfies the checklist text.\n";
        vp += "- rejected: if claim is UI theater, local-only state presented as formal, missing route, or not in code.\n";
        vp += "Do NOT confirm items about real backend lease/version/fingerprint/idempotent envelopes unless mock is explicit and UI honest.\n";
        vp += "Use read_file/grep. Empty confirmed is valid after inspection.\n";
        vp += "Return confirmed[], rejected[], notes.\n";
        vjobs.push(#{
            prompt: vp,
            label: "verify-" + out.workspace_id,
            capability_mode: "read-only",
            agent_type: "explore",
            output_schema: verify_schema,
        });
    }
    if vjobs.len() > 0 {
        let vresults = parallel(vjobs);
        for vr in vresults {
            if vr != () && vr.success && vr.output != () {
                verify_outputs.push(vr.output);
            }
        }
    }
}

let verify_path = write_scratch_file(
    "wave-" + wave_key + "-verify.json",
    json_encode(verify_outputs)
);

// ========== Phase 4: Integrate ==========
phase("Integrate");

let next_wave = wave + 1;
if next_wave > 6 { next_wave = 6; }

let int_prompt = "";
int_prompt += "You are the INTEGRATE agent for fill-w-gaps wave ";
int_prompt += wave_key;
int_prompt += ".\n\n";
int_prompt += "Inputs (read these scratch/files + re-check code):\n";
int_prompt += "- Implement results already on disk under erp-client (do not trust JSON alone).\n";
int_prompt += "- Verify outputs JSON:\n";
int_prompt += json_encode(verify_outputs);
int_prompt += "\n- Implement outputs JSON:\n";
int_prompt += json_encode(impl_ok);
int_prompt += "\n\n";
int_prompt += "TASKS:\n";
int_prompt += "1. Run `cd erp-client && npm run build` and capture success/failure.\n";
int_prompt += "2. For each verify.confirmed item only: in the matching docs/ui-workspaces/wXX-*.md, ";
int_prompt += "flip the matching `- [ ]` line to `- [x]` if the checklist text matches. ";
int_prompt += "Never flip rejected or unverified items. Prefer exact string match on the checklist line.\n";
int_prompt += "3. Write a durable markdown report to docs/ui-workspaces/_wave-" + wave_key + "-progress.md ";
int_prompt += "with: wave workspaces, confirmed items, rejected items, files changed, build status, ";
int_prompt += "remaining open counts (grep), recommended next_wave.\n";
int_prompt += "4. Optionally append a short bullet to erp-client if needed — prefer docs progress file.\n";
int_prompt += "5. Return report_path, checkboxes_flipped, build_ok, remaining_estimate (open [ ] across all W if cheap else this wave), next_wave, summary.\n";

let integ = agent(int_prompt, #{
    label: "integrate-wave-" + wave_key,
    capability_mode: "execute",
    agent_type: "general-purpose",
    output_schema: integrate_schema,
});

let report = #{
    wave: wave,
    workspaces: workspaces,
    inventory_path: inv_path,
    implement_path: impl_path,
    verify_path: verify_path,
    implementers_ok: impl_ok.len(),
    implementers_total: impl_jobs.len(),
    verifiers_ok: verify_outputs.len(),
    integrate: if integ != () && integ.success { integ.output } else { () },
    next_wave: next_wave,
    hint: "Re-run with args #{ wave: " + next_wave.to_string() + " } for the next cluster. Waves 1..6 cover W01–W30.",
};

let report_path = write_scratch_file(
    "wave-" + wave_key + "-final.json",
    json_encode(report)
);

if integ == () || !integ.success {
    complete(#{
        summary: "Wave " + wave_key + " implemented partially; integrate agent failed — see implement/verify scratch JSON",
        path: report_path,
        report: report,
    });
}

complete(#{
    summary: "Wave " + wave_key + " complete. " + integ.output.summary,
    path: report_path,
    report: report,
    checkboxes_flipped: integ.output.checkboxes_flipped,
    build_ok: integ.output.build_ok,
    next_wave: integ.output.next_wave,
});
