let meta = #{
    name: "remove-mall-phase2",
    description: "Remove mall phase-2 code, revert to phase-1 caliber, clean collections and indexes",
    phases: [
        #{ title: "Remove", detail: "parallel scoped deletions across backend and frontend" },
        #{ title: "Verify", detail: "compile checks and coupling sweep" },
    ],
};

let removal_schema = #{
    "type": "object",
    "required": ["summary", "files_removed", "errors"],
    "properties": #{
        "summary": #{ "type": "string" },
        "files_removed": #{ "type": "array", "items": #{ "type": "string" } },
        "errors": #{ "type": "array", "items": #{ "type": "string" } },
    },
};

let verify_schema = #{
    "type": "object",
    "required": ["passed", "evidence", "notes"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "evidence": #{ "type": "string" },
        "notes": #{ "type": "string" },
    },
};

phase("Remove");

let jobs = [];

jobs.push(#{
    prompt: "In /Users/huangjiajiang/Development/erp/backend, remove pure-mall domains. Use read_file, list_dir, grep before deleting. Delete directories backend/entities/src/mall_order, mall_sync, mall_after_sales, mall_backfill, card_instance, projection, publication if they exist. Delete database files backend/database/src/repository/mall_order.rs, mall_sync, mall_after_sales, mall_backfill, card_instance.rs, projection.rs, publication.rs plus subdirs repository/mall_order, repository/mall_sync, repository/mall_after_sales, repository/mall_backfill, plus extensions/mall_*.rs and indexes/mall_*.rs. Delete services dirs backend/services/src/mall_order, mall_sync, mall_after_sales, mall_backfill, card_instance plus services projection and publication modules if present. Delete web-api routes backend/apps/web-api/src/core/routes/mall_*.rs and handlers backend/apps/web-api/src/core/handler/mall_*.rs plus projection and publication route and handler files. After file deletion, grep for remaining mod declarations and route mounts in entities/src/lib.rs, database repository mod.rs, extensions mod.rs, indexes mod.rs, services lib.rs, web-api routes admin.rs and mod.rs, handler mod.rs, and remove those lines. Do NOT touch sales_order, receivable, catalog voucher, customer, contract, approval, fulfillment, supplier core files in this job. Report files_removed and errors.",
    label: "remove-backend-mall",
    capability_mode: "read-write",
    output_schema: removal_schema,
});

jobs.push(#{
    prompt: "In /Users/huangjiajiang/Development/erp/backend, revert cross-cutting mall coupling to phase-1 caliber. Use grep and read_file to inspect before editing. Tasks: in entities/src/cost/cost_allocation.rs remove mall_consumption_entry_id alternative so only sales_order_id remains; in entities/src/common/source.rs remove SourceType MallSync and HistoryBackfill variants and fix matches; in entities/src/ids.rs remove D23 mall_sync, D29 mall_order, D30 mall_after_sales, D31 mall_backfill sections; in entities/src/source_registry remove mall_sync_stage.rs reference and mall stage logic but keep generic source registry; in database/src/repository/supplier_fulfillment.rs remove mall_order_id and mall_order_item_id aggregation and MallAfterSalesExt references; in database/src/repository/publication.rs, projection.rs, card_instance.rs, receivable.rs, source_registry.rs remove target_mall_id, mall_ack_at, mall_version, origin_sales_order_id mall-only branches; in entities/src/supplier_fulfillment remove mall_order_id and mall_order_item_id fields. Keep supplier core, receivable core, sales_order intact. Report files_removed as edited files and errors.",
    label: "revert-cross-cutting",
    capability_mode: "read-write",
    output_schema: removal_schema,
});

jobs.push(#{
    prompt: "In /Users/huangjiajiang/Development/erp/erp-client, remove mall frontend. Use list_dir, grep, read_file before deleting. Delete directories erp-client/features/mall-consumption-orders, mall-sync, product-publications, execution-projections, history-backfill, card-business-analytics. Delete app routes erp-client/app/(workspace)/commerce whole directory, erp-client/app/(workspace)/governance/mall-sync, governance/history-backfill, and analytics card-business page if mall-only. In lib/workspace-registry.ts remove W22, W23, W25, W17, W30, W28 mall entries and 商城与发布 nav group items; in lib/permissions.generated.ts remove mall and projection publication permission entries if generated; remove collaboration-card imports from features/sales-orders/components/sales-order-detail-finance-panels.tsx and fix dangling links to /commerce and /governance/mall-sync; grep for remaining imports of deleted features and remove them. Do NOT delete sales/orders pages or generic business components. Report files_removed and errors.",
    label: "remove-frontend-mall",
    capability_mode: "read-write",
    output_schema: removal_schema,
});

jobs.push(#{
    prompt: "In /Users/huangjiajiang/Development/erp, clean tests, scripts, docs references for removed mall code. Use grep and read_file before editing. Delete backend/database/tests/mall_after_sales_limit_scope.rs, mall_order_fact_order_query.rs, mall_sync_snapshot_ingest.rs and any mall_backfill or card_instance test files; remove mall inline test references. In backend/services/tests and backend/crates/test-support remove mall seed helpers. In scripts and e2e remove mall seed and mall flow specs references but keep sales voucher flows. Update docs/erp-phase-2.md only if needed with a short deprecation note at top; do NOT rewrite phase-1 docs. Report files_removed and errors.",
    label: "clean-tests-scripts",
    capability_mode: "read-write",
    output_schema: removal_schema,
});

let results = parallel(jobs);

log("Remove phase finished");

phase("Verify");

let vjobs = [];

vjobs.push(#{
    prompt: "In /Users/huangjiajiang/Development/erp, verify no mall coupling remains and backend compiles. Use grep for mall_order, mall_sync, mall_after_sales, mall_backfill, mall_card_instance, MallOrder, MallSync, target_mall_id, mall_ack_at in backend/entities/src, backend/services/src, backend/database/src, backend/apps/web-api/src. Then run cargo check -p erp-entities -p erp-database -p erp-services or cargo check --workspace if those package names differ, and report the exact command output tail. Set passed=true only if grep finds no production references outside docs and cargo check succeeds. Provide evidence as command output and remaining hits.",
    label: "verify-backend",
    capability_mode: "execute",
    output_schema: verify_schema,
});

vjobs.push(#{
    prompt: "In /Users/huangjiajiang/Development/erp/erp-client, verify frontend has no dangling mall imports and typechecks. Use grep for from.*mall-consumption-orders, from.*execution-projections, from.*product-publications, from.*mall-sync, from.*history-backfill, from.*card-business-analytics, /commerce/, /governance/mall-sync, /governance/history-backfill in app and features and lib. Then run npx tsc --noEmit in erp-client and report tail output. Set passed=true only if no dangling imports remain and tsc passes or only pre-existing errors unrelated to mall remain with explanation. Provide evidence.",
    label: "verify-frontend",
    capability_mode: "execute",
    output_schema: verify_schema,
});

let verdicts = parallel(vjobs);

let passed = 0;
let total = 0;
for v in verdicts {
    total += 1;
    if v != () && v.success && v.output.passed == true {
        passed += 1;
    }
}

log(passed.to_string() + "/" + total.to_string() + " verification jobs passed");

complete(#{ summary: passed.to_string() + "/" + total.to_string() + " verification passed", results: results, verdicts: verdicts });
