let meta = #{
    name: "relocate-cross-aggregate-queries",
    description: "把写在错误仓储里的跨聚合查询迁回集合主人：删除死方法、改走已有主仓储 API、搬迁 impl Repository<外域实体>、用例 fact bundle 只准调用主仓储。按文件不相交波次并行改代码，对抗评审后统一门禁。",
    when_to_use: "当采购/销售/任务/商城同步等 Repository 查询了不属于自己的集合，或 impl Repository<外域实体> 写在错误文件里时。只改 backend/database 仓储与直接调用方，不执行本 workflow 以外的分层合同项。",
    phases: [
        #{ title: "Implement", detail: "波次 1 不相交簇并行；波次 2 共享 work_item/receivable 的 orphan impl 串行；波次 3 fact bundle 并行" },
        #{ title: "Review", detail: "每个实现成功的簇一名对抗评审，缺证据视为未通过" },
        #{ title: "Repair", detail: "resume 实现者只修本簇评审 blocker，不跑 cargo" },
        #{ title: "Quality", detail: "唯一编译入口：fmt/check/clippy/test" },
        #{ title: "Report", detail: "写出迁回证据和残留越界" },
    ],
};

const IMPLEMENTATION_SCHEMA = #{
    "type": "object",
    "required": ["cluster", "completed", "skipped_items", "files_changed", "remaining", "summary"],
    "properties": #{
        "cluster": #{ "type": "string" },
        "completed": #{ "type": "boolean" },
        "skipped_items": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "files_changed": #{ "type": "array", "maxItems": 40, "items": #{ "type": "string" } },
        "remaining": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

const REVIEW_SCHEMA = #{
    "type": "object",
    "required": ["cluster", "passed", "issues", "summary"],
    "properties": #{
        "cluster": #{ "type": "string" },
        "passed": #{ "type": "boolean" },
        "issues": #{
            "type": "array",
            "maxItems": 24,
            "items": #{
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

const QUALITY_SCHEMA = #{
    "type": "object",
    "required": ["passed", "commands", "files_changed", "remaining_issues", "summary"],
    "properties": #{
        "passed": #{ "type": "boolean" },
        "commands": #{ "type": "array", "maxItems": 16, "items": #{ "type": "string" } },
        "files_changed": #{ "type": "array", "maxItems": 80, "items": #{ "type": "string" } },
        "remaining_issues": #{ "type": "array", "maxItems": 24, "items": #{ "type": "string" } },
        "summary": #{ "type": "string" },
    },
};

fn is_text(value) {
    type_of(value) == "string" && value != ""
}

fn is_text_array(value) {
    type_of(value) == "array"
}

fn is_true(value) {
    type_of(value) == "bool" && value == true
}

fn integer_or(value, fallback) {
    if type_of(value) == "i64" { value } else { fallback }
}

fn array_contains(items, wanted) {
    let found = false;
    if is_text_array(items) {
        for item in items {
            if item == wanted { found = true; }
        }
    }
    found
}

fn add_unique(items, value) {
    if is_text(value) {
        if !array_contains(items, value) { items.push(value); }
    }
    items
}

fn join_text(items, sep) {
    let out = "";
    let first = true;
    if is_text_array(items) {
        for item in items {
            if is_text(item) {
                if !first { out += sep; }
                out += item;
                first = false;
            }
        }
    }
    out
}

fn agent_id_of(result) {
    let id = "";
    if result != () {
        if is_text(result.agent_id) { id = result.agent_id; }
    }
    id
}

fn remaining_budget() {
    let n = 999;
    let b = budget();
    if b != () { n = integer_or(b.remaining, 999); }
    n
}

fn path_allowed(path, prefixes) {
    let ok = false;
    if is_text(path) {
        if is_text_array(prefixes) {
            for prefix in prefixes {
                if is_text(prefix) {
                    if path == prefix { ok = true; }
                    if path.starts_with(prefix) { ok = true; }
                }
            }
        }
    }
    ok
}

fn filter_allowed_files(files, prefixes) {
    let kept = [];
    let extra = [];
    if is_text_array(files) {
        for path in files {
            if path_allowed(path, prefixes) {
                kept = add_unique(kept, path);
            } else {
                extra = add_unique(extra, path);
            }
        }
    }
    #{ kept: kept, extra: extra }
}

fn no_cargo_rules() {
    let text = "";
    text += "CARGO: Do not run cargo test, cargo check, cargo clippy, cargo test --workspace, or clippy --workspace. ";
    text += "Do not run ./scripts/check-bpm-boundaries.sh or check-service-boundaries.sh. ";
    text += "Do not set CARGO_TARGET_DIR or use a second target directory. ";
    text += "Quality is the only agent allowed to compile or test. ";
    text += "You may run cargo fmt on files you touched, nothing else.\n";
    text
}

fn layering_rules() {
    let text = "";
    text += "LAYERING (mandatory):\n";
    text += "1. A repository file may query only collections owned by its Ext trait, except a named use-case fact bundle may orchestrate multiple collections by CALLING the owning repository accessors (db.sales_orders(), db.skus(), db.party(), ...).\n";
    text += "2. Generic find/list/count of aggregate B must live on B's repository. Never add find_sales_orders_* onto PurchaseOrderRepository or equivalent.\n";
    text += "3. impl Repository<'a, T> for a foreign entity T must live in T's owning domain file. Moving the impl does not change call sites that already use db.<owner_collection>().method().\n";
    text += "4. Querying purchase_orders filtered by sales_order_id is legal. Querying the sales_orders collection from PurchaseOrderRepository is not.\n";
    text += "5. Fact bundles (list_facts, center_facts, coverage, creation_basis, supplier list/detail, customer center, procurement catalog bundle) MAY stay, but must not reimplement owner CRUD. Prefer existing APIs: SalesOrderRepository::find_orders_by_ids, Sku::find_by_ids, FileAsset::find_by_ids.\n";
    text += "6. Delete dead methods with zero production callers. Update tests in the same files.\n";
    text += "7. Do not change HTTP/Service View, RBAC, transactions, or business rules. Repository methods keep executor: &mut dyn Executor. Do not open sessions.\n";
    text += "8. Every new or moved method needs a full doc comment (purpose, params, returns, errors, constraints).\n";
    text += "9. Read ~/.claude/skills/rust-coding-standards/SKILL.md and references/repository-database.md plus references/file-organization.md before editing. Put new owner methods in that domain's query file, not a giant mod.rs dump.\n";
    text += "10. Stay inside the allowed path prefixes. Grep callers across backend/ before deleting a method. Prefer existing APIs over new ones.\n";
    text
}

fn cluster_task(id) {
    let t = "";
    if id == "po-generic" {
        t += "Purchase-order generic foreign queries and fact-bundle internals.\n";
        t += "DELETE (zero production callers; also drop tests that only exist for them): ";
        t += "PurchaseOrderRepository::list_active_offerings_by_sku, find_first_active_warehouse, list_open_work_items_by_execution.\n";
        t += "RETARGET: find_sales_orders_by_ids -> db.sales_orders().find_orders_by_ids (already exists). ";
        t += "find_effective_sales_orders_by_ids -> add find_effective_orders_by_ids on SalesOrder repository if missing, filtering CommercialStatus::Effective and the same deleted_at as find_orders_by_ids; update services/src/purchase_order/creation_basis.rs and any other callers.\n";
        t += "RETARGET: find_payable_account -> Payable repository method keyed by purchase_order source_document_id + PayableSourceType::PurchaseOrder; add on payable.rs only if missing.\n";
        t += "FACT BUNDLES in the same directory (list_facts.rs, center_facts.rs, coverage.rs, creation_basis.rs) may keep composing, but must call owning accessors (sales_orders, supplier, accounts, payable, catalog, inventory). They must not call purchase_order().find_sales_orders_by_ids.\n";
        t += "Grep find_sales_orders_by_ids / find_effective_sales_orders_by_ids / find_payable_account / list_active_offerings_by_sku / find_first_active_warehouse / list_open_work_items_by_execution before and after.\n";
        return t;
    }
    if id == "publication-offerings" {
        t += "publication.rs orphan impls on SupplierOffering, SupplierOfferingRevision, SkuRevision.\n";
        t += "MOVE impl Repository<SupplierOfferingRevision>::{find_publication_offering_revision, list_publication_offering_revisions} and impl Repository<SupplierOffering>::find_publication_supplier_offering into supplier_offering repository files.\n";
        t += "MOVE impl Repository<SkuRevision>::find_publication_sku_revision into catalog sku/revision query files.\n";
        t += "Keep method names or rename to domain-natural names; update publication.rs call sites inside this crate. Do not change Service behavior.\n";
        return t;
    }
    if id == "warehouse-sku" {
        t += "warehouse.rs WarehouseRepository::sku reads the sku collection by string constant.\n";
        t += "RETARGET to db.skus().find_by_id (or Catalog query API). Delete the local SKUS constant usage for this method if unused. Callers of warehouse().sku() must keep compiling: either keep a thin wrapper that delegates, or update in-crate callers to skus().find_by_id. Prefer delegate-then-delete if that wrapper would still be a generic foreign finder — delete and update callers.\n";
        return t;
    }
    if id == "supplier-file-asset" {
        t += "supplier.rs SupplierRepository::qualification_attachment constructs Repository on FILE_ASSETS.\n";
        t += "RETARGET to db.file_assets().find_by_id. Update in-crate callers. Do not add new FileAsset APIs unless find_by_id is insufficient.\n";
        return t;
    }
    if id == "inventory-hydrate" {
        t += "inventory/aggregate.rs InventoryRepository generic foreign loaders: business_document, purchase_receipts_by_ids, warehouses_by_ids, warehouse_revisions_by_ids, skus_by_ids, sku_revisions_by_ids.\n";
        t += "RETARGET each to owning accessors (document_registry/business_documents, fulfillment/purchase_receipts, warehouses, warehouse_revisions, skus, sku_revisions). Use existing find_by_ids / find_by_id. Only edit files under inventory/. Do not add methods on catalog/warehouse/fulfillment in this cluster.\n";
        t += "If InventoryService still needs a single hydrate entry, keep a thin InventoryRepository method that calls those accessors — that is a fact bundle, not a reimplemented find_many on string collection names.\n";
        return t;
    }
    if id == "catalog-file-asset" {
        t += "catalog/product.rs missing_file_asset_ids queries file_assets.\n";
        t += "MOVE the missing-ids fact to FileAsset repository (existing find_by_ids plus a missing-ids helper on file_asset.rs is OK). Catalog product code must call db.file_assets()... not reimplement file-asset existence.\n";
        return t;
    }
    if id == "supplier-fulfillment-mall" {
        t += "supplier_fulfillment.rs impl Repository<MallOrderItem>::list_by_ids and impl Repository<MallAfterSalesRequestLine>::list_by_request_id.\n";
        t += "MOVE these impls into mall_order and mall_after_sales repository files. Call sites using db.mall_order_items()/db.mall_after_sales_request_lines() stay. Remove the impls from supplier_fulfillment.rs.\n";
        return t;
    }
    if id == "so-foreign" {
        t += "sales_order/order.rs impl Repository<ReceivableAccount>::{list_by_sales_order, find_primary_by_sales_order} and impl Repository<WorkItem>::list_open_procurement_by_responsibility.\n";
        t += "MOVE the ReceivableAccount impl into receivable.rs (or receivable query file). MOVE the WorkItem impl into work_item.rs. Call sites already use db.receivable_accounts() / db.work_items(); do not change Service signatures.\n";
        t += "Remove unused sales_order imports after the move. Grep list_by_sales_order / find_primary_by_sales_order / list_open_procurement_by_responsibility.\n";
        return t;
    }
    if id == "mall-sync-foreign" {
        t += "mall_sync/mod.rs orphan impls: WorkItem::list_for_master_mapping_task; ExternalIdentityTarget::{list_for_external_identity_map, list_active_for_external_identity_map}; AuditLog::list_master_mapping_task_history; ReceivableAccount::find_by_source_sales_order_revision; Contract::find_effective_for_settlement_party; VoucherCategoryProfileRevision::find_active_by_sku.\n";
        t += "MOVE each impl to the owning domain file (work_item.rs, source_registry.rs, audit_log.rs, receivable.rs, contract.rs, catalog/voucher.rs). Keep method names so mall_sync call sites that use db.work_items() etc. still compile. Remove the impls from mall_sync/mod.rs.\n";
        return t;
    }
    if id == "workitem-inbound" {
        t += "WorkItem queries defined outside work_item.rs: legacy_import/mod.rs list_legacy_import_confirmations_by_ids; integration_ops.rs list_for_integration_error_task and list_for_reconciliation_difference; integration_ops.rs impl Repository<SupplierRefundFact>::exists_by_inbox_message.\n";
        t += "MOVE WorkItem impls into work_item.rs. MOVE SupplierRefundFact::exists_by_inbox_message into supplier_fulfillment.rs. Call sites keep using db.work_items() / db.supplier_refund_facts().\n";
        return t;
    }
    if id == "customer-center-receivable" {
        t += "customer_center_receivable.rs impl Repository<ReceivableAccount>::customer_center_receivable is a customer-center fact aggregation on the receivable collection.\n";
        t += "MOVE the impl into the receivable module (new receivable/customer_center.rs or receivable.rs). Keep the aggregation pipeline and Decimal128 semantics. customer_center_related.rs is a different cluster — do not edit it here.\n";
        t += "Update DatabaseExt/module exports if the file moves. Call sites that use db.receivable_accounts().customer_center_receivable stay.\n";
        return t;
    }
    if id == "proc-catalog-impls" {
        t += "procurement_responsibility.rs impl Repository<Sku/Product/ProductRevision/ProductCategory/SkuRevision/AccountCore> list_procurement_responsibility_* helpers.\n";
        t += "These support load_procurement_catalog_bundle / rule display facts (PROC-R01). KEEP the fact bundle functions in procurement_responsibility, but they must call catalog/account accessors instead of owning impl Repository<Sku> inside this file.\n";
        t += "MOVE generic list-by-ids of Sku/Product/Category/AccountCore to catalog and account_core query files if those APIs are missing. Delete the orphan impls from procurement_responsibility.rs after retargeting.\n";
        return t;
    }
    if id == "workitem-outbound" {
        t += "work_item.rs impls on foreign entities used to hydrate inbox briefs: AccountCore, AuditLog, PurchaseOrderSubmission, SalesOrderSubmission, SalesChangeSubmission, ReceivableAccount, SalesOrder, SalesOrderRevision, StockAdjustment, StockAdjustmentLine, SupplierSettlementStatement, IntegrationErrorTask, ReconciliationDifference; plus list_work_item_brief_entities_by_ids generic.\n";
        t += "MOVE each impl to the owning domain query file. work_item remaining code may call db.sales_orders().find_work_item_sales_order() etc., or better, call the owner's natural find_by_id / find_by_ids and keep only work-item-specific filters in work_item.rs.\n";
        t += "Prefer owner natural APIs (find_by_id, find_orders_by_ids, find_submissions_by_ids) over keeping find_work_item_* names when the method is a plain by-id fetch. Keep names when the query has work-item-specific filters that belong on that collection.\n";
        t += "Do not leave impl Repository<SalesOrder> inside work_item.rs.\n";
        return t;
    }
    if id == "supplier-party-facts" {
        t += "supplier/list_page.rs and supplier/detail_bundle.rs load Party names/contacts/addresses/tax/banks.\n";
        t += "KEEP as supplier-domain fact bundles. They MUST call PartyExt accessors (db.party(), db.party_contacts(), ...), not reimplement party finders on SupplierRepository. ";
        t += "If a needed Party API is missing, add it under repository/party/, then call it. Do not add list_party_* onto supplier.rs.\n";
        t += "If already compliant, grep evidence and skip with completed=true and remaining=[].\n";
        return t;
    }
    if id == "offering-list" {
        t += "supplier_offering/list_page.rs calls db.catalog() for associated listing filters.\n";
        t += "KEEP as fact bundle. Ensure it calls CatalogExt query APIs rather than embedding catalog find_many documents that belong in catalog/. Add catalog APIs only if missing. If already compliant, skip with evidence.\n";
        return t;
    }
    if id == "supplier-api-impact" {
        t += "supplier_api.rs governance_audit reads audit_logs; active_offering_revisions / active_publication_count / open supplier orders / background jobs use foreign collection name constants.\n";
        t += "RETARGET to owning accessors: db.audit_logs(), supplier_offerings, product_publications, supplier_fulfillment_orders, background_jobs. Connection-impact may stay as one SupplierApiRepository fact method that calls those accessors. Do not add generic finders onto SupplierApiRepository for other aggregates.\n";
        return t;
    }
    if id == "customer-center-related" {
        t += "customer_center_related.rs aggregates contracts and sales_orders for a customer center read model.\n";
        t += "KEEP as a customer-domain fact bundle. It must use ContractExt/SalesOrderExt collection constants or accessors, not impl Repository<SalesOrder> inside the customer module. Do not add find_sales_orders to CustomerAccount repository. If the current aggregation already queries those collections as a named customer-center pipeline, that is allowed — document it and only fix if it exposes generic sales-order CRUD on Repository<CustomerAccount>.\n";
        return t;
    }
    t += "Unknown cluster.";
    t
}

fn all_clusters() {
    [
        #{
            id: "po-generic",
            wave: 1,
            kind: "retarget",
            prefixes: [
                "backend/database/src/repository/purchase_order/",
                "backend/services/src/purchase_order/",
                "backend/database/src/repository/sales_order/order.rs",
                "backend/database/src/repository/payable.rs",
            ],
        },
        #{
            id: "publication-offerings",
            wave: 1,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/publication.rs",
                "backend/database/src/repository/supplier_offering/",
                "backend/database/src/repository/catalog/sku.rs",
            ],
        },
        #{
            id: "warehouse-sku",
            wave: 1,
            kind: "retarget",
            prefixes: ["backend/database/src/repository/warehouse.rs"],
        },
        #{
            id: "supplier-file-asset",
            wave: 1,
            kind: "retarget",
            prefixes: ["backend/database/src/repository/supplier.rs"],
        },
        #{
            id: "inventory-hydrate",
            wave: 1,
            kind: "retarget",
            prefixes: ["backend/database/src/repository/inventory/"],
        },
        #{
            id: "catalog-file-asset",
            wave: 1,
            kind: "retarget",
            prefixes: [
                "backend/database/src/repository/catalog/product.rs",
                "backend/database/src/repository/file_asset.rs",
            ],
        },
        #{
            id: "supplier-fulfillment-mall",
            wave: 1,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/supplier_fulfillment.rs",
                "backend/database/src/repository/mall_order.rs",
                "backend/database/src/repository/mall_after_sales/",
            ],
        },
        #{
            id: "so-foreign",
            wave: 2,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/sales_order/order.rs",
                "backend/database/src/repository/receivable.rs",
                "backend/database/src/repository/work_item.rs",
            ],
        },
        #{
            id: "mall-sync-foreign",
            wave: 2,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/mall_sync/",
                "backend/database/src/repository/work_item.rs",
                "backend/database/src/repository/receivable.rs",
                "backend/database/src/repository/source_registry.rs",
                "backend/database/src/repository/audit_log.rs",
                "backend/database/src/repository/contract.rs",
                "backend/database/src/repository/catalog/voucher.rs",
            ],
        },
        #{
            id: "workitem-inbound",
            wave: 2,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/legacy_import/",
                "backend/database/src/repository/integration_ops.rs",
                "backend/database/src/repository/work_item.rs",
                "backend/database/src/repository/supplier_fulfillment.rs",
            ],
        },
        #{
            id: "customer-center-receivable",
            wave: 2,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/customer_center_receivable.rs",
                "backend/database/src/repository/receivable.rs",
                "backend/database/src/repository/receivable/",
            ],
        },
        #{
            id: "proc-catalog-impls",
            wave: 2,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/procurement_responsibility.rs",
                "backend/database/src/repository/catalog/",
                "backend/database/src/repository/account_core.rs",
            ],
        },
        #{
            id: "workitem-outbound",
            wave: 2,
            kind: "orphan-impl",
            prefixes: [
                "backend/database/src/repository/work_item.rs",
                "backend/database/src/repository/account_core.rs",
                "backend/database/src/repository/audit_log.rs",
                "backend/database/src/repository/access_control.rs",
                "backend/database/src/repository/purchase_order/",
                "backend/database/src/repository/sales_order/",
                "backend/database/src/repository/sales_review.rs",
                "backend/database/src/repository/receivable.rs",
                "backend/database/src/repository/inventory/",
                "backend/database/src/repository/supplier_settlement.rs",
                "backend/database/src/repository/integration_ops.rs",
            ],
        },
        #{
            id: "supplier-party-facts",
            wave: 3,
            kind: "fact-bundle",
            prefixes: [
                "backend/database/src/repository/supplier/list_page.rs",
                "backend/database/src/repository/supplier/detail_bundle.rs",
                "backend/database/src/repository/party/",
            ],
        },
        #{
            id: "offering-list",
            wave: 3,
            kind: "fact-bundle",
            prefixes: [
                "backend/database/src/repository/supplier_offering/list_page.rs",
                "backend/database/src/repository/catalog/",
            ],
        },
        #{
            id: "supplier-api-impact",
            wave: 3,
            kind: "retarget",
            prefixes: ["backend/database/src/repository/supplier_api.rs"],
        },
        #{
            id: "customer-center-related",
            wave: 3,
            kind: "fact-bundle",
            prefixes: ["backend/database/src/repository/customer_center_related.rs"],
        },
    ]
}

fn clusters_for_wave(wave_no, only_id) {
    let selected = [];
    for cluster in all_clusters() {
        let take = cluster.wave == wave_no;
        if is_text(only_id) {
            take = cluster.id == only_id && cluster.wave == wave_no;
        }
        if take { selected.push(cluster); }
    }
    selected
}

fn implement_prompt(cluster) {
    let p = "";
    p += "You are relocating cross-aggregate MongoDB queries in the ERP Rust backend. ";
    p += "Use read_file and grep on the real code before editing. Do not answer from memory.\n";
    p += "Cluster id: " + cluster.id + "\nKind: " + cluster.kind + "\n";
    p += "Allowed write prefixes:\n";
    for prefix in cluster.prefixes {
        p += "- " + prefix + "\n";
    }
    p += cluster_task(cluster.id);
    p += "\n";
    p += layering_rules();
    p += no_cargo_rules();
    p += "If a listed method is already gone, put it in skipped_items and continue the rest. ";
    p += "completed=true only when every still-present item in this cluster is relocated or deleted and grep no longer finds the old foreign query on the wrong type/file. ";
    p += "remaining lists any leftover foreign queries you saw in allowed files that you did not fix.\n";
    p
}

fn review_prompt(cluster, impl_summary) {
    let p = "";
    p += "Adversarially verify this cross-aggregate repository relocation. ";
    p += "Use grep and read_file on the shipped code. Default passed=false unless you independently inspected evidence.\n";
    p += "Cluster: " + cluster.id + "\nKind: " + cluster.kind + "\n";
    p += "Implementer summary (untrusted): " + json_encode(impl_summary) + "\n";
    p += cluster_task(cluster.id);
    p += "\nFail the review if any of these remain:\n";
    p += "- Generic find/list of aggregate B still implemented on repository A.\n";
    p += "- impl Repository<'a, ForeignEntity> still in the wrong domain file.\n";
    p += "- Callers still going through the old wrong accessor (e.g. purchase_order().find_sales_orders_by_ids).\n";
    p += "- Behavior change, missing executor param, new transaction, or missing doc comments on moved methods.\n";
    p += "- Edits outside the allowed prefixes.\n";
    p += "Fact bundles that call owning accessors are allowed. passed=true only with concrete grep/read evidence in issues or summary.\n";
    p
}

fn fallback_impl(cluster) {
    #{
        cluster: cluster.id,
        completed: false,
        skipped_items: [],
        files_changed: [],
        remaining: ["implementer returned unusable output"],
        summary: "implementer failed",
        success: false,
        agent_id: "",
        extra_files: [],
    }
}

fn impl_from_agent(cluster, result) {
    let out = fallback_impl(cluster);
    if result != () {
        out.agent_id = agent_id_of(result);
        if result.success && result.output != () {
            let files = [];
            if is_text_array(result.output.files_changed) { files = result.output.files_changed; }
            let filtered = filter_allowed_files(files, cluster.prefixes);
            let remaining = [];
            if is_text_array(result.output.remaining) { remaining = result.output.remaining; }
            let skipped = [];
            if is_text_array(result.output.skipped_items) { skipped = result.output.skipped_items; }
            let summary = "implemented";
            if is_text(result.output.summary) { summary = result.output.summary; }
            let completed = is_true(result.output.completed);
            if filtered.extra.len() > 0 {
                completed = false;
                remaining = add_unique(remaining, "wrote files outside allowlist");
            }
            out = #{
                cluster: cluster.id,
                completed: completed,
                skipped_items: skipped,
                files_changed: filtered.kept,
                remaining: remaining,
                summary: summary,
                success: true,
                agent_id: agent_id_of(result),
                extra_files: filtered.extra,
            };
        }
    }
    out
}

fn review_passed(review_result) {
    let ok = false;
    if review_result != () && review_result.success && review_result.output != () {
        if is_true(review_result.output.passed) {
            let issues = review_result.output.issues;
            let has_blocker = false;
            if type_of(issues) == "array" {
                for issue in issues {
                    if issue != () {
                        if issue.severity == "blocker" { has_blocker = true; }
                        if issue.severity == "error" { has_blocker = true; }
                    }
                }
            }
            if !has_blocker { ok = true; }
        }
    }
    ok
}

fn repair_prompt(cluster, review_result) {
    let p = "";
    p += "Resume and fix only this cluster's review blockers. Do not expand scope. Do not run cargo.\n";
    p += "Cluster: " + cluster.id + "\n";
    p += "Allowed prefixes:\n";
    for prefix in cluster.prefixes { p += "- " + prefix + "\n"; }
    p += "Review output (untrusted, verify in code): " + json_encode(review_result) + "\n";
    p += layering_rules();
    p += no_cargo_rules();
    p
}

fn run_parallel_impl(clusters, impl_schema) {
    let jobs = [];
    for cluster in clusters {
        jobs.push(#{
            prompt: implement_prompt(cluster),
            label: "impl:" + cluster.id,
            capability_mode: "read-write",
            output_schema: impl_schema,
        });
    }
    let results = [];
    if jobs.len() == 0 { return []; }
    if remaining_budget() < jobs.len() {
        log("skip implement panel; remaining budget " + remaining_budget().to_string());
        return [];
    }
    let raw = parallel(jobs);
    let i = 0;
    for cluster in clusters {
        let slot = ();
        if i < raw.len() { slot = raw[i]; }
        results.push(impl_from_agent(cluster, slot));
        i += 1;
    }
    results
}

fn run_serial_impl(clusters, impl_schema) {
    let results = [];
    for cluster in clusters {
        if remaining_budget() < 1 {
            log("skip serial impl " + cluster.id + " — no budget");
            results.push(fallback_impl(cluster));
        } else {
            let raw = agent(implement_prompt(cluster), #{
                label: "impl:" + cluster.id,
                capability_mode: "read-write",
                output_schema: impl_schema,
            });
            results.push(impl_from_agent(cluster, raw));
        }
    }
    results
}

fn run_reviews(clusters, impls, review_schema) {
    let jobs = [];
    let indexed = [];
    let i = 0;
    for cluster in clusters {
        let impl_row = ();
        if i < impls.len() { impl_row = impls[i]; }
        if impl_row != () && is_true(impl_row.success) {
            jobs.push(#{
                prompt: review_prompt(cluster, impl_row),
                label: "review:" + cluster.id,
                capability_mode: "read-only",
                output_schema: review_schema,
            });
            indexed.push(i);
        }
        i += 1;
    }
    let verdicts = [];
    for cluster in clusters { verdicts.push(()); }
    if jobs.len() == 0 { return verdicts; }
    if remaining_budget() < jobs.len() {
        log("skip review panel; remaining budget " + remaining_budget().to_string());
        return verdicts;
    }
    let raw = parallel(jobs);
    let j = 0;
    for impl_index in indexed {
        if j < raw.len() { verdicts[impl_index] = raw[j]; }
        j += 1;
    }
    verdicts
}

fn run_repairs(clusters, impls, reviews, impl_schema) {
    let repaired = [];
    let i = 0;
    for cluster in clusters {
        let impl_row = fallback_impl(cluster);
        if i < impls.len() { impl_row = impls[i]; }
        let review_row = ();
        if i < reviews.len() { review_row = reviews[i]; }
        let need = false;
        if impl_row != () && is_true(impl_row.success) {
            if !review_passed(review_row) { need = true; }
        }
        if need {
            if is_text(impl_row.agent_id) && remaining_budget() >= 1 {
                let review_payload = #{};
                if review_row != () && review_row.output != () { review_payload = review_row.output; }
                let raw = agent(repair_prompt(cluster, review_payload), #{
                    label: "repair:" + cluster.id,
                    capability_mode: "read-write",
                    output_schema: impl_schema,
                    resume_from: impl_row.agent_id,
                });
                repaired.push(impl_from_agent(cluster, raw));
            } else {
                repaired.push(impl_row);
            }
        } else {
            repaired.push(impl_row);
        }
        i += 1;
    }
    repaired
}

fn process_wave(wave_no, serial, only_id, impl_schema, review_schema) {
    let clusters = clusters_for_wave(wave_no, only_id);
    if clusters.len() == 0 {
        log("wave " + wave_no.to_string() + " has no clusters");
        return #{ clusters: [], impls: [], reviews: [] };
    }
    let mode = "parallel";
    if serial { mode = "serial"; }
    log("wave " + wave_no.to_string() + " clusters: " + clusters.len().to_string() + " mode=" + mode);
    phase("Implement");
    let impls = [];
    if serial {
        impls = run_serial_impl(clusters, impl_schema);
    } else {
        impls = run_parallel_impl(clusters, impl_schema);
    }
    phase("Review");
    let reviews = run_reviews(clusters, impls, review_schema);
    phase("Repair");
    impls = run_repairs(clusters, impls, reviews, impl_schema);
    #{ clusters: clusters, impls: impls, reviews: reviews }
}

fn count_completed(impls) {
    let n = 0;
    for row in impls {
        if row != () && is_true(row.completed) { n += 1; }
    }
    n
}

fn collect_files(impls) {
    let files = [];
    for row in impls {
        if row != () && is_text_array(row.files_changed) {
            for path in row.files_changed { files = add_unique(files, path); }
        }
    }
    files
}

fn collect_remaining(impls) {
    let items = [];
    for row in impls {
        if row != () && is_text_array(row.remaining) {
            for item in row.remaining {
                if is_text(item) { items = add_unique(items, row.cluster + ": " + item); }
            }
        }
        if row != () && !is_true(row.completed) {
            items = add_unique(items, row.cluster + " not completed: " + row.summary);
        }
    }
    items
}

fn build_report(wave_results, quality_output) {
    let md = "";
    md += "# Relocate cross-aggregate queries\n\n";
    md += "Fixed work-list in the workflow. Implementers were not allowed to invent extra clusters.\n\n";
    for pack in wave_results {
        if pack != () && is_text_array(pack.clusters) {
            md += "## Wave results\n\n";
            let i = 0;
            for cluster in pack.clusters {
                let impl_row = ();
                if i < pack.impls.len() { impl_row = pack.impls[i]; }
                let status = "failed";
                let summary = "";
                let files = "";
                if impl_row != () {
                    if is_true(impl_row.completed) { status = "completed"; }
                    if is_text(impl_row.summary) { summary = impl_row.summary; }
                    files = join_text(impl_row.files_changed, ", ");
                }
                md += "- `" + cluster.id + "` (" + cluster.kind + "): **" + status + "** — " + summary;
                if files != "" { md += " Files: " + files; }
                md += "\n";
                i += 1;
            }
            md += "\n";
        }
    }
    md += "## Quality\n\n";
    if quality_output != () {
        md += json_encode(quality_output);
        md += "\n";
    } else {
        md += "Quality did not run.\n";
    }
    md
}

let wave_filter = 0;
let only_cluster = "";
if args != () {
    if args.wave != () {
        if type_of(args.wave) == "i64" { wave_filter = args.wave; }
        if args.wave == "1" { wave_filter = 1; }
        if args.wave == "2" { wave_filter = 2; }
        if args.wave == "3" { wave_filter = 3; }
        if args.wave == "all" { wave_filter = 0; }
    }
    if is_text(args.cluster) { only_cluster = args.cluster; }
}
log("relocate-cross-aggregate-queries wave=" + wave_filter.to_string() + " cluster=" + only_cluster);

let packs = [];
if wave_filter == 0 || wave_filter == 1 {
    packs.push(process_wave(1, false, only_cluster, IMPLEMENTATION_SCHEMA, REVIEW_SCHEMA));
}
if wave_filter == 0 || wave_filter == 2 {
    packs.push(process_wave(2, true, only_cluster, IMPLEMENTATION_SCHEMA, REVIEW_SCHEMA));
}
if wave_filter == 0 || wave_filter == 3 {
    packs.push(process_wave(3, false, only_cluster, IMPLEMENTATION_SCHEMA, REVIEW_SCHEMA));
}

let all_impls = [];
for pack in packs {
    if pack != () && is_text_array(pack.impls) {
        for row in pack.impls { all_impls.push(row); }
    }
}

let changed = collect_files(all_impls);
let leftover = collect_remaining(all_impls);

phase("Quality");
let quality_out = ();
if changed.len() == 0 && leftover.len() == 0 && count_completed(all_impls) == 0 {
    log("no files changed; skip cargo");
} else {
    if remaining_budget() >= 1 {
        let q = "";
        q += "You are the only agent allowed to compile. Run these in the repo root and report honestly:\n";
        q += "1. cargo fmt --all\n";
        q += "2. cargo check --workspace\n";
        q += "3. cargo clippy --workspace --all-targets --all-features\n";
        q += "4. cargo test --workspace\n";
        q += "Do not start extra target directories. If a command cannot run, put it in remaining_issues and passed=false.\n";
        q += "Changed files reported by implementers (untrusted): " + json_encode(changed) + "\n";
        q += "Known leftover claims: " + json_encode(leftover) + "\n";
        q += "Grep backend/database/src/repository for remaining PurchaseOrderRepository sales-order finders and impl Repository<SalesOrder> outside sales_order/. Include any hits in remaining_issues.\n";
        let qraw = agent(q, #{
            label: "quality",
            capability_mode: "execute",
            output_schema: QUALITY_SCHEMA,
        });
        if qraw != () && qraw.success { quality_out = qraw.output; }
    } else {
        log("skip quality; no budget");
    }
}

phase("Report");
let report = build_report(packs, quality_out);
let report_path = write_scratch_file("report.md", report);

let quality_passed = false;
if quality_out != () && is_true(quality_out.passed) { quality_passed = true; }
if changed.len() == 0 && leftover.len() == 0 && count_completed(all_impls) > 0 { quality_passed = true; }

complete(#{
    report_path: report_path,
    completed_clusters: count_completed(all_impls),
    files_changed: changed,
    leftover: leftover,
    quality_passed: quality_passed,
    wave: wave_filter,
});
