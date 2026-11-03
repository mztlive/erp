// Read-only prerequisite for uk_sales_change_orders_active_per_order_base_v2.
// Required: APPROVAL_AUDIT_DB and SALES_CHANGE_AUDIT_ORDER_IDS (JSON string array,
// 1..100 sales-order IDs). Run every reviewed batch before building the index.
// Include soft-deleted rows: the unique partial index also includes them.
(function auditActiveSalesChanges() {
    "use strict";
    const databaseName = process.env.APPROVAL_AUDIT_DB;
    if (!databaseName || !/^[A-Za-z0-9_-]+$/.test(databaseName) || ["admin", "config", "local"].includes(databaseName)) {
        throw new Error("Explicit business APPROVAL_AUDIT_DB required");
    }
    const ids = JSON.parse(process.env.SALES_CHANGE_AUDIT_ORDER_IDS ?? "null");
    if (!Array.isArray(ids) || ids.length < 1 || ids.length > 100 ||
        ids.some(id => typeof id !== "string" || !id.trim() || id.length > 200) || new Set(ids).size !== ids.length) {
        throw new Error("SALES_CHANGE_AUDIT_ORDER_IDS must contain 1..100 unique nonempty IDs");
    }
    const activeStatuses = ["DRAFT", "IN_APPROVAL", "PENDING_IMPACT_CONFIRMATION", "PENDING_FINANCE_REVIEW", "REJECTED"];
    const collection = db.getSiblingDB(databaseName).getCollection("sales_change_orders");
    let conflicts = 0;
    for (const salesOrderId of ids) {
        const rows = collection.find({ sales_order_id: salesOrderId, status: { $in: activeStatuses } }, {
            _id: 0, id: 1, version: 1, sales_order_id: 1, base_revision_id: 1, status: 1, deleted_at: 1,
        }).hint("idx_sales_change_orders_order_status").limit(501).maxTimeMS(10000).toArray();
        if (rows.length > 500) throw new Error(`Bound exceeded for sales order ${salesOrderId}; inventory incomplete`);
        const byBase = new Map();
        for (const row of rows) {
            const key = row.base_revision_id ?? null;
            if (!byBase.has(key)) byBase.set(key, []);
            byBase.get(key).push({ id: row.id, version: String(row.version), status: row.status, deleted_at: row.deleted_at });
        }
        for (const [baseRevisionId, changes] of byBase) {
            if (changes.length < 2) continue;
            conflicts += 1;
            printjson({ kind: "duplicate_active_base", sales_order_id: salesOrderId, base_revision_id: baseRevisionId, changes });
        }
    }
    printjson({ kind: "summary", database: databaseName, requested_order_count: ids.length, conflicting_bases: conflicts,
        complete_for_requested_orders: true, whole_database_verified: false, read_only: true });
})();
