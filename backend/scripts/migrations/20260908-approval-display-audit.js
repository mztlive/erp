// mongosh <连接参数> --file scripts/migrations/20260908-approval-display-audit.js
// 可选字段兼容迁移的只读检查。不得以当前单据内容回填历史 display。
const kinds = ["customer_receipt", "customer_refund", "supplier_refund", "receipt_reversal", "payment_reversal", "stock_adjustment"];
const snapshots = db.getCollection("approval_subject_snapshots");
for (const document_type of kinds) {
    const total = snapshots.countDocuments({ document_type });
    const frozen = snapshots.countDocuments({ document_type, display: { $type: "object" } });
    const legacy = snapshots.countDocuments({ document_type, $or: [{ display: { $exists: false } }, { display: null }] });
    printjson({ document_type, total, frozen, legacy, invalid: total - frozen - legacy });
}
// 回滚只切换应用版本；保留新增字段和所有既有审批/任务/审计事实。
