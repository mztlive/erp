// 默认仅新增索引和参与修订号集合；不生成授权，不修改业务事实。
// 回滚：先停止维护入口、转派生产方及 worker，设置全局 resourceGrantMigrationMode = "rollback" 后加载本文件。
// 保留全部事件、来源、审计和修订号；旧 worker 不得消费 invoice_task_reassigned 事件。
const resourceGrantMaintenanceIndexes = [
  ["approval_process_instances", "idx_approval_instances_grant_backfill", {"subject.subject_kind": 1, status: 1, deleted_at: 1, id: 1}],
  ["approval_node_executions", "idx_approval_executions_grant_backfill", {deleted_at: 1, id: 1}],
  ["work_item_command_receipts", "idx_work_item_receipts_grant_backfill", {"result.kind": 1, deleted_at: 1, id: 1}],
];
if (typeof resourceGrantMigrationMode !== "undefined" && resourceGrantMigrationMode === "rollback") {
  for (const [collection, name] of resourceGrantMaintenanceIndexes) {
    if (db.getCollection(collection).getIndexes().some(index => index.name === name)) db.getCollection(collection).dropIndex(name);
  }
} else {
  if (!db.getCollectionNames().includes("document_participant_revisions")) db.createCollection("document_participant_revisions");
  for (const [collection, name, keys] of resourceGrantMaintenanceIndexes) db.getCollection(collection).createIndex(keys, {name});
}
