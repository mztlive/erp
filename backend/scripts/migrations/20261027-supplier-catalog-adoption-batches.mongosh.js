// 在目标 ERP 数据库登记批次和逐项执行索引；不得删除草稿、冻结载荷或成功回执。
// 应用：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261027-supplier-catalog-adoption-batches.mongosh.js
// 回退：ERP_INDEX_ROLLBACK=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261027-supplier-catalog-adoption-batches.mongosh.js
// 回退前须停止接收新批次并停止批次后台执行器，核实所有处理中及待核实操作，再回退应用。
// 回退只撤销本脚本索引，两个集合及永久回执、商品、供给、来源关系和编号计数器必须保留。
const adoptionBatchIndexes = [
    ["supplier_catalog_adoption_batches", [
        [{id: 1}, "uk_catalog_adoption_batch_id", true],
        [{batch_no: 1}, "uk_catalog_adoption_batch_no", true],
        [{connection_id: 1, selection_complete: 1, id: -1}, "idx_catalog_adoption_batch_connection", false],
        [{connection_id: 1, actor_id: 1, id: -1}, "idx_catalog_adoption_batch_actor", false],
        [{execution_pending: 1, selection_complete: 1, submitted: 1, id: 1}, "idx_catalog_adoption_batch_runnable", false],
    ]],
    ["supplier_catalog_items", [
        [{connection_id: 1, deleted_at: 1, id: 1}, "idx_supplier_catalog_snapshot", false],
    ]],
    ["supplier_catalog_adoption_batch_items", [
        [{id: 1}, "uk_catalog_adoption_batch_item_id", true],
        [{batch_id: 1, "source.id": 1}, "uk_catalog_adoption_batch_source", true],
        [{batch_id: 1, id: 1}, "idx_catalog_adoption_batch_item_page", false],
        [{batch_id: 1, status: 1, id: 1}, "idx_catalog_adoption_batch_item_due", false],
        [{batch_id: 1, status: 1, processing_started_at: 1}, "idx_catalog_adoption_batch_item_stale", false],
    ]],
];
for (const [collectionName, indexes] of adoptionBatchIndexes) {
    const collection = db.getCollection(collectionName);
    if (process.env.ERP_INDEX_ROLLBACK === "1") {
        if (!db.getCollectionNames().includes(collectionName)) continue;
        const existing = new Set(collection.getIndexes().map((index) => index.name));
        for (const [, name] of indexes) {
            if (existing.has(name)) collection.dropIndex(name);
        }
    } else {
        for (const [keys, name, unique] of indexes) {
            collection.createIndex(keys, {name, unique});
        }
    }
}
