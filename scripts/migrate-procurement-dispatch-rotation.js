// 停止旧版本写入并备份规则与轮转集合后，在目标业务库执行：
// mongosh '<目标业务库 URI>' --file scripts/migrate-procurement-dispatch-rotation.js
// 新版后端启动包含同等的幂等迁移；执行与回滚要求见 docs/procurement-responsibility-contract.md。
const rules = db.getCollection("procurement_responsibility_rules")
const rotations = db.getCollection("procurement_dispatch_rotation")
rules.updateMany(
    { owner_user_ids: { $exists: false }, owner_user_id: { $type: "string" } },
    [{ $set: { owner_user_ids: ["$owner_user_id"] } }],
)
rules.updateMany(
    { strategy: { $exists: false } },
    { $set: { strategy: "ROUND_ROBIN" } },
)
const defaults = rules.find({ rule_type: "DEFAULT_DISPATCHER", status: "active", deleted_at: NumberLong(0) }).sort({ id: 1 }).toArray()
if (defaults.length > 0) {
    const first = defaults[0]
    const members = [...new Set(defaults.flatMap((rule) => rule.owner_user_ids))]
    if (members.length < 1 || members.length > 100) {
        throw new Error("默认负责人池须包含 1 至 100 人，请维护旧规则后重新执行")
    }
    const old = rotations.findOne({ _id: "default" })
    const lastRule = old?.last_rule_id ? rules.findOne({ id: old.last_rule_id }) : null
    const lastOwner = lastRule?.owner_user_ids?.[0] ?? null
    // 先保存完整成员，再停用重复规则；中断后重试仍保留已合并成员。
    rules.updateOne({ id: first.id }, { $set: { owner_user_ids: members } })
    const others = defaults.slice(1).map((rule) => rule.id)
    if (others.length > 0) {
        rules.updateMany({ id: { $in: others } }, { $set: { status: "disabled", migration_merged_into: first.id } })
    }
    rotations.updateOne(
        { _id: first.id },
        { $setOnInsert: { last_owner_user_id: lastOwner, revision: NumberLong(0) } },
        { upsert: true },
    )
}
rules.updateMany({ rule_type: "DEFAULT_DISPATCHER" }, { $set: { selector_key: "default_dispatcher" } })
rules.updateMany({ owner_user_id: { $exists: true } }, { $unset: { owner_user_id: "" } })
printjson({ migration: "20261023-procurement-responsibility-pools", mergedDefaultRules: Math.max(0, defaults.length - 1) })
