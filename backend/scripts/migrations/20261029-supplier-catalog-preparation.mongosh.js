// 执行前切换至目标 ERP 数据库；本脚本仅增量创建索引，不更改业务数据。
// 回滚：先停止新版 worker，取消或等待全部 QUEUED/PREPARING 准备任务结束后再回退程序。
// 成员清单与 preparation 字段保留，禁止回滚时删除未完成任务的数据。
const batches = db.getCollection("supplier_catalog_adoption_batches");
batches.createIndex({ "preparation.status": 1, id: 1 }, { name: "idx_catalog_adoption_preparation" });
batches.createIndex({ "preparation.request_key": 1 }, {
    name: "uk_catalog_adoption_preparation_request", unique: true,
    partialFilterExpression: { "preparation.request_key": { $type: "string" } },
});
batches.createIndex({ connection_id: 1, id: -1 }, { name: "idx_catalog_adoption_batch_recent" });
db.getCollection("supplier_catalog_adoption_selections").createIndex(
    { batch_id: 1, page: 1 }, { name: "uk_catalog_adoption_selection_page", unique: true },
);
