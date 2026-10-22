// 停止业务写入并备份目标库后执行。默认仅预览；INQUIRY_FEES_MIGRATION=apply 执行。
// mongosh '<目标业务库 URI>' --file scripts/migrate-simplify-inquiry-fees.js
// 回滚：保持停写，恢复迁移前全库备份及旧版程序；发生新业务后须先核对新增记录。
// 不修改费用身份、金额、税率、商品范围、历史有效期或费用占用台账。
const apply = process.env.INQUIRY_FEES_MIGRATION === "apply"
const fields = ["category", "allocation_ids", "included_in_goods_price", "confirmation", "requires_erp_payment", "valid_conditions"]
const targets = [
    { name: "sales_inquiries", paths: ["feedbacks.data.fees", "feedback_work.data.fees"], roots: ["feedbacks", "feedback_work"] },
    { name: "sales_order_inquiry_sources", paths: ["fees"], roots: ["fees"] },
]
function entries(document, target) {
    if (target.name === "sales_order_inquiry_sources") return (document.fees ?? []).map(fee => ({ fee, published: true }))
    return [
        ...(document.feedbacks ?? []).flatMap(snapshot => (snapshot.data.fees ?? []).map(fee => ({ fee, published: true }))),
        ...(document.feedback_work ?? []).flatMap(work => (work.data.fees ?? []).map(fee => ({ fee, published: false }))),
    ]
}
function note(fee) {
    return [fee.public_note, fee.valid_conditions].filter(value => typeof value === "string" && value.trim()).join("\n")
}
// 先完成所有集合预检，再开始写入。不能把已含价项目变成额外成本，或把历史暂估变成可办理费用。
const blockers = []
const plans = targets.map(target => {
    const query = { $or: target.paths.flatMap(path => fields.map(field => ({ [`${path}.${field}`]: { $exists: true } }))) }
    const collection = db.getCollection(target.name)
    let matched = 0
    const cursor = collection.find(query).batchSize(100)
    while (cursor.hasNext()) {
        const document = cursor.next()
        matched++
        for (const { fee, published } of entries(document, target)) {
            const reasons = []
            if (fee.included_in_goods_price === true) reasons.push("费用已含商品价")
            if (published && fee.confirmation === "estimated") reasons.push("已发布暂估费用")
            if ((fee.allocation_ids ?? []).length) reasons.push("费用有指定地址范围")
            if (Array.from(note(fee)).length > 4000) reasons.push("合并备注超过 4000 字符")
            if (reasons.length) blockers.push({ collection: target.name, id: document._id, fee_id: fee.fee_id, reasons })
        }
    }
    return { target, query, matched }
})
if (blockers.length) {
    printjson({ migration: "20261022-simplify-inquiry-fees", blockers })
    throw new Error("存在需核对的历史费用；本次未修改数据，请先确认处理后重试")
}
const backup = db.getCollection("migration_20261022_inquiry_fees_originals")
const results = []
for (const { target, query, matched } of plans) {
    const collection = db.getCollection(target.name)
    let modified = 0
    if (apply) {
        const cursor = collection.find(query).batchSize(100)
        while (cursor.hasNext()) {
            const original = cursor.next()
            const document = EJSON.parse(EJSON.stringify(original))
            for (const { fee } of entries(document, target)) {
                fee.public_note = note(fee)
                for (const field of fields) delete fee[field]
            }
            backup.updateOne(
                { _id: `${target.name}:${original._id.toString()}` },
                { $setOnInsert: { collection: target.name, document: original, backed_up_at: new Date() } },
                { upsert: true },
            )
            const roots = target.roots.filter(key => original[key] !== undefined)
            const expected = Object.fromEntries(roots.map(key => [key, original[key]]))
            const changed = Object.fromEntries(roots.map(key => [key, document[key]]))
            const result = collection.updateOne({ _id: original._id, ...expected }, { $set: changed })
            if (result.matchedCount !== 1) throw new Error("检测到并发修改，停止迁移；请确认业务已停写")
            modified += result.modifiedCount
        }
    }
    const remaining = collection.countDocuments(query, { maxTimeMS: 60000 })
    results.push({ collection: target.name, matched, modified, remaining })
    if (apply && remaining) throw new Error(`${target.name} 仍有旧字段残留`)
}
printjson({ migration: "20261022-simplify-inquiry-fees", mode: apply ? "apply" : "preview", results })
