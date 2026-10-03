// 在目标业务库执行：mongosh '<目标业务库 URI>' --file scripts/migrate-sales-fulfillment-evidence.js
// 物流只使用新的销售明细关联；本入口不转换旧单号、不改写单据或附件，只校验并列出回退影响。

const salesFulfillmentFieldChecks = [
    ["sales_orders", "evidence_file_asset_ids", "array"],
    ["deliveries", "tracking_entries", "array"],
    ["customer_acceptances", "evidence_attachment_id", "string"],
]

for (const [collectionName, fieldName, expectedType] of salesFulfillmentFieldChecks) {
    const fieldFilter = { $exists: true, $not: { $type: expectedType } }
    if (expectedType === "string") fieldFilter.$ne = null
    const invalid = db.getCollection(collectionName).findOne(
        { [fieldName]: fieldFilter },
        { _id: 1 },
        { maxTimeMS: 10000 },
    )
    if (invalid) {
        throw new Error(`${collectionName}.${fieldName} 存在不兼容字段类型，停止切换并修复数据。`)
    }
}

const salesFulfillmentRollbackImpact = {
    erpOrdersWithoutContract: db.getCollection("sales_orders").countDocuments(
        { origin_system: "ERP", deleted_at: null, $or: [{ contract_id: null }, { contract_id: "" }] },
        { maxTimeMS: 10000 },
    ),
    deliveriesWithTrackingEntries: db.getCollection("deliveries").countDocuments(
        { deleted_at: null, "tracking_entries.0": { $exists: true } },
        { maxTimeMS: 10000 },
    ),
    acceptancesWithEvidence: db.getCollection("customer_acceptances").countDocuments(
        { deleted_at: null, evidence_attachment_id: { $type: "string", $ne: "" } },
        { maxTimeMS: 10000 },
    ),
}

printjson({
    migration: "20261003-sales-fulfillment-evidence",
    dataWrites: 0,
    fieldCompatibility: "passed",
    rollbackImpact: salesFulfillmentRollbackImpact,
})
