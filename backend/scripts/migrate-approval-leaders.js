// mongosh 连接明确的目标库后执行。新增字段由读取默认值兼容，无需回填。
// ERP_APPROVAL_LEADERS_MIGRATION=check|apply|rollback，默认 check；任何模式都不删除历史。
const mode = process.env.ERP_APPROVAL_LEADERS_MIGRATION || "check";
if (!["check", "apply", "rollback"].includes(mode)) throw new Error("迁移模式无效");
if (["admin", "config", "local", "test"].includes(db.getName())) throw new Error("请显式选择业务数据库");
const rules = ["APPLICANT_TEAM_LEADER", "APPLICANT_DEPARTMENT_LEADER"];
const nodes = db.getCollection("approval_node_definitions");
if (nodes.findOne({ assignee_rule: { $exists: true, $ne: null, $nin: rules } })) {
    throw new Error("存在未知审批人来源，停止切换");
}
if (nodes.findOne({ assignee_rule: { $in: rules }, $or: [
    { assignee_participant_id: { $ne: null } },
    { "additional_assignees.0": { $exists: true } },
    { resolution: { $ne: null } },
] })) throw new Error("动态定义含具体人员或运行快照，停止切换");
if (mode === "rollback") {
    const incompatible = [
        ["org_units", { leader: { $ne: null } }],
        ["org_changes", { "request.change.operation": "set_leader" }],
        ["org_changes", { "after.units.leader": { $type: "object" } }],
        ["approval_node_definitions", { assignee_rule: { $in: rules } }],
        ["approval_instance_assignees", { resolution: { $ne: null } }],
    ];
    for (const [collection, filter] of incompatible) {
        if (db.getCollection(collection).findOne(filter)) {
            throw new Error(`集合 ${collection} 已存在负责人配置或动态审批历史，禁止回滚旧程序`);
        }
    }
}
print(JSON.stringify({ database: db.getName(), mode, passed: true, backfill_required: false }));
