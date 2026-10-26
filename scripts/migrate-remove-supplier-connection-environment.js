// 在停止 API、后台任务及供应商推送后执行；先完成数据库与受控配置备份。
// MIGRATION_MODE=check|apply|rollback mongosh '<目标业务库 URI>' --file scripts/migrate-remove-supplier-connection-environment.js
// 默认只检查。迁移备份仅保存原环境，不保存密钥；禁止删除备份集合。
// 升级及回滚后均须重新绑定蛋糕叔叔技术引用，执行健康检查后恢复业务。

const mode = process.env.MIGRATION_MODE || "check"
if (!["check", "apply", "rollback"].includes(mode)) {
    throw new Error("MIGRATION_MODE 必须为 check、apply 或 rollback")
}

const options = { maxTimeMS: 10000 }
const connections = db.getCollection("supplier_api_connections")
const backups = db.getCollection("migration_supplier_connection_environment_v1")
const oldIndex = "idx_supplier_api_connections_environment_status_health"
const newIndex = "idx_supplier_api_connections_status_health"
const oldKeys = { environment: 1, status: 1, last_health_result: 1, updated_at: -1 }
const newKeys = { status: 1, last_health_result: 1, updated_at: -1 }
const withEnvironment = { environment: { $exists: true } }

// 只遍历连接元数据，按游标分批读取；每条备份先成功保存，再移除业务字段。
if (mode === "apply") {
    const cursor = connections.find(withEnvironment, { _id: 1, environment: 1 }).batchSize(100)
    while (cursor.hasNext()) {
        const connection = cursor.next()
        backups.updateOne(
            { _id: connection._id },
            { $setOnInsert: { environment: connection.environment } },
            { ...options, upsert: true },
        )
        connections.updateOne(
            { _id: connection._id },
            { $unset: { environment: "" } },
            options,
        )
    }
    connections.createIndex(newKeys, { name: newIndex })
    if (connections.getIndexes().some((index) => index.name === oldIndex)) {
        connections.dropIndex(oldIndex)
    }
}

if (mode === "rollback") {
    // 恢复前验证全部连接，缺少旧环境时拒绝猜测，避免只恢复一部分后才发现缺口。
    const pending = connections.find({}, { _id: 1 }).batchSize(100)
    while (pending.hasNext()) {
        const connection = pending.next()
        const backup = backups.findOne({ _id: connection._id }, { environment: 1 })
        if (!backup || !["production", "testing"].includes(backup.environment)) {
            throw new Error("存在缺少有效环境备份的连接；请明确其旧版本环境并补齐迁移备份后重试")
        }
    }
    const restoring = connections.find({}, { _id: 1 }).batchSize(100)
    while (restoring.hasNext()) {
        const connection = restoring.next()
        const backup = backups.findOne({ _id: connection._id }, { environment: 1 })
        connections.updateOne(
            { _id: connection._id },
            { $set: { environment: backup.environment } },
            options,
        )
    }
    connections.createIndex(oldKeys, { name: oldIndex })
    if (connections.getIndexes().some((index) => index.name === newIndex)) {
        connections.dropIndex(newIndex)
    }
}

printjson({
    migration: "20261026-remove-supplier-connection-environment",
    mode,
    connections: connections.countDocuments({}, options),
    withEnvironment: connections.countDocuments(withEnvironment, options),
    backedUp: backups.countDocuments({}, options),
})
