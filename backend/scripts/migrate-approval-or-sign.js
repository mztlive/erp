// 使用 mongosh 连接明确的目标库执行。默认只检查；ERP_OR_SIGN_MIGRATION=apply 执行升级，rollback 回滚。
const mode = process.env.ERP_OR_SIGN_MIGRATION || "check";
if (!["check", "apply", "rollback"].includes(mode)) throw new Error("迁移模式无效");
const collection = db.getCollection("work_items");
const name = "uk_work_items_approval_execution";
const oldKeys = { approval_node_execution_id: 1 };
const newKeys = { approval_node_execution_id: 1, owner_user_id: 1 };
const keys = mode === "rollback" ? oldKeys : newKeys;
const current = collection.getIndexes().find((index) => index.name === name);
if (current && JSON.stringify(current.key) !== JSON.stringify(oldKeys) && JSON.stringify(current.key) !== JSON.stringify(newKeys)) {
    throw new Error("现有索引结构未知，停止迁移");
}
if (mode === "rollback") {
    for (const name of ["approval_node_definitions", "approval_node_executions", "approval_instance_assignees"]) {
        if (db.getCollection(name).findOne({ "additional_assignees.0": { $exists: true } })) {
            throw new Error("已有或签定义或运行记录，禁止回退到不支持或签的版本");
        }
    }
}
const group = mode === "rollback" ? "$approval_node_execution_id" : { execution: "$approval_node_execution_id", owner: "$owner_user_id" };
const duplicate = collection.aggregate([
    { $match: { approval_node_execution_id: { $type: "string" } } },
    { $group: { _id: group, count: { $sum: 1 } } },
    { $match: { count: { $gt: 1 } } },
    { $limit: 1 },
]).hasNext();
if (duplicate) throw new Error("既有审批任务违反目标唯一性，停止迁移");
if (mode !== "check") {
    if (current && JSON.stringify(current.key) !== JSON.stringify(keys)) collection.dropIndex(name);
    collection.createIndex(keys, { name, unique: true, partialFilterExpression: { approval_node_execution_id: { $type: "string" } } });
}
print(JSON.stringify({ database: db.getName(), mode, index: name, keys, passed: true }));
