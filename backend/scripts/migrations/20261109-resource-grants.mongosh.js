// 执行目标：当前 ERP 数据库。仅创建新增身份授权集合及索引。
// 执行前确认应用尚未启用指定记录授权；索引冲突必须停止，不得删除来源记录。
// 回滚：恢复旧应用并停止新授权写入，保留新增集合、终止记录、事件与索引。
// 旧无类型事件不能推断为开票审批通过；发现旧记录时停止，须从正式业务证据重建。
if (db.getCollection("resource_grant_events").findOne({ "evidence.payload.type": { $exists: false } })) {
    throw new Error("存在无类型授权事件，禁止自动转换；请核验正式业务证据后重建事件");
}
// 新增 typed_source 索引；旧 source 索引保留，禁止为兼容而删除历史数据。
db.getCollection("resource_grants").createIndex({"id": 1}, {name: 'uk_resource_grants_id', unique: true});
db.getCollection("resource_grants").createIndex({"source_id": 1, "resource": 1, "resource_id": 1, "action": 1}, {name: 'uk_resource_grants_source_target', unique: true});
db.getCollection("resource_grants").createIndex({"recipient": 1, "resource": 1, "active": 1, "resource_id": 1}, {name: 'idx_resource_grants_recipient'});
db.getCollection("resource_grants").createIndex({"source_id": 1, "source_version": 1, "resource": 1, "active": 1}, {name: 'idx_resource_grants_source_version'});
db.getCollection("resource_grant_sources").createIndex({"id": 1}, {name: 'uk_resource_grant_sources_id', unique: true});
db.getCollection("resource_grant_sources").createIndex({"key.rule": 1, "key.business_type": 1, "key.business_id": 1, "key.recipient": 1}, {name: 'uk_resource_grant_sources_key', unique: true});
db.getCollection("resource_grant_sources").createIndex({"key.recipient": 1, "deleted_at": 1, "id": 1}, {name: 'idx_resource_grant_sources_recipient'});
db.getCollection("resource_grant_events").createIndex({"id": 1}, {name: 'uk_resource_grant_events_id', unique: true});
db.getCollection("resource_grant_events").createIndex({"status": 1, "available_at": 1, "lease_until": 1, "id": 1}, {name: 'idx_resource_grant_events_claim'});
db.getCollection("resource_grant_events").createIndex({"source.business_type": 1, "source.business_id": 1, "evidence.fact_version": 1}, {name: 'idx_resource_grant_events_typed_source'});
db.getCollection("resource_grant_audits").createIndex({"id": 1}, {name: 'uk_resource_grant_audits_id', unique: true});
db.getCollection("resource_grant_audits").createIndex({"source_id": 1, "snapshot.version": 1}, {name: 'idx_resource_grant_audits_source'});

// 按目标读取与来源主键翻页索引；保留旧索引以支持回滚。
db.getCollection("resource_grants").createIndex({recipient: 1, resource: 1, resource_id: 1, action: 1, active: 1, deleted_at: 1}, {name: "idx_resource_grants_target_read"});
db.getCollection("resource_grants").createIndex({source_id: 1, deleted_at: 1, id: 1}, {name: "idx_resource_grants_source_page"});
// 修订号缺失等于初始版本 0；所有写入方切换为事务内递增后才能启用版本校验。
// 不回填、不清零已有修订号；禁止旧发布程序继续绕过修订号写入。
// MongoDB 内建 _id 唯一索引即为账号资源修订号的读写索引。
if (!db.getCollectionNames().includes("resource_grant_revisions")) {
    db.createCollection("resource_grant_revisions");
}
