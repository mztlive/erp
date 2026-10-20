// 在目标 ERP 数据库执行；只登记时间线查询索引，不回填或修改业务记录。
// 应用：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261020-sales-timeline-indexes.mongosh.js
// 回退：ERP_INDEX_ROLLBACK=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261020-sales-timeline-indexes.mongosh.js
// 回退前先回退前端/API 与索引登记代码，防止下次启动重新建立。
const salesTimelineIndexes = [
    ["audit_logs", {resource_type: 1, resource_id: 1, deleted_at: 1, created_at: -1, id: -1}, "idx_audit_logs_resource_timeline"],
    ["workflow_actions", {document_id: 1, deleted_at: 1, created_at: -1, id: -1}, "idx_workflow_actions_timeline"],
    ["purchase_payment_requests", {"lines.purchase_order_id": 1, deleted_at: 1}, "idx_purchase_payment_requests_purchase"],
    ["purchase_payment_requests", {"data.lines.payable_account_id": 1, deleted_at: 1}, "idx_purchase_payment_requests_legacy_account"],
    ["purchase_payment_request_snapshots", {"request.lines.purchase_order_id": 1, deleted_at: 1}, "idx_purchase_payment_request_snapshots_purchase"],
    ["customer_refunds", {original_receivable_entry_id: 1, deleted_at: 1}, "idx_customer_refunds_entry"],
    ["customer_refunds", {sales_return_case_id: 1, deleted_at: 1}, "idx_customer_refunds_return"],
    ["supplier_refunds", {original_payable_entry_id: 1, deleted_at: 1}, "idx_supplier_refunds_entry"],
    ["supplier_refunds", {purchase_return_order_id: 1, deleted_at: 1}, "idx_supplier_refunds_return"],
];
for (const [collectionName, keys, name] of salesTimelineIndexes) {
    const collection = db.getCollection(collectionName);
    if (process.env.ERP_INDEX_ROLLBACK === "1") {
        if (collection.getIndexes().some((index) => index.name === name)) {
            collection.dropIndex(name);
        }
    } else {
        collection.createIndex(keys, {name, unique: false});
    }
}
