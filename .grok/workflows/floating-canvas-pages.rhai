// 将 docs/erp-ui-design.md §2.5「浮动画布内容区固定模式」落到全部工作面页面。
//
// Run: /workflow floating-canvas-pages
// Args (optional):
//   batches: "all" | "sales,finance,..."  (default "all")
//   skip_verify: bool                    (default false)

let meta = #{
    name: "floating-canvas-pages",
    description: "Apply Floating Canvas Content pattern (§2.5) to all workspace pages",
    when_to_use: "After shell theme change, when content pages still use max-w-shell hand layout or heavy borders",
    phases: [
        #{ title: "Apply", detail: "Parallel domain batches: PageScaffold + surfaces + light chrome" },
        #{ title: "Verify", detail: "Grep residual max-w-shell / heavy borders; spot-check" },
        #{ title: "Report", detail: "Write summary of files changed" },
    ],
};

let batches_arg = "all";
let skip_verify = false;
if args != () {
    if args.batches != () { batches_arg = args.batches; }
    if args.skip_verify != () { skip_verify = args.skip_verify; }
}

// Domain batches: feature page files that still need §2.5 (exclude already-migrated samples)
let all_batches = [
    #{
        id: "sales",
        files: [
            "erp-client/features/contracts/contracts-list-page.tsx",
            "erp-client/features/contracts/contract-detail-page.tsx",
            "erp-client/features/customers/customer-center-page.tsx",
            "erp-client/features/customers/customer-detail-page.tsx",
            "erp-client/features/sales-orders/sales-order-create-page.tsx",
        ],
        notes: "M2 lists + M4 centers + M5 create; sales list/detail already migrated",
    },
    #{
        id: "procurement",
        files: [
            "erp-client/features/purchase-orders/purchase-orders-list-page.tsx",
            "erp-client/features/purchase-orders/purchase-order-detail-page.tsx",
            "erp-client/features/supplier-catalog/supplier-catalog-page.tsx",
            "erp-client/features/supplier-catalog/supplier-product-center-page.tsx",
            "erp-client/features/supplier-catalog/supply-relationship-list-view.tsx",
        ],
        notes: "Procurement confirm already migrated",
    },
    #{
        id: "supplier-api",
        files: [
            "erp-client/features/supplier-orders/supplier-orders-list-page.tsx",
            "erp-client/features/supplier-orders/supplier-order-center-page.tsx",
            "erp-client/features/supplier-api-connections/supplier-api-connections-page.tsx",
            "erp-client/features/supplier-settlements/supplier-settlements-page.tsx",
        ],
        notes: "Supplier API lists and centers",
    },
    #{
        id: "commerce",
        files: [
            "erp-client/features/mall-consumption-orders/consumption-orders-list-page.tsx",
            "erp-client/features/mall-consumption-orders/consumption-order-center-page.tsx",
            "erp-client/features/product-publications/product-publications-list-page.tsx",
            "erp-client/features/product-publications/publication-center-page.tsx",
            "erp-client/features/execution-projections/execution-projections-page.tsx",
        ],
        notes: "Mall + publications + projections",
    },
    #{
        id: "finance",
        files: [
            "erp-client/features/customer-receivables/customer-receivables-page.tsx",
            "erp-client/features/supplier-payables/supplier-accounts-page.tsx",
            "erp-client/features/card-funds-review/card-funds-review-page.tsx",
            "erp-client/features/card-business-analytics/card-business-analytics-page.tsx",
            "erp-client/features/actual-profit-loss/actual-profit-loss-page.tsx",
            "erp-client/features/customer-quality/customer-quality-page.tsx",
        ],
        notes: "Finance + analytics workspaces",
    },
    #{
        id: "ops-master",
        files: [
            "erp-client/features/fulfillment-operations/fulfillment-operations-page.tsx",
            "erp-client/features/inventory/inventory-ledger-page.tsx",
            "erp-client/features/master-data/master-data-page.tsx",
            "erp-client/features/master-data/master-data-center-page.tsx",
            "erp-client/features/master-data/category-tree-page.tsx",
            "erp-client/features/master-data/product-detail-page.tsx",
            "erp-client/features/master-data/supplier-detail-page.tsx",
            "erp-client/features/unified-task-queue/unified-task-queue-page.tsx",
        ],
        notes: "Fulfillment, inventory, master data, task queue",
    },
    #{
        id: "governance",
        files: [
            "erp-client/features/history-backfill/history-backfill-page.tsx",
            "erp-client/features/import-opening/import-opening-page.tsx",
            "erp-client/features/integration-errors/integration-errors-page.tsx",
            "erp-client/features/mall-sync/mall-sync-page.tsx",
            "erp-client/features/access-audit/access-audit-page.tsx",
            "erp-client/features/admin/role-form-page.tsx",
        ],
        notes: "Governance + admin",
    },
    #{
        id: "app-shells",
        files: [
            "erp-client/app/(workspace)/analytics/card-business/page.tsx",
            "erp-client/app/(workspace)/analytics/customer-quality/page.tsx",
            "erp-client/app/(workspace)/finance/card-funds-review/page.tsx",
            "erp-client/app/(workspace)/finance/customer-accounts/page.tsx",
            "erp-client/app/(workspace)/finance/supplier-accounts/page.tsx",
            "erp-client/app/(workspace)/supplier-api/orders/page.tsx",
            "erp-client/app/(workspace)/supplier-api/orders/[supplierOrderId]/page.tsx",
            "erp-client/app/(workspace)/workspace/page.tsx",
            "erp-client/app/(workspace)/workspace/tasks/page.tsx",
        ],
        notes: "Thin route wrappers / skeletons: align padding to PageScaffold rhythm",
    },
];

fn batch_selected(id, batches_arg) {
    if batches_arg == "all" { return true; }
    let needle = "," + id + ",";
    let hay = "," + batches_arg + ",";
    // simple contains without regex
    let i = 0;
    let n = hay.len();
    let m = needle.len();
    if m > n { return false; }
    while i + m <= n {
        if hay.sub_string(i, m) == needle { return true; }
        i += 1;
    }
    false
}

let selected = [];
for b in all_batches {
    if batch_selected(b.id, batches_arg) {
        selected.push(b);
    }
}

if selected.len() == 0 {
    pause("verification", "No batches selected. Pass batches: all or comma ids e.g. sales,finance");
}

log("Selected " + selected.len().to_string() + " batches for floating-canvas apply");

// --- shared pattern text (kept short; agents must also read the design doc) ---
let pattern_core = ""
    + "Apply Floating Canvas Content pattern from docs/erp-ui-design.md section 2.5.\n"
    + "Reference implementations (DO NOT rewrite unless needed): "
    + "erp-client/features/workspace/workspace-home-page.tsx, "
    + "erp-client/features/sales-orders/sales-orders-list-page.tsx, "
    + "erp-client/features/sales-orders/sales-order-detail-page.tsx, "
    + "erp-client/features/procurement-confirmation/procurement-confirmation-page.tsx.\n"
    + "Rules:\n"
    + "1) Replace hand-written mx-auto max-w-shell wrappers with PageScaffold (import from @/components/business). "
    + "Use density=compact for dense M2 lists when the page was p-3 md:p-4.\n"
    + "2) Main cards/tables: add surfacePanelClassName on Card or rely on BusinessTableFrame (already panel).\n"
    + "3) Replace rounded-2xl border border-border bg-card sticky bars with surfacePanelClassName or surfaceInsetClassName.\n"
    + "4) CardHeader dividers: border-b border-border/30. Avoid heavy border-border stacks.\n"
    + "5) Empty state inside a card: className rounded-lg border-0 bg-transparent shadow-none ring-0; actions use variant=secondary rounded-lg shadow-none.\n"
    + "6) Binary scope toggles: segmented track (bg-muted + selected bg-card shadow) like workspace home — not outline ToggleGroup.\n"
    + "7) Refresh/secondary tools: ghost; one solid primary max per page.\n"
    + "8) Loading skeletons: rounded-lg not rounded-2xl; wrap in PageScaffold.\n"
    + "9) Do NOT change business logic, queries, URL state, or routes. Visual/layout only.\n"
    + "10) Skip files already using PageScaffold throughout unless residual heavy borders remain.\n"
    + "After edits, list changed files and what you did. Use read_file/grep/search_replace/write tools.\n";

phase("Apply");
let apply_jobs = [];
for b in selected {
    let file_list = "";
    for f in b.files {
        file_list += f + "\n";
    }
    let prompt = "";
    prompt += pattern_core;
    prompt += "\nBatch id: " + b.id + "\n";
    prompt += "Notes: " + b.notes + "\n";
    prompt += "Files to process (read each; skip missing):\n" + file_list;
    prompt += "\nReturn a short bullet summary of changes per file. If a file needs no change, say SKIP and why.";
    apply_jobs.push(#{
        prompt: prompt,
        label: "apply:" + b.id,
        capability_mode: "read-write",
    });
}

let apply_results = parallel(apply_jobs);

let apply_summaries = [];
let i = 0;
for r in apply_results {
    let bid = selected[i].id;
    if r == () || !r.success {
        apply_summaries.push("FAIL " + bid);
        log("Apply failed: " + bid);
    } else {
        let out = r.output;
        if type_of(out) != "string" {
            out = json_encode(out);
        }
        apply_summaries.push("OK " + bid + ": " + out);
        log("Apply ok: " + bid);
    }
    i += 1;
}

if !skip_verify {
    phase("Verify");
    let verify_prompt = "";
    verify_prompt += "Verify Floating Canvas migration residual issues under erp-client.\n";
    verify_prompt += "Use grep and read_file only (no large refactors).\n";
    verify_prompt += "1) Count files still matching: mx-auto flex w-full max-w-shell (exclude page.tsx PageScaffold definition).\n";
    verify_prompt += "2) Sample 5 unfixed feature pages if any; list paths.\n";
    verify_prompt += "3) Grep for rounded-2xl border border-border in features/ (heavy sticky bars); list top 15 paths.\n";
    verify_prompt += "4) Confirm PageScaffold import usage grew (grep PageScaffold under features).\n";
    verify_prompt += "5) Do not fix unless trivial one-liner; report only.\n";
    verify_prompt += "Output structured markdown report.\n";
    let v = agent(verify_prompt, #{
        label: "verify-residuals",
        capability_mode: "read-only",
    });
    if v == () || !v.success {
        log("Verify agent failed");
    } else {
        log("Verify completed");
    }
}

phase("Report");
let report = "# Floating Canvas Pages — run report\n\n";
report += "## Pattern\n";
report += "docs/erp-ui-design.md §2.5 Floating Canvas Content\n\n";
report += "## Batches\n";
for s in apply_summaries {
    report += "- " + s + "\n\n";
}
let path = write_scratch_file("floating-canvas-report.md", report);
complete(#{
    path: path,
    batches: selected.len(),
    summary: apply_summaries.len().to_string() + " batch results",
});
