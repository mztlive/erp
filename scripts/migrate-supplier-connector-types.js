// 停止 API、后台任务和推送，完成数据库备份后执行；目标连接须先在后台停用。
// 默认 check 只统计未指定类型的连接。
// CONNECTION_TYPES='{"<连接ID>":"dangaoshushu"}' MIGRATION_MODE=apply|rollback mongosh '<业务库 URI>' --file scripts/migrate-supplier-connector-types.js
// 只按显式 ID 分配类型，不根据供应商名称、连接代码、地址或现有配置推断。
const mode = process.env.MIGRATION_MODE || "check"
if (!["check", "apply", "rollback"].includes(mode)) throw new Error("迁移模式无效")
const assignments = JSON.parse(process.env.CONNECTION_TYPES || "{}")
if (!assignments || Array.isArray(assignments) || typeof assignments !== "object") {
    throw new Error("CONNECTION_TYPES 必须为连接 ID 到接口类型的 JSON 对象")
}
const entries = Object.entries(assignments)
if (mode !== "check" && entries.length === 0) throw new Error("必须明确指定目标连接及接口类型")
if (entries.some(([id, kind]) => !id.trim() || kind !== "dangaoshushu")) {
    throw new Error("连接 ID 不能为空，接口类型必须为已实现的 dangaoshushu")
}
const connections = db.getCollection("supplier_api_connections")
const backups = db.getCollection("migration_supplier_connector_types_v1")
const options = { maxTimeMS: 10000 }

// 先验证全部目标，再执行；不修改已声明的接口类型或活动连接。
for (const [id, kind] of entries) {
    const connection = connections.findOne({ id, deleted_at: 0, status: "disabled" })
    if (!connection) {
        throw new Error("目标连接不存在、已删除或未停用，迁移未执行")
    }
    if (connection.connector_type != null && connection.connector_type !== kind) {
        throw new Error("已声明的接口类型不可变更，迁移未执行")
    }
    if (mode === "rollback" && !backups.findOne({ _id: id })) {
        throw new Error("目标连接没有迁移备份，回滚未执行")
    }
}

if (mode !== "check") {
    for (const [id, kind] of entries) {
        const connection = connections.findOne({ id, deleted_at: 0, status: "disabled" })
        if (mode === "apply" && connection.connector_type === kind) continue
        if (mode === "rollback" && connection.connector_type == null) continue
        const update = {
            $set: {
                endpoint_reference_bound: false,
                credential_reference_bound: false,
                last_healthy_technical_config_version: null,
            },
            $inc: { version: 1, technical_config_version: 1 },
        }
        if (mode === "apply") {
            backups.updateOne(
                { _id: id },
                { $setOnInsert: { had_connector_type: Object.hasOwn(connection, "connector_type") } },
                { ...options, upsert: true },
            )
            update.$set.connector_type = kind
        } else {
            const backup = backups.findOne({ _id: id })
            if (backup.had_connector_type) update.$set.connector_type = null
            else update.$unset = { connector_type: "" }
        }
        const result = connections.updateOne(
            { id, version: connection.version, deleted_at: 0, status: "disabled", connector_type: connection.connector_type ?? null },
            update,
            options,
        )
        if (result.matchedCount !== 1) throw new Error("连接已变化，停止迁移；核对后可重复执行")
    }
}
printjson({
    migration: "20261026-supplier-connector-types",
    mode,
    selected: entries.length,
    untyped: connections.countDocuments({ connector_type: null }, options),
})
