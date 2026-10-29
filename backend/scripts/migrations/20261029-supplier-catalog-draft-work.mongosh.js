// 执行前切换至目标 ERP 数据库；仅创建保存与核对任务索引，不修改业务数据。
// 发布顺序：创建索引，再发布含草稿任务处理器的后端及对应前端。
// 回滚条件：停止接收新保存与核对任务，等待 RUNNING 完成，并处置全部 FAILED。
// 未完成任务不得交给旧版处理器；保留 work 字段及请求记录，禁止删除检查点。
db.getCollection("supplier_catalog_adoption_work_requests").createIndex(
    { id: 1 }, { name: "uk_catalog_adoption_work_request", unique: true },
);
db.getCollection("supplier_catalog_adoption_batches").createIndex(
    { "work.status": 1, id: 1 }, { name: "idx_catalog_adoption_work" },
);
