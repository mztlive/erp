// 在目标 ERP 数据库登记目录成功检查点查询索引；不回填或删除历史数据。
// 应用：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-supplier-catalog-checkpoints.mongosh.js
// 回退：先回退应用，再以 ERP_INDEX_ROLLBACK=1 执行本脚本。保留新增字段及全部检查点。
const catalogRuns = db.getCollection("supplier_catalog_runs");
const catalogRunIndexes = [
    [{connection_id:1, technical_config_version:1, mapping_version:1, checkpoint_version:1, succeeded:1,
      finished:1, deleted_at:1, scan_through:-1, id:-1}, "idx_supplier_catalog_success"],
    [{connection_id:1, technical_config_version:1, mapping_version:1, checkpoint_version:1, succeeded:1,
      finished:1, deleted_at:1, sync_mode:1, skipped_count:1, scan_through:-1, id:-1},
      "idx_supplier_catalog_full_success"],
];
for (const [keys, name] of catalogRunIndexes) {
    if (process.env.ERP_INDEX_ROLLBACK === "1") {
        if (catalogRuns.getIndexes().some((index) => index.name === name)) catalogRuns.dropIndex(name);
    } else {
        catalogRuns.createIndex(keys, {name, unique:false});
    }
}
