// 供应商退款意图、结算检查点及来源收件的新增索引。默认只读。
// 应用：ERP_MIGRATION_APPLY=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261031-supplier-after-sales.mongosh.js
// 回滚：先停止售后 worker；只允许对三个全空集合移除本次索引。
// 任一动作或证据已保存时必须保留集合、身份索引及兼容原动作恢复的二进制；禁止删除未知退款。
const afterSalesIndexes = [
  ["supplier_api_refunds", [
    [{id: 1}, "uniq_api_refund_id", true],
    [{connection_id: 1, "receipt.value.external_refund_no": 1}, "uniq_api_refund_receipt", true, {"receipt.value.external_refund_no": {$type: "string"}}],
    [{next_run_at: 1, id: 1}, "idx_api_refund_due", false],
    [{"request.order.merchant_order_no": 1}, "idx_api_refund_order", false],
  ]],
  ["supplier_api_settlement_scans", [[{id: 1}, "uniq_api_settlement_scan_id", true]]],
  ["supplier_api_settlement_evidence", [
    [{id: 1}, "uniq_api_settlement_evidence_id", true],
    [{connection_id: 1, "evidence.value.external_entry_id": 1}, "uniq_api_settlement_external", true],
    [{connection_id: 1, source_evidence_id: 1, id: 1}, "idx_api_settlement_pending", false],
  ]],
];
const apply = process.env.ERP_MIGRATION_APPLY === "1";
const rollback = process.env.ERP_INDEX_ROLLBACK === "1";
const existingCollections = new Set(db.getCollectionNames());
const containsEvidence = afterSalesIndexes.some(([name]) => existingCollections.has(name) && db.getCollection(name).findOne({}, {_id: 1}) !== null);
printjson({mode: apply ? (rollback ? "apply_rollback" : "apply") : "read_only", containsEvidence});
if (apply && rollback && containsEvidence) throw new Error("已有售后动作或来源证据，禁止移除索引或以删除数据回滚");
for (const [name, indexes] of afterSalesIndexes) {
  if (!apply) continue;
  const collection = db.getCollection(name);
  if (rollback) {
    if (!existingCollections.has(name)) continue;
    const existing = new Set(collection.getIndexes().map(index => index.name));
    for (const [, indexName] of indexes) if (existing.has(indexName)) collection.dropIndex(indexName);
  } else {
    for (const [keys, indexName, unique, partial] of indexes) {
      const options = {name: indexName, unique};
      if (partial) options.partialFilterExpression = partial;
      collection.createIndex(keys, options);
    }
  }
}
