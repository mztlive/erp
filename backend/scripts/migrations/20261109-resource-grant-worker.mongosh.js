// 执行前备份授权集合；仅新增索引，不更新业务事实。回滚时停止 worker 后删除本脚本列出的索引。
const indexes = [
  ["resource_grant_events", "idx_resource_grant_events_recompute", {"source.rule": 1, id: 1}],
  ["resource_grant_events", "idx_resource_grant_events_oldest", {status: 1, deleted_at: 1, "evidence.occurred_at": 1}],
  ["resource_grant_events", "idx_resource_grant_events_recover", {status: 1, lease_until: 1, id: 1}],
  ["resource_grant_events", "idx_resource_grant_events_source_page", {"source.rule": 1, "source.business_type": 1, "source.business_id": 1, "source.recipient": 1, id: 1}],
  ["resource_grant_sources", "idx_resource_grant_sources_recent", {"key.rule": 1, updated_at: -1, id: 1}],
];
for (const [collection, name, keys] of indexes) db.getCollection(collection).createIndex(keys, {name});
