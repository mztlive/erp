let meta = #{
    name: "drop-page-reexports",
    description: "Rewrite app routes to import feature pages from pages/, then delete root *-page.ts re-export shims",
    when_to_use: "when erp-client feature-root page re-exports should be removed in favor of direct pages/ imports",
    phases: [
        #{ title: "Apply", detail: "update imports and delete shims in four shards" },
        #{ title: "Verify", detail: "grep leftover shims/imports and lint" },
        #{ title: "Fix", detail: "repair leftovers if verification failed" },
    ],
};

let apply_schema = #{
    "type": "object",
    "required": ["ok", "updated_imports", "deleted_shims", "skipped", "errors"],
    "properties": #{
        "ok": #{ "type": "boolean" },
        "updated_imports": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "deleted_shims": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "skipped": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
        "errors": #{ "type": "array", "maxItems": 20, "items": #{ "type": "string" } },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["ok", "leftover_shims", "leftover_imports", "lint_ok", "notes"],
    "properties": #{
        "ok": #{ "type": "boolean" },
        "leftover_shims": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "leftover_imports": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "lint_ok": #{ "type": "boolean" },
        "notes": #{ "type": "string" },
    },
};

fn item(shim, impl, old_import, new_import) {
    #{
        shim: shim,
        impl: impl,
        old_import: old_import,
        new_import: new_import,
    }
}

let shard_auth = [
    item("erp-client/features/auth/login-page.ts", "erp-client/features/auth/pages/login-page.tsx", "@/features/auth/login-page", "@/features/auth/pages/login-page"),
    item("erp-client/features/admin/role-form-page.ts", "erp-client/features/admin/pages/role-form-page.tsx", "@/features/admin/role-form-page", "@/features/admin/pages/role-form-page"),
    item("erp-client/features/access-audit/access-audit-page.ts", "erp-client/features/access-audit/pages/access-audit-page.tsx", "@/features/access-audit/access-audit-page", "@/features/access-audit/pages/access-audit-page"),
    item("erp-client/features/workspace/workspace-home-page.ts", "erp-client/features/workspace/pages/workspace-home-page.tsx", "@/features/workspace/workspace-home-page", "@/features/workspace/pages/workspace-home-page"),
    item("erp-client/features/unified-task-queue/unified-task-queue-page.ts", "erp-client/features/unified-task-queue/pages/unified-task-queue-page.tsx", "@/features/unified-task-queue/unified-task-queue-page", "@/features/unified-task-queue/pages/unified-task-queue-page"),
];

let shard_sales = [
    item("erp-client/features/customers/customer-center-page.ts", "erp-client/features/customers/pages/customer-center-page.tsx", "@/features/customers/customer-center-page", "@/features/customers/pages/customer-center-page"),
    item("erp-client/features/customers/customer-detail-page.ts", "erp-client/features/customers/pages/customer-detail-page.tsx", "@/features/customers/customer-detail-page", "@/features/customers/pages/customer-detail-page"),
    item("erp-client/features/contracts/contracts-list-page.ts", "erp-client/features/contracts/pages/contracts-list-page.tsx", "@/features/contracts/contracts-list-page", "@/features/contracts/pages/contracts-list-page"),
    item("erp-client/features/contracts/contract-detail-page.ts", "erp-client/features/contracts/pages/contract-detail-page.tsx", "@/features/contracts/contract-detail-page", "@/features/contracts/pages/contract-detail-page"),
    item("erp-client/features/product-publications/product-publications-list-page.ts", "erp-client/features/product-publications/pages/product-publications-list-page.tsx", "@/features/product-publications/product-publications-list-page", "@/features/product-publications/pages/product-publications-list-page"),
    item("erp-client/features/product-publications/publication-center-page.ts", "erp-client/features/product-publications/pages/publication-center-page.tsx", "@/features/product-publications/publication-center-page", "@/features/product-publications/pages/publication-center-page"),
    item("erp-client/features/mall-consumption-orders/consumption-orders-list-page.ts", "erp-client/features/mall-consumption-orders/pages/consumption-orders-list-page.tsx", "@/features/mall-consumption-orders/consumption-orders-list-page", "@/features/mall-consumption-orders/pages/consumption-orders-list-page"),
    item("erp-client/features/mall-consumption-orders/consumption-order-center-page.ts", "erp-client/features/mall-consumption-orders/pages/consumption-order-center-page.tsx", "@/features/mall-consumption-orders/consumption-order-center-page", "@/features/mall-consumption-orders/pages/consumption-order-center-page"),
    item("erp-client/features/execution-projections/execution-projections-page.ts", "erp-client/features/execution-projections/pages/execution-projections-page.tsx", "@/features/execution-projections/execution-projections-page", "@/features/execution-projections/pages/execution-projections-page"),
];

let shard_procurement = [
    item("erp-client/features/purchase-orders/purchase-orders-list-page.ts", "erp-client/features/purchase-orders/pages/purchase-orders-list-page.tsx", "@/features/purchase-orders/purchase-orders-list-page", "@/features/purchase-orders/pages/purchase-orders-list-page"),
    item("erp-client/features/purchase-orders/purchase-order-detail-page.ts", "erp-client/features/purchase-orders/pages/purchase-order-detail-page.tsx", "@/features/purchase-orders/purchase-order-detail-page", "@/features/purchase-orders/pages/purchase-order-detail-page"),
    item("erp-client/features/procurement-confirmation/procurement-confirmation-page.ts", "erp-client/features/procurement-confirmation/pages/procurement-confirmation-page.tsx", "@/features/procurement-confirmation/procurement-confirmation-page", "@/features/procurement-confirmation/pages/procurement-confirmation-page"),
    item("erp-client/features/supplier-offerings/supplier-offerings-page.ts", "erp-client/features/supplier-offerings/pages/supplier-offerings-page.tsx", "@/features/supplier-offerings/supplier-offerings-page", "@/features/supplier-offerings/pages/supplier-offerings-page"),
    item("erp-client/features/supplier-orders/supplier-orders-list-page.ts", "erp-client/features/supplier-orders/pages/supplier-orders-list-page.tsx", "@/features/supplier-orders/supplier-orders-list-page", "@/features/supplier-orders/pages/supplier-orders-list-page"),
    item("erp-client/features/supplier-orders/supplier-order-center-page.ts", "erp-client/features/supplier-orders/pages/supplier-order-center-page.tsx", "@/features/supplier-orders/supplier-order-center-page", "@/features/supplier-orders/pages/supplier-order-center-page"),
    item("erp-client/features/supplier-api-connections/supplier-api-connections-page.ts", "erp-client/features/supplier-api-connections/pages/supplier-api-connections-page.tsx", "@/features/supplier-api-connections/supplier-api-connections-page", "@/features/supplier-api-connections/pages/supplier-api-connections-page"),
    item("erp-client/features/supplier-settlements/supplier-settlements-page.ts", "erp-client/features/supplier-settlements/pages/supplier-settlements-page.tsx", "@/features/supplier-settlements/supplier-settlements-page", "@/features/supplier-settlements/pages/supplier-settlements-page"),
    item("erp-client/features/supplier-payables/supplier-accounts-page.ts", "erp-client/features/supplier-payables/pages/supplier-accounts-page.tsx", "@/features/supplier-payables/supplier-accounts-page", "@/features/supplier-payables/pages/supplier-accounts-page"),
];

let shard_ops = [
    item("erp-client/features/actual-profit-loss/actual-profit-loss-page.ts", "erp-client/features/actual-profit-loss/pages/actual-profit-loss-page.tsx", "@/features/actual-profit-loss/actual-profit-loss-page", "@/features/actual-profit-loss/pages/actual-profit-loss-page"),
    item("erp-client/features/card-business-analytics/card-business-analytics-page.ts", "erp-client/features/card-business-analytics/pages/card-business-analytics-page.tsx", "@/features/card-business-analytics/card-business-analytics-page", "@/features/card-business-analytics/pages/card-business-analytics-page"),
    item("erp-client/features/card-funds-review/card-funds-review-page.ts", "erp-client/features/card-funds-review/pages/card-funds-review-page.tsx", "@/features/card-funds-review/card-funds-review-page", "@/features/card-funds-review/pages/card-funds-review-page"),
    item("erp-client/features/customer-quality/customer-quality-page.ts", "erp-client/features/customer-quality/pages/customer-quality-page.tsx", "@/features/customer-quality/customer-quality-page", "@/features/customer-quality/pages/customer-quality-page"),
    item("erp-client/features/customer-receivables/customer-receivables-page.ts", "erp-client/features/customer-receivables/pages/customer-receivables-page.tsx", "@/features/customer-receivables/customer-receivables-page", "@/features/customer-receivables/pages/customer-receivables-page"),
    item("erp-client/features/history-backfill/history-backfill-page.ts", "erp-client/features/history-backfill/pages/history-backfill-page.tsx", "@/features/history-backfill/history-backfill-page", "@/features/history-backfill/pages/history-backfill-page"),
    item("erp-client/features/import-opening/import-opening-page.ts", "erp-client/features/import-opening/pages/import-opening-page.tsx", "@/features/import-opening/import-opening-page", "@/features/import-opening/pages/import-opening-page"),
    item("erp-client/features/integration-errors/integration-errors-page.ts", "erp-client/features/integration-errors/pages/integration-errors-page.tsx", "@/features/integration-errors/integration-errors-page", "@/features/integration-errors/pages/integration-errors-page"),
    item("erp-client/features/integration-errors/integration-error-detail-page.ts", "erp-client/features/integration-errors/pages/integration-error-detail-page.tsx", "@/features/integration-errors/integration-error-detail-page", "@/features/integration-errors/pages/integration-error-detail-page"),
    item("erp-client/features/inventory/inventory-ledger-page.ts", "erp-client/features/inventory/pages/inventory-ledger-page.tsx", "@/features/inventory/inventory-ledger-page", "@/features/inventory/pages/inventory-ledger-page"),
    item("erp-client/features/mall-sync/mall-sync-page.ts", "erp-client/features/mall-sync/pages/mall-sync-page.tsx", "@/features/mall-sync/mall-sync-page", "@/features/mall-sync/pages/mall-sync-page"),
    item("erp-client/features/fulfillment-operations/fulfillment-operations-page.ts", "erp-client/features/fulfillment-operations/pages/fulfillment-operations-page.tsx", "@/features/fulfillment-operations/fulfillment-operations-page", "@/features/fulfillment-operations/pages/fulfillment-operations-page"),
];

fn apply_prompt(shard_name, items) {
    let p = "Edit the repo at /Users/huangjiajiang/Development/erp.";
    p += " Remove feature-root page re-export shims and point consumers at pages/ implementations.";
    p += " Use grep, read_file, and search_replace. Read each shim before deleting it.";
    p += " Work ONLY this shard: ";
    p += shard_name;
    p += "\n\nItems JSON:\n";
    p += json_encode(items);
    p += "\n\nFor each item:";
    p += " 1) Read the shim. It must only re-export from pages/ (comments and 'use client' are fine).";
    p += " If it has real logic besides re-export, skip it and list it in skipped.";
    p += " 2) Confirm the impl file exists.";
    p += " 3) Grep the whole repo for the old_import specifier.";
    p += " 4) In every matching import, change old_import to new_import. Keep named bindings and aliases.";
    p += " 5) Delete the shim file after importers are updated.";
    p += "\nDo not touch api.ts, queries.ts, dialog re-exports, hooks, master-data, or page implementations.";
    p += " Do not edit docs unless they contain a TypeScript import of old_import.";
    p += " Do not change behavior. Return updated_imports, deleted_shims, skipped, errors.";
    p
}

fn leftover_import_needles() {
    [
        "@/features/auth/login-page\"",
        "@/features/admin/role-form-page\"",
        "@/features/access-audit/access-audit-page\"",
        "@/features/workspace/workspace-home-page\"",
        "@/features/unified-task-queue/unified-task-queue-page\"",
        "@/features/customers/customer-center-page\"",
        "@/features/customers/customer-detail-page\"",
        "@/features/contracts/contracts-list-page\"",
        "@/features/contracts/contract-detail-page\"",
        "@/features/product-publications/product-publications-list-page\"",
        "@/features/product-publications/publication-center-page\"",
        "@/features/mall-consumption-orders/consumption-orders-list-page\"",
        "@/features/mall-consumption-orders/consumption-order-center-page\"",
        "@/features/execution-projections/execution-projections-page\"",
        "@/features/purchase-orders/purchase-orders-list-page\"",
        "@/features/purchase-orders/purchase-order-detail-page\"",
        "@/features/procurement-confirmation/procurement-confirmation-page\"",
        "@/features/supplier-offerings/supplier-offerings-page\"",
        "@/features/supplier-orders/supplier-orders-list-page\"",
        "@/features/supplier-orders/supplier-order-center-page\"",
        "@/features/supplier-api-connections/supplier-api-connections-page\"",
        "@/features/supplier-settlements/supplier-settlements-page\"",
        "@/features/supplier-payables/supplier-accounts-page\"",
        "@/features/actual-profit-loss/actual-profit-loss-page\"",
        "@/features/card-business-analytics/card-business-analytics-page\"",
        "@/features/card-funds-review/card-funds-review-page\"",
        "@/features/customer-quality/customer-quality-page\"",
        "@/features/customer-receivables/customer-receivables-page\"",
        "@/features/history-backfill/history-backfill-page\"",
        "@/features/import-opening/import-opening-page\"",
        "@/features/integration-errors/integration-errors-page\"",
        "@/features/integration-errors/integration-error-detail-page\"",
        "@/features/inventory/inventory-ledger-page\"",
        "@/features/mall-sync/mall-sync-page\"",
        "@/features/fulfillment-operations/fulfillment-operations-page\"",
    ]
}

fn verify_prompt() {
    let p = "Verify the drop-page-reexports change in /Users/huangjiajiang/Development/erp.";
    p += " Use grep and read_file. Then run lint in erp-client.";
    p += "\n1) Confirm these shim files no longer exist (list any that still exist in leftover_shims):";
    p += " erp-client/features/*/*-page.ts at feature root that only re-export from ./pages or @/features/<x>/pages.";
    p += " Grep erp-client/features for files named *-page.ts that are NOT under a pages/ directory.";
    p += " Ignore non-page files. Known expected leftovers: none.";
    p += "\n2) Grep leftover old import specifiers. Needles JSON:\n";
    p += json_encode(leftover_import_needles());
    p += "\nAny match that is NOT already /pages/ is a leftover_import.";
    p += " Imports of hooks like use-*-page or api/fetch-page are NOT leftovers.";
    p += "\n3) In erp-client run: npm run lint";
    p += " Set lint_ok from that command. Set ok=true only if leftover_shims and leftover_imports are empty AND lint_ok is true.";
    p += " Put command output summary in notes.";
    p
}

fn fix_prompt(verify_json) {
    let p = "Fix leftovers after dropping page re-exports in /Users/huangjiajiang/Development/erp.";
    p += " Verification result JSON:\n";
    p += verify_json;
    p += "\nFor each leftover_import: rewrite the specifier to insert /pages/ before the file name";
    p += " (example: @/features/auth/login-page -> @/features/auth/pages/login-page).";
    p += " For each leftover_shim: confirm it is only a re-export, then delete it after importers are updated.";
    p += " Do not touch api.ts, queries.ts, dialogs, hooks, or page implementations.";
    p += " Re-run grep to confirm leftovers are gone.";
    p
}

phase("Apply");
let jobs = [
    #{
        prompt: apply_prompt("auth-system", shard_auth),
        label: "apply:auth-system",
        capability_mode: "all",
        output_schema: apply_schema,
    },
    #{
        prompt: apply_prompt("sales", shard_sales),
        label: "apply:sales",
        capability_mode: "all",
        output_schema: apply_schema,
    },
    #{
        prompt: apply_prompt("procurement", shard_procurement),
        label: "apply:procurement",
        capability_mode: "all",
        output_schema: apply_schema,
    },
    #{
        prompt: apply_prompt("ops", shard_ops),
        label: "apply:ops",
        capability_mode: "all",
        output_schema: apply_schema,
    },
];
let applied = parallel(jobs);
let apply_ok = true;
let apply_notes = [];
for r in applied {
    if r == () || !r.success {
        apply_ok = false;
        apply_notes.push("shard failed or empty");
    } else if r.output == () {
        apply_ok = false;
        apply_notes.push("shard returned empty output");
    } else if r.output.ok != true {
        apply_ok = false;
        if r.output.errors != () {
            apply_notes.push(json_encode(r.output.errors));
        } else {
            apply_notes.push("shard ok=false");
        }
    }
}
log("apply_ok=" + apply_ok.to_string());

phase("Verify");
let verified = agent(verify_prompt(), #{
    label: "verify",
    capability_mode: "all",
    output_schema: verify_schema,
});

let verify_ok = false;
let verify_json = "{}";
if verified != () && verified.success && verified.output != () {
    verify_json = json_encode(verified.output);
    if verified.output.ok == true {
        verify_ok = true;
    }
}

if !verify_ok {
    phase("Fix");
    log("verification failed; launching fixer");
    let fixed = agent(fix_prompt(verify_json), #{
        label: "fix",
        capability_mode: "all",
        output_schema: apply_schema,
    });
    if fixed == () || !fixed.success {
        apply_ok = false;
        apply_notes.push("fixer failed");
    }
    let verified2 = agent(verify_prompt(), #{
        label: "verify-2",
        capability_mode: "all",
        output_schema: verify_schema,
    });
    if verified2 != () && verified2.success && verified2.output != () && verified2.output.ok == true {
        verify_ok = true;
        verify_json = json_encode(verified2.output);
    } else if verified2 != () && verified2.success && verified2.output != () {
        verify_json = json_encode(verified2.output);
    }
}

let summary = "apply_ok=" + apply_ok.to_string() + " verify_ok=" + verify_ok.to_string();
let report = "# drop-page-reexports\n\n";
report += summary;
report += "\n\napply_notes: ";
report += json_encode(apply_notes);
report += "\n\nverify: ";
report += verify_json;
report += "\n";
let report_path = write_scratch_file("report.md", report);
complete(#{
    summary: summary,
    apply_ok: apply_ok,
    verify_ok: verify_ok,
    path: report_path,
});
