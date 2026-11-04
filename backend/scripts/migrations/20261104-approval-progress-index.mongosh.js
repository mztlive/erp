// 发起人未读进展索引。默认只检查；APPLY_APPROVAL_PROGRESS_INDEX=1 时创建。
// 不回填历史未读，不修改实例版本。回滚保留字段和索引，见审批进展提醒合同。
(() => {
    const collection = db.getCollection("approval_process_instances");
    const name = "idx_approval_process_instances_started_unread";
    const key = { started_by: 1, initiator_unread: 1, deleted_at: 1 };
    const existing = collection.getIndexes().find(index => index.name === name);
    if (existing && JSON.stringify(existing.key) !== JSON.stringify(key)) {
        throw new Error(`索引 ${name} 的键与合同不符，停止迁移`);
    }
    if (process.env.APPLY_APPROVAL_PROGRESS_INDEX === "1" && !existing) {
        collection.createIndex(key, { name });
        printjson({ status: "created", name, key });
        return;
    }
    printjson({ status: existing ? "ready" : "pending", name, key, historical_backfill: false });
})();
