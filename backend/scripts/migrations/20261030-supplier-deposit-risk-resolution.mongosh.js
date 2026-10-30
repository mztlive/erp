// 预存款风险结案升级：默认只读核对，显式应用仅建立非唯一查询索引。
// ERP_DEPOSIT_RESOLUTION_ACCOUNT_ID=<已确认账户> mongosh "$ERP_MONGO_URI" --file <本文件>
// 批准应用索引时另设置 ERP_DEPOSIT_RESOLUTION_APPLY=1；禁止在未确认数据库上执行。
// 新增 resolution、review_resolution、release_receipts.*.verification、last_reconciliation_risk_at 均可缺省，不回填历史结案或猜测风险时刻。
// 历史受限账户首次办理核对时，由领域命令在同一事务保留办理前updated_at作为保守风险水位；本脚本不改写该字段。
// 回滚只允许回退读取兼容版本；保留核验附件、来源证据、原差异及释放回执。
// 可在确认无新版本查询后移除本文件新增的三个非唯一索引；不得撤销余额/核销或删除结案字段来模拟回滚。

const resolutionAccount = process.env.ERP_DEPOSIT_RESOLUTION_ACCOUNT_ID?.trim();
if (!resolutionAccount || resolutionAccount.length > 256) {
    throw new Error("必须明确提供 ERP_DEPOSIT_RESOLUTION_ACCOUNT_ID，禁止默认扫描全账户");
}
const resolutionApply = process.env.ERP_DEPOSIT_RESOLUTION_APPLY === "1";
const resolutionCollections = [
    ["supplier_deposit_reservations", "idx_deposit_outstanding", {account_id: 1, created_at: 1, id: 1}],
    ["supplier_deposit_reconciliations", "idx_deposit_reconciliation_open", {account_id: 1, resolved_by: 1, covered_from: 1, id: 1}],
    ["supplier_funds_scan_checkpoints", "idx_funds_scan_coverage", {account_id: 1, "window.since": 1, "window.through": 1}],
];
for (const [collectionName, indexName, indexKeys] of resolutionCollections) {
    const collection = db.getCollection(collectionName);
    const rows = collection.find({account_id: resolutionAccount, deleted_at: 0}, {id: 1})
        .sort({id: 1}).batchSize(100).limit(1001).toArray();
    printjson({collection: collectionName, sampled: Math.min(rows.length, 1000), complete: rows.length <= 1000,
        existing_index: collection.getIndexes().some((index) => index.name === indexName), apply: resolutionApply});
    if (resolutionApply) collection.createIndex(indexKeys, {name: indexName});
}
