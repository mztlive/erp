// 停止业务写入并备份目标库后执行：mongosh '<目标业务库 URI>' --file scripts/migrate-remove-inquiry-quote-evidence.js
// 默认预览；INQUIRY_QUOTE_EVIDENCE_MIGRATION=apply 执行。报价条款精简迁移须先完成。
// 仅移除报价凭证字段；保留文件实体、费用依据、报价冻结地址、版本及余额。
// 回滚须停止写入并核对新增业务，再恢复迁移前数据库和配套旧版本。
const apply = process.env.INQUIRY_QUOTE_EVIDENCE_MIGRATION === "apply"
const collection = db.getCollection("sales_inquiry_internal_quotes")
const query = { attachment_ids: { $exists: true } }
const matched = collection.countDocuments(query, { maxTimeMS: 60000 })
let modified = 0
if (apply) {
    const backup = db.getCollection("migration_20261022_inquiry_quote_evidence_originals")
    const cursor = collection.find(query).batchSize(100)
    while (cursor.hasNext()) {
        const document = cursor.next()
        backup.updateOne(
            { _id: document._id },
            { $setOnInsert: { document, backed_up_at: new Date() } },
            { upsert: true },
        )
        modified += collection.updateOne(
            { _id: document._id },
            { $unset: { attachment_ids: "" } },
        ).modifiedCount
    }
}
const remaining = collection.countDocuments(query, { maxTimeMS: 60000 })
printjson({ migration: "20261022-remove-inquiry-quote-evidence", mode: apply ? "apply" : "preview", matched, modified, remaining })
if (apply && remaining !== 0) throw new Error("旧报价凭证字段仍有残留")
