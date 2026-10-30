// 供应商资金证据索引迁移。默认只读，不执行资金记账，不删除证据。
// 核对：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-supplier-funds-evidence.mongosh.js
// 应用：ERP_MIGRATION_APPLY=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-supplier-funds-evidence.mongosh.js
// 回退：先停止账号资金调查worker，再设置 ERP_MIGRATION_APPLY=1 ERP_INDEX_ROLLBACK=1。
// 已存在资金证据、检查点或余额观察时禁止移除身份索引或回退不支持原事实的二进制。
// 租约不得配置TTL删除；账户停用不能停止原资金事实调查。

const fundsEvidenceIndexes = [
    ["supplier_funds_evidence_inbox", [
        [{id: 1}, "uk_funds_evidence_id", true],
        [{account_id: 1, "source_key.kind": 1, "source_key.value": 1}, "uk_funds_evidence_source", true],
        [{account_id: 1, state: 1, "input.source_observed_at": 1, id: 1}, "idx_funds_evidence_pending", false],
        [{account_id: 1, "input.result.external_order_id": 1, "input.source_observed_at": 1, id: 1}, "idx_funds_evidence_order", false],
    ]],
    ["supplier_funds_evidence_conflicts", [
        [{id: 1}, "uk_funds_conflict_id", true],
        [{account_id: 1, inbox_id: 1, id: 1}, "idx_funds_conflict_inbox", false],
    ]],
    ["supplier_funds_scan_checkpoints", [
        [{id: 1}, "uk_funds_scan_id", true],
        [{account_id: 1, scan_key: 1}, "uk_funds_scan_key", true],
        [{account_id: 1}, "uk_funds_scan_active_account", true, {finished: false}],
        [{account_id: 1, finished: 1, completed_through: -1, created_at: -1, id: -1}, "idx_funds_scan_latest", false],
    ]],
    ["supplier_funds_balance_observations", [
        [{id: 1}, "uk_funds_balance_id", true],
        [{account_id: 1, "observation.observed_at": -1, id: -1}, "idx_funds_balance_observed", false],
    ]],
    ["supplier_funds_scan_leases", [
        [{id: 1}, "uk_funds_lease_id", true],
        [{account_id: 1}, "uk_funds_lease_account", true],
    ]],
];

const apply = process.env.ERP_MIGRATION_APPLY === "1";
const rollback = process.env.ERP_INDEX_ROLLBACK === "1";
const collections = new Set(db.getCollectionNames());
const containsEvidence = fundsEvidenceIndexes
    .filter(([name]) => name !== "supplier_funds_scan_leases" && collections.has(name))
    .some(([name]) => db.getCollection(name).findOne({}, {_id: 1}) !== null);
printjson({mode: apply ? (rollback ? "apply_rollback" : "apply") : "read_only", containsEvidence});
if (apply && rollback && containsEvidence) {
    throw new Error("资金证据或检查点已存在，须保留身份索引及兼容调查worker，禁止以删证据实现回退");
}
for (const [name, indexes] of fundsEvidenceIndexes) {
    if (!apply) continue;
    const collection = db.getCollection(name);
    if (rollback) {
        if (!collections.has(name)) continue;
        const existing = new Set(collection.getIndexes().map((index) => index.name));
        for (const [, indexName] of indexes) if (existing.has(indexName)) collection.dropIndex(indexName);
    } else {
        for (const [keys, indexName, unique, partial] of indexes) {
            const options = {name: indexName, unique};
            if (partial) options.partialFilterExpression = partial;
            collection.createIndex(keys, options);
        }
    }
}
