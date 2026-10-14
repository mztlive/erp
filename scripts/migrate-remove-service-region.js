// 在目标业务库执行：mongosh '<目标业务库 URI>' --file scripts/migrate-remove-service-region.js
// 销售明细不再记录服务区域，采购责任规则取消“分类 + 服务区域”类型，采购影响取消“服务地区变化”。
// 新应用无法读取旧规则类型与旧影响原因，须在启动新应用前执行；可重复执行。
// 回滚：执行前导出 procurement_responsibility_rules 中 rule_type 为 CATEGORY_SERVICE_REGION 的文档，
// 回退旧应用后再导入；销售明细中删除的 service_region 不回填。

const options = { maxTimeMS: 10000 }

const regionRules = db.getCollection("procurement_responsibility_rules")
const removedRules = regionRules.deleteMany(
    { rule_type: "CATEGORY_SERVICE_REGION" },
    options,
).deletedCount
const ruleFields = regionRules.updateMany(
    { service_region: { $exists: true } },
    { $unset: { service_region: "" } },
    options,
).modifiedCount

const sourceUpdates = db.getCollection("purchase_source_updates")
const regionImpacts = sourceUpdates.updateMany(
    { "reasons.kind": "SERVICE_REGION_CHANGED" },
    { $pull: { reasons: { kind: "SERVICE_REGION_CHANGED" } } },
    options,
).modifiedCount
const emptyImpacts = sourceUpdates.countDocuments({ reasons: { $size: 0 } }, options)

const salesLineCollections = [
    "sales_order_working_copy_lines",
    "sales_order_submission_lines",
    "sales_order_goods_service_line_revisions",
    "sales_change_submission_lines",
]
const salesLineFields = Object.fromEntries(
    salesLineCollections.map((name) => [
        name,
        db.getCollection(name).updateMany(
            { service_region: { $exists: true } },
            { $unset: { service_region: "" } },
            options,
        ).modifiedCount,
    ]),
)

printjson({
    migration: "20261014-remove-service-region",
    removedRegionRules: removedRules,
    rulesWithoutServiceRegion: ruleFields,
    sourceUpdatesWithoutRegionReason: regionImpacts,
    sourceUpdatesWithNoReasonLeft: emptyImpacts,
    salesLinesWithoutServiceRegion: salesLineFields,
})

if (emptyImpacts > 0) {
    print("存在只含服务地区变化原因的采购影响记录，原因已清空；请在采购详情核对后按当前销售版本重新判定。")
}
