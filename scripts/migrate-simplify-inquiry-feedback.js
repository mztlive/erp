// 执行前停止业务写入并备份目标库，使用与目标库匹配的 mongosh URI。
// 默认预览：mongosh '<目标业务库 URI>' --file scripts/migrate-simplify-inquiry-feedback.js
// 执行：INQUIRY_FEEDBACK_MIGRATION=apply mongosh '<目标业务库 URI>' --file scripts/migrate-simplify-inquiry-feedback.js
// 发现同一商品多条存量报价时终止；须先核对并处理，禁止任取一条或丢弃已转单来源。
// 回滚：保持停写，恢复迁移前数据库备份和配套旧版前后端；发生新业务后须先核对新增记录。
// 只清理对销售反馈及已绑定销售来源的报价条件，不修改需求、内部供应商报价或数量占用台账。
const apply = process.env.INQUIRY_FEEDBACK_MIGRATION === "apply"
const fields = ["quantity", "delivery_due_at", "min_quantity", "max_quantity", "allow_cumulative", "allocations", "included_cost_note"]
const targets = [
    { name: "sales_inquiries", paths: ["feedbacks.data.lines.prices", "feedback_work.data.lines.prices"], roots: ["feedbacks", "feedback_work"] },
    { name: "sales_order_inquiry_sources", paths: ["lines.price"], roots: ["lines"] },
]
const inquiries = db.getCollection("sales_inquiries")
const multiple = inquiries.countDocuments({ $or: [
    { "feedbacks.data.lines.prices.1": { $exists: true } },
    { "feedback_work.data.lines.prices.1": { $exists: true } },
] }, { maxTimeMS: 60000 })
if (multiple) throw new Error(`${multiple} 张询价含多条报价，须先核对处理；本次未修改数据`)
const backup = db.getCollection("migration_20261022_inquiry_feedback_originals")
const results = []
function cleanPrice(price) {
    for (const field of fields) delete price[field]
}
for (const target of targets) {
    const query = { $or: target.paths.flatMap((path) => fields.map((field) => ({ [`${path}.${field}`]: { $exists: true } }))) }
    const collection = db.getCollection(target.name)
    const matched = collection.countDocuments(query, { maxTimeMS: 60000 })
    let modified = 0
    if (apply) {
        const cursor = collection.find(query).batchSize(100)
        while (cursor.hasNext()) {
            const original = cursor.next()
            const document = EJSON.parse(EJSON.stringify(original))
            if (target.name === "sales_inquiries") {
                for (const root of target.roots)
                    for (const entry of document[root] ?? [])
                        for (const line of entry.data.lines)
                            for (const price of line.prices ?? []) cleanPrice(price)
            } else {
                for (const line of document.lines ?? []) cleanPrice(line.price)
            }
            backup.updateOne(
                { _id: `${target.name}:${original._id.toString()}` },
                { $setOnInsert: { collection: target.name, document: original, backed_up_at: new Date() } },
                { upsert: true },
            )
            const roots = target.roots.filter((key) => original[key] !== undefined)
            const expected = Object.fromEntries(roots.map((key) => [key, original[key]]))
            const changed = Object.fromEntries(roots.map((key) => [key, document[key]]))
            const result = collection.updateOne({ _id: original._id, ...expected }, { $set: changed })
            if (result.matchedCount !== 1) throw new Error("检测到并发修改，停止迁移；请确认业务已停写")
            modified += result.modifiedCount
        }
    }
    const remaining = collection.countDocuments(query, { maxTimeMS: 60000 })
    results.push({ collection: target.name, matched, modified, remaining })
    if (apply && remaining) throw new Error(`${target.name} 仍有旧字段残留`)
}
printjson({ migration: "20261022-simplify-inquiry-feedback", mode: apply ? "apply" : "preview", results })
