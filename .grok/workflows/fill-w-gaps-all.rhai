// Sequential multi-wave orchestrator: runs fill-w-gaps style pipeline for waves 2–6
// (or args.start_wave..args.end_wave) automatically without re-invoking the workflow.
// Prefer starting after wave 1 finishes to avoid concurrent edits; default start_wave=2.
//
// Args:
//   start_wave: int (default 2)
//   end_wave: int (default 6)
//   skip_verify: bool (default false)
//   wait_for_wave1: bool (default true) — if true and start_wave>1, pause until
//                   docs/ui-workspaces/_wave-1-progress.md exists (resume when ready)

let meta = #{
    name: "fill-w-gaps-all",
    description: "Auto-chain waves of W01–W30 gap filling (inventory→implement→verify→integrate per wave)",
    when_to_use: "After or instead of single-wave fill-w-gaps; chains waves 2–6 (or a range) in one run",
    phases: [
        #{ title: "Gate", detail: "Optional wait for wave 1 progress file" },
        #{ title: "Waves", detail: "Sequential inventory/implement/verify/integrate per wave" },
        #{ title: "Rollup", detail: "Final coverage summary across waves" },
    ],
};

let start_wave = 2;
let end_wave = 6;
let skip_verify = false;
let wait_for_wave1 = true;
if args != () {
    if args.start_wave != () { start_wave = args.start_wave; }
    if args.end_wave != () { end_wave = args.end_wave; }
    if args.skip_verify != () { skip_verify = args.skip_verify; }
    if args.wait_for_wave1 != () { wait_for_wave1 = args.wait_for_wave1; }
}
if start_wave < 1 { start_wave = 1; }
if end_wave > 6 { end_wave = 6; }
if end_wave < start_wave { end_wave = start_wave; }

let wave_map = #{
    "1": ["W01", "W02", "W05", "W06", "W07"],
    "2": ["W03", "W04", "W08", "W09", "W10"],
    "3": ["W11", "W12", "W13", "W14"],
    "4": ["W15", "W16", "W17", "W18", "W19"],
    "5": ["W20", "W21", "W22", "W23", "W24"],
    "6": ["W25", "W26", "W27", "W28", "W29", "W30"],
};

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

let gate_schema = #{
    "type": "object",
    "required": ["ready", "reason"],
    "properties": #{
        "ready": #{ "type": "boolean" },
        "reason": #{ "type": "string" },
        "wave1_progress_exists": #{ "type": "boolean" },
    },
};

let rollup_schema = #{
    "type": "object",
    "required": ["summary", "waves_done", "total_flipped", "open_remaining"],
    "properties": #{
        "summary": #{ "type": "string" },
        "waves_done": #{ "type": "array", "items": #{ "type": "integer" } },
        "total_flipped": #{ "type": "integer" },
        "open_remaining": #{ "type": "integer" },
        "report_path": #{ "type": "string" },
    },
};

// ========== Gate ==========
phase("Gate");

if wait_for_wave1 && start_wave > 1 {
    let gate_prompt = "";
    gate_prompt += "Check whether wave 1 of fill-w-gaps has finished integrating.\n";
    gate_prompt += "1. Look for docs/ui-workspaces/_wave-1-progress.md\n";
    gate_prompt += "2. Optionally list docs/ui-workspaces for _wave-*-progress.md files.\n";
    gate_prompt += "3. ready=true only if _wave-1-progress.md exists and is non-empty.\n";
    gate_prompt += "4. If missing, ready=false and explain that wave 1 fill-w-gaps is still running or failed.\n";
    gate_prompt += "Use list_dir/read_file only.\n";
    let gate = agent(gate_prompt, #{
        label: "gate-wave1",
        capability_mode: "read-only",
        agent_type: "explore",
        output_schema: gate_schema,
    });
    let ready = false;
    if gate != () && gate.success && gate.output != () && gate.output.ready == true {
        ready = true;
    }
    if !ready {
        await_user(
            "user",
            "Waiting for fill-w-gaps wave 1 to finish (docs/ui-workspaces/_wave-1-progress.md). Resume this run when /workflows shows fill-w-gaps complete, or re-run with args.wait_for_wave1=false to proceed immediately."
        );
        // After resume, re-check once; proceed even if still missing (user override).
        let gate2 = agent(gate_prompt, #{
            label: "gate-wave1-recheck",
            capability_mode: "read-only",
            agent_type: "explore",
            output_schema: gate_schema,
        });
        if gate2 != () && gate2.success && gate2.output != () {
            log("Gate recheck: " + gate2.output.reason);
        }
    } else {
        log("Wave 1 progress present — starting chained waves");
    }
}

// Build ordered wave list (Rhai: plain for, no while)
let all_waves = [1, 2, 3, 4, 5, 6];
let wave_list = [];
for n in all_waves {
    if n >= start_wave && n <= end_wave {
        wave_list.push(n);
    }
}

phase("Waves");

let all_wave_summaries = [];
let total_flipped = 0;

for wave_num in wave_list {
    let wave_key = wave_num.to_string();
    let workspaces = wave_map[wave_key];
    if workspaces == () {
        log("Skip unknown wave " + wave_key);
        continue;
    }

    log("===== WAVE " + wave_key + " workspaces " + json_encode(workspaces) + " =====");

    // --- Inventory ---
    let inv_prompt = "";
    inv_prompt += "Inventory open ERP workspace acceptance items for wave ";
    inv_prompt += wave_key;
    inv_prompt += ".\nWorkspaces (cover ALL): ";
    inv_prompt += json_encode(workspaces);
    inv_prompt += "\nFor EACH id: read docs/ui-workspaces/README.md + wXX-*.md; list all `- [ ]` under §12; ";
    inv_prompt += "inspect erp-client implementation; pick ≤12 feasible high-value open items (UI/mock/Query/Form/routes). ";
    inv_prompt += "Exclude pure server lease/fingerprint/idempotent envelopes unless mockable honestly.\n";
    inv_prompt += "Use list_dir, grep, read_file. Return schema workspaces[].\n";

    let inv = agent(inv_prompt, #{
        label: "inv-w" + wave_key,
        capability_mode: "read-only",
        agent_type: "explore",
        output_schema: inventory_schema,
    });

    if inv == () || !inv.success || inv.output == () || inv.output.workspaces == () {
        log("Inventory failed for wave " + wave_key + " — skipping wave");
        all_wave_summaries.push(#{
            wave: wave_num,
            status: "inventory_failed",
        });
        continue;
    }

    let inv_workspaces = inv.output.workspaces;
    write_scratch_file("wave-" + wave_key + "-inventory.json", json_encode(inv.output));

    // --- Implement parallel ---
    let impl_jobs = [];
    for brief in inv_workspaces {
        if brief == () || brief.id == () { continue; }
        let wid = brief.id;
        let p = "";
        p += "IMPLEMENT feasible open acceptance items for workspace ";
        p += wid;
        p += " (wave ";
        p += wave_key;
        p += ").\nInventory brief (re-verify in files):\n";
        p += json_encode(brief);
        p += "\nRULES:\n";
        p += "1. Read full W doc §4/5/7/12 and erp-client/AGENTS.md.\n";
        p += "2. Prefer erp-client/features/<domain>/ specialized pages over bloating mock/workspace-pages.ts.\n";
        p += "3. SPA: TanStack Query + Form; components/business; Chinese UI.\n";
        p += "4. 暂挂 keeps tasks in queue; formal actions use session-mock + useMutation + invalidate.\n";
        p += "5. Do NOT flip W doc checkboxes — Integrate does that after verify.\n";
        p += "6. Avoid editing files unrelated to this workspace; minimize conflicts with other waves.\n";
        p += "7. Run `cd erp-client && npm run build` when feasible.\n";
        p += "Return files_changed, items_addressed, items_still_open, summary, build_ok.\n";
        impl_jobs.push(#{
            prompt: p,
            label: "impl-" + wid,
            capability_mode: "execute",
            agent_type: "general-purpose",
            output_schema: impl_schema,
        });
    }

    let impl_ok = [];
    if impl_jobs.len() == 0 {
        log("No implement jobs wave " + wave_key);
    } else {
        let impl_results = parallel(impl_jobs);
        for r in impl_results {
            if r != () && r.success && r.output != () {
                impl_ok.push(r.output);
            }
        }
    }
    write_scratch_file("wave-" + wave_key + "-implement.json", json_encode(impl_ok));
    log("Wave " + wave_key + " implementers ok: " + impl_ok.len().to_string());

    // --- Verify ---
    let verify_outputs = [];
    if !skip_verify && impl_ok.len() > 0 {
        let vjobs = [];
        for out in impl_ok {
            if out == () || out.workspace_id == () { continue; }
            let vp = "";
            vp += "ADVERSARIAL verify workspace ";
            vp += out.workspace_id;
            vp += ".\nClaims:\n";
            vp += json_encode(out);
            vp += "\nConfirm only with shipped-code evidence. Reject theater/local-only formal claims.\n";
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
    write_scratch_file("wave-" + wave_key + "-verify.json", json_encode(verify_outputs));

    // --- Integrate ---
    let next_w = wave_num + 1;
    let int_prompt = "";
    int_prompt += "INTEGRATE wave ";
    int_prompt += wave_key;
    int_prompt += " of fill-w-gaps-all.\n";
    int_prompt += "Verify JSON:\n";
    int_prompt += json_encode(verify_outputs);
    int_prompt += "\nImplement JSON:\n";
    int_prompt += json_encode(impl_ok);
    int_prompt += "\nTASKS:\n";
    int_prompt += "1. `cd erp-client && npm run build`\n";
    int_prompt += "2. Flip ONLY verify.confirmed checklist lines `- [ ]`→`- [x]` in docs/ui-workspaces/wXX-*.md\n";
    int_prompt += "3. Write docs/ui-workspaces/_wave-";
    int_prompt += wave_key;
    int_prompt += "-progress.md with confirmed/rejected/files/build/open counts\n";
    int_prompt += "4. Return report_path, checkboxes_flipped, build_ok, remaining_estimate, next_wave=";
    int_prompt += next_w.to_string();
    int_prompt += ", summary\n";

    let integ = agent(int_prompt, #{
        label: "integrate-w" + wave_key,
        capability_mode: "execute",
        agent_type: "general-purpose",
        output_schema: integrate_schema,
    });

    let flipped = 0;
    let build_ok = false;
    let status = "integrate_failed";
    if integ != () && integ.success && integ.output != () {
        status = "ok";
        if integ.output.checkboxes_flipped != () {
            flipped = integ.output.checkboxes_flipped;
        }
        if integ.output.build_ok == true {
            build_ok = true;
        }
        total_flipped += flipped;
    }

    all_wave_summaries.push(#{
        wave: wave_num,
        status: status,
        implementers_ok: impl_ok.len(),
        verifiers_ok: verify_outputs.len(),
        checkboxes_flipped: flipped,
        build_ok: build_ok,
    });
    write_scratch_file("wave-" + wave_key + "-final.json", json_encode(all_wave_summaries[all_wave_summaries.len() - 1]));
}

// ========== Rollup ==========
phase("Rollup");

let roll_prompt = "";
roll_prompt += "Roll up fill-w-gaps-all results for waves ";
roll_prompt += start_wave.to_string();
roll_prompt += "..";
roll_prompt += end_wave.to_string();
roll_prompt += ".\nPer-wave summaries:\n";
roll_prompt += json_encode(all_wave_summaries);
roll_prompt += "\n1. Grep docs/ui-workspaces/w*.md for counts of `- [x]` vs `- [ ]`.\n";
roll_prompt += "2. Write docs/ui-workspaces/_waves-2-6-rollup.md (or matching range) with totals and remaining gaps.\n";
roll_prompt += "3. Return summary, waves_done, total_flipped, open_remaining, report_path.\n";

let roll = agent(roll_prompt, #{
    label: "rollup-all",
    capability_mode: "execute",
    agent_type: "general-purpose",
    output_schema: rollup_schema,
});

let final_path = write_scratch_file(
    "all-waves-final.json",
    json_encode(#{
        start_wave: start_wave,
        end_wave: end_wave,
        wave_summaries: all_wave_summaries,
        total_flipped_tracked: total_flipped,
        rollup: if roll != () && roll.success { roll.output } else { () },
    })
);

if roll != () && roll.success && roll.output != () {
    complete(#{
        summary: roll.output.summary,
        path: final_path,
        waves: all_wave_summaries,
        total_flipped: roll.output.total_flipped,
        open_remaining: roll.output.open_remaining,
    });
}

complete(#{
    summary: "Waves " + start_wave.to_string() + "-" + end_wave.to_string() + " finished with partial rollup",
    path: final_path,
    waves: all_wave_summaries,
    total_flipped_tracked: total_flipped,
});
