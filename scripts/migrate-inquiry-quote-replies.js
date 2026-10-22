// 先停止业务写入并备份目标库。默认只预览，不执行数据库修改。
// mongosh '<目标业务库 URI>' --file scripts/migrate-inquiry-quote-replies.js
// INQUIRY_QUOTE_REPLIES_MIGRATION=apply 可选回填存量默认值；新后端兼容缺失字段，无须强制迁移。
// INQUIRY_QUOTE_REPLIES_MIGRATION=rollback 仅允许不存在无法供货回复时回退字段。
// 回滚前完整报价写入独立备份集合；存在无法供货回复须保留新读取模型或恢复发布前整库备份，不得删除历史。
const mode = process.env.INQUIRY_QUOTE_REPLIES_MIGRATION || "preview"
if (!["preview", "apply", "rollback"].includes(mode)) throw new Error("迁移模式无效")
const quotes = db.getCollection("sales_inquiry_internal_quotes")
const missing = { $or: [{ "terms.freight_gross": { $exists: false } }, { "terms.unavailable": { $exists: false } }] }
const unavailable = quotes.countDocuments({ "terms.unavailable": true }, { maxTimeMS: 60000 })
const matched = quotes.countDocuments(missing, { maxTimeMS: 60000 })
let modified = 0
if (mode === "rollback" && unavailable) throw new Error("存在无法供货回复，禁止直接回滚旧模型；请保留新读取模型或恢复发布前备份")
if (mode !== "preview") {
    const query = mode === "apply" ? missing : { $or: [{ "terms.freight_gross": { $exists: true } }, { "terms.unavailable": { $exists: true } }] }
    const backup = db.getCollection(`migration_20261022_inquiry_quote_replies_${mode}`)
    const cursor = quotes.find(query).batchSize(100)
    while (cursor.hasNext()) {
        const document = cursor.next()
        backup.updateOne({ _id: document._id }, { $setOnInsert: { document, backed_up_at: new Date() } }, { upsert: true })
        if (mode === "apply") {
            const values = {}
            if (!("freight_gross" in document.terms)) values["terms.freight_gross"] = null
            if (!("unavailable" in document.terms)) values["terms.unavailable"] = false
            modified += quotes.updateOne({ _id: document._id }, { $set: values }).modifiedCount
        } else {
            modified += quotes.updateOne({ _id: document._id }, { $unset: { "terms.freight_gross": "", "terms.unavailable": "" } }).modifiedCount
        }
    }
}
printjson({ migration: "20261022-inquiry-quote-replies", mode, missing: matched, unavailable, modified })
