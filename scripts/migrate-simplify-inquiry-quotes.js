// 停止业务写入后执行：mongosh '<目标业务库 URI>' --file scripts/migrate-simplify-inquiry-quotes.js
// 默认预览；INQUIRY_QUOTE_MIGRATION=apply 执行。运行前须备份目标库并核对存量供应商条款。
// 回滚：停止写入，恢复迁移前数据库备份与旧版前后端；新业务须先导出核对。
// 本脚本另保留受影响文档原件供核对，不修改正式采购付款条件、报价余额或审计记录。
const apply = process.env.INQUIRY_QUOTE_MIGRATION === "apply"
const fields = ["minimum_order_quantity", "valid_from", "payment_term_code", "allow_cumulative_batches"]
const targets = [
    ["sales_inquiry_internal_quotes", "terms"],
    ["purchase_order_submission_lines", "supplier_offering_source.inquiry_quote"],
    ["purchase_order_revision_lines", "supplier_offering_source.inquiry_quote"],
    ["purchase_change_submission_lines", "supplier_offering_source.inquiry_quote"],
]
const backup = db.getCollection("migration_20261022_inquiry_quote_originals")
const results = []
for (const [name, prefix] of targets) {
    const paths = fields.map((field) => `${prefix}.${field}`)
    const query = { $or: paths.map((path) => ({ [path]: { $exists: true } })) }
    const collection = db.getCollection(name)
    const matched = collection.countDocuments(query, { maxTimeMS: 60000 })
    let modified = 0
    if (apply) {
        const cursor = collection.find(query).batchSize(100)
        while (cursor.hasNext()) {
            const document = cursor.next()
            backup.updateOne(
                { _id: `${name}:${document._id.toString()}` },
                { $setOnInsert: { collection: name, document, backed_up_at: new Date() } },
                { upsert: true },
            )
            modified += collection.updateOne(
                { _id: document._id },
                { $unset: Object.fromEntries(paths.map((path) => [path, ""])) },
            ).modifiedCount
        }
    }
    results.push({ collection: name, matched, modified })
}
printjson({ migration: "20261022-simplify-inquiry-quotes", mode: apply ? "apply" : "preview", results })
