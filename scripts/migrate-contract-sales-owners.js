// 合同独立负责销售迁移。默认只读，禁止按客户当前名单或创建人猜测历史责任。
// 1. 准备经业务核对的 JSON 文件：[{"contract_id":"...","expected_version":1,"sales_owner_user_id":"..."}]
// 2. ERP_CONTRACT_OWNER_ASSIGNMENTS=/absolute/assignments.json mongosh '<目标业务库 URI>' --file scripts/migrate-contract-sales-owners.js
// 3. 核对只读报告后，增加 ERP_CONTRACT_OWNER_MIGRATION_APPLY=1 执行；副本集事务原子写入。
// 4. 回滚须使用报告中的 before 原值及迁移后的 expected_version 做 CAS，仅撤销本批字段；不得恢复客户级合同授权。

const fs = require("fs")
const path = process.env.ERP_CONTRACT_OWNER_ASSIGNMENTS
const assignments = path ? JSON.parse(fs.readFileSync(path, "utf8")) : []
const apply = process.env.ERP_CONTRACT_OWNER_MIGRATION_APPLY === "1"
if (!Array.isArray(assignments)) throw new Error("迁移文件必须是数组")
const seen = new Set()
const before = []
for (const item of assignments) {
    if (!item.contract_id || !item.sales_owner_user_id || !Number.isSafeInteger(item.expected_version) || item.expected_version < 1 || seen.has(item.contract_id)) {
        throw new Error("合同身份、负责销售、期望版本必须有效且合同不得重复")
    }
    seen.add(item.contract_id)
    const contract = db.contracts.findOne({ id: item.contract_id, deleted_at: NumberLong(0) })
    const account = db.accounts.findOne({ id: item.sales_owner_user_id, deleted_at: NumberLong(0) })
    if (!contract || !account || contract.version.toString() !== String(item.expected_version)) throw new Error(`合同或人员不存在，或版本已变化：${item.contract_id}`)
    if (contract.sales_owner_user_id) throw new Error(`合同已有独立负责销售，不得覆盖：${item.contract_id}`)
    before.push({ contract_id: item.contract_id, expected_version: item.expected_version + 1, sales_owner_user_id: contract.sales_owner_user_id ?? null })
}
const missing = db.contracts.countDocuments({ deleted_at: NumberLong(0), $or: [{ sales_owner_user_id: { $exists: false } }, { sales_owner_user_id: "" }] })
printjson({ migration: "20261021-contract-sales-owners", mode: apply ? "apply" : "read-only", missing, confirmed: assignments.length, before })
if (apply && assignments.length) {
    const session = db.getMongo().startSession()
    try {
        const target = session.getDatabase(db.getName())
        session.startTransaction({ readConcern: { level: "snapshot" }, writeConcern: { w: "majority" } })
        for (const item of assignments) {
            if (!target.accounts.findOne({ id: item.sales_owner_user_id, deleted_at: NumberLong(0) })) throw new Error("共享目标账号已变化")
            const result = target.contracts.updateOne({ id: item.contract_id, version: NumberLong(String(item.expected_version)), deleted_at: NumberLong(0), $or: [{ sales_owner_user_id: { $exists: false } }, { sales_owner_user_id: "" }] }, {
                $set: { sales_owner_user_id: item.sales_owner_user_id, updated_at: NumberLong(String(Math.floor(Date.now() / 1000))) },
                $inc: { version: NumberLong(1) },
            })
            if (result.matchedCount !== 1) throw new Error(`合同版本冲突：${item.contract_id}`)
        }
        session.commitTransaction()
    } catch (error) {
        try { session.abortTransaction() } catch (_) { /* 保留原始提交或回滚结果，禁止自动重放。 */ }
        throw error
    } finally {
        session.endSession()
    }
}
