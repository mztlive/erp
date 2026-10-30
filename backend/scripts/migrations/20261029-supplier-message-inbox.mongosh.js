// 默认只读核对：mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-supplier-message-inbox.mongosh.js
// 显式应用：ERP_MIGRATION_APPLY=1 mongosh "$ERP_MONGO_URI" --file scripts/migrations/20261029-supplier-message-inbox.mongosh.js
// 回退须先停止新增消息领取，保留未知删除调查；本脚本不删除集合、密文证据、消费绑定或索引。
// 消息写入后只能回退到可读取新状态的二进制，禁止恢复为回调接收记录。
const supplierMessageIndexes = [
    ["supplier_message_consumers", [
        [{id: 1}, "uk_supplier_message_consumers_id", true],
        [{account_scope: 1}, "uk_supplier_message_consumers_account", true],
    ]],
    ["supplier_pulled_messages", [
        [{id: 1}, "uk_supplier_pulled_messages_id", true],
        [{account_scope: 1, source_message_id: 1}, "uk_supplier_pulled_messages_source", true],
        [{account_scope: 1, connection_id: 1, state: 1, next_attempt_at: 1, received_at: 1, id: 1},
            "ix_supplier_pulled_messages_pending", false],
    ]],
];
for (const [collectionName, indexes] of supplierMessageIndexes) {
    const collection = db.getCollection(collectionName);
    for (const [keys, name, unique] of indexes) {
        if (unique) {
            const identity = Object.fromEntries(Object.keys(keys).map((key) => [key, `$${key}`]));
            const duplicates = collection.aggregate([
                {$group: {_id: identity, count: {$sum: 1}}},
                {$match: {count: {$gt: 1}}},
                {$limit: 1},
            ], {allowDiskUse: true}).toArray();
            if (duplicates.length) {
                throw new Error(`${collectionName} ${name} 存在重复身份，禁止创建索引`);
            }
        }
        if (process.env.ERP_MIGRATION_APPLY === "1") {
            collection.createIndex(keys, {name, unique});
        }
    }
    print(`${collectionName}: 身份核对完成；${process.env.ERP_MIGRATION_APPLY === "1" ? "索引已应用" : "仅只读核对"}`);
}
