// 供应商实际收货通知升级核对；本脚本始终只读，不生成收货、验收或远端成功事实。
// 执行：ERP_RECEIPT_CONNECTION_ID=<已确认连接> mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261030-supplier-receipt-confirmation-audit.mongosh.js
// 新字段 receipt_actions / receipt_next_run_at / receipt_blocker 缺失按空动作、未调度和无原因读取，禁止回填供应商已收到。
// 发布顺序：部署兼容读取及领域登记的 receipt_due / receipt_sales 索引，再开放已核验实际能力。
// 旧存量已完成ERP验收/入库不会凭本脚本自动发送通知；在原履约详情按原结果查询唤醒后，由worker重读完整净有效证据。
// 回滚：只允许使用可读取新动作字段且继续调查原目标的兼容版本；已写receipt_actions和原供应商证据不得删除。
// 当前无独立的“仅暂停新增收货通知”开关；不得通过移除writer或receipt_confirmation端口暂停，因为这也会停止原未知结果查询。
// 若回滚要求暂停新增通知，须先交付并验证仅查询原动作的兼容worker；未具备时不得回退到不支持本流程的版本。

const receiptConnection = process.env.ERP_RECEIPT_CONNECTION_ID?.trim();
if (!receiptConnection || receiptConnection.length > 256) {
    throw new Error("必须提供已核实的 ERP_RECEIPT_CONNECTION_ID，禁止默认全库核对");
}
if (process.env.ERP_RECEIPT_AUDIT_APPLY === "1") {
    throw new Error("此核对入口只读，禁止回填ERP实收或供应商通知结果");
}

const receiptReport = { inspected: 0, legacy: 0, unknown: 0, blocked: 0, invalid: 0, complete: true, samples: [] };
const receiptRows = db.getCollection("supplier_api_order_executions")
    .find({ connection_id: receiptConnection, deleted_at: 0 }, {
        id: 1, receipt_actions: 1, receipt_next_run_at: 1, receipt_blocker: 1,
    }).sort({ id: 1 }).batchSize(100).limit(1001);
for (const row of receiptRows) {
    if (receiptReport.inspected === 1000) { receiptReport.complete = false; break; }
    receiptReport.inspected += 1;
    if (!row.receipt_actions) { receiptReport.legacy += 1; continue; }
    if (row.receipt_blocker) receiptReport.blocked += 1;
    for (const [target, action] of Object.entries(row.receipt_actions)) {
        const invalid = action.order?.merchant_order_no !== row.id
            || action.order?.external_order_no !== target
            || !["SENT_UNKNOWN", "CONFIRMED", "NOT_REQUIRED"].includes(action.state)
            || (action.state === "SENT_UNKNOWN" && (!action.request?.action?.id || !action.request?.evidence_fingerprint));
        if (action.state === "SENT_UNKNOWN") receiptReport.unknown += 1;
        if (invalid) {
            receiptReport.invalid += 1;
            if (receiptReport.samples.length < 20) receiptReport.samples.push({ execution: row.id, target, reason: "ORIGINAL_RECEIPT_ACTION_INVALID" });
        }
    }
}
printjson(receiptReport);
if (!receiptReport.complete || receiptReport.invalid) {
    throw new Error("核对不完整或有原动作异常；须保留原证据并在发布前核实，禁止重建目标重发");
}
