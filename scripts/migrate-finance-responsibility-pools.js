// mongosh <目标库连接> scripts/migrate-finance-responsibility-pools.js
// 执行前停止旧版写入并备份规则及轮次集合；回滚按财务责任合同恢复备份。
const rules = db.getCollection("finance_responsibility_rules")
rules.updateMany(
    { owner_user_ids: { $exists: false }, owner_user_id: { $type: "string" } },
    [{ $set: { owner_user_ids: ["$owner_user_id"] } }],
)
rules.updateMany(
    { strategy: { $exists: false } },
    { $set: { strategy: "ROUND_ROBIN" } },
)
rules.updateMany(
    { owner_user_ids: { $exists: true }, owner_user_id: { $exists: true } },
    { $unset: { owner_user_id: "" } },
)
print("财务责任规则已迁为负责人池；已有池及轮次保持不变。")
