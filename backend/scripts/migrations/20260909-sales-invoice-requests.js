// 在业务停写窗口执行。默认只读；必须显式声明数据库名称和执行模式。
// INVOICE_MIGRATION_DB=<业务库名> mongosh <连接参数> --file scripts/migrations/20260909-sales-invoice-requests.js
// 实际执行时另设 INVOICE_MIGRATION_APPLY=yes。先保存 dry-run 输出和数据库备份。
const expectedDb = process.env.INVOICE_MIGRATION_DB;
if (!expectedDb || db.getName() !== expectedDb || ["admin", "local", "config"].includes(expectedDb)) {
    throw new Error("请通过 INVOICE_MIGRATION_DB 指定当前业务数据库名称");
}
const apply = process.env.INVOICE_MIGRATION_APPLY === "yes";
const tasks = db.getCollection("work_items");
const requests = db.getCollection("sales_invoice_requests");
const oldIndex = "uk_work_items_open_sales_invoice_execution_object";
const newIndex = "uk_work_items_open_sales_invoice_execution_request";
const openFilter = { deleted_at: 0, status: "OPEN", work_item_type: "SALES_INVOICE_EXECUTION" };
const legacy = tasks.aggregate([
    { $match: openFilter },
    { $lookup: { from: "sales_invoice_requests", localField: "id", foreignField: "work_item_id", as: "requests" } },
    { $match: { requests: { $size: 0 } } },
    { $project: { requests: 0 } },
]).toArray();
const indexes = tasks.getIndexes();
printjson({ mode: apply ? "apply" : "dry-run", database: db.getName(), openTasks: tasks.countDocuments(openFilter), legacyTasksToClose: legacy.length, approvedRequests: requests.countDocuments({ status: "approved", deleted_at: 0 }), oldIndexPresent: indexes.some((index) => index.name === oldIndex), newIndexPresent: indexes.some((index) => index.name === newIndex) });
for (const task of legacy) printjson({ taskId: task.id, receivableAccountId: task.business_object_id, version: task.version });
if (apply) {
    // 先建立新约束，再退役旧任务，最后移除旧约束；中断后允许重跑。
    tasks.createIndex({ business_object_type: 1, business_object_id: 1, work_item_type: 1, responsibility_key: 1 }, { name: newIndex, unique: true, partialFilterExpression: { status: "OPEN", work_item_type: "SALES_INVOICE_EXECUTION" } });
    for (const task of legacy) {
        const session = db.getMongo().startSession();
        try {
            session.withTransaction(() => {
                const tx = session.getDatabase(expectedDb);
                if (tx.getCollection("sales_invoice_requests").findOne({ work_item_id: task.id })) throw new Error(`任务 ${task.id} 已关联申请，停止迁移并重新预览`);
                const now = Math.floor(Date.now() / 1000);
                const result = tx.getCollection("work_items").updateOne({ ...openFilter, id: task.id, version: task.version }, { $set: { status: "CLOSED", closed_at: now, closed_by: "migration-invoice-approval", close_reason: "切换开票申请审批，旧自动任务退役；有开票需求须重新申请", updated_at: now }, $inc: { version: 1 } });
                if (result.modifiedCount !== 1) throw new Error(`任务 ${task.id} 已变化，停止迁移并重新预览`);
                tx.getCollection("audit_logs").insertOne({ id: `invoice-request-cutover-${task.id}`, version: 1, created_at: now, updated_at: now, deleted_at: 0, actor_id: "migration-invoice-approval", actor_account: "migration-invoice-approval", actor_type: "admin", action: "sales_invoice_request.retire_legacy_task", resource_type: "work_item", resource_id: task.id, success: true, message: JSON.stringify({ previous_status: task.status, previous_version: task.version, receivable_account_id: task.business_object_id }) });
            });
        } finally { session.endSession(); }
    }
    if (indexes.some((index) => index.name === oldIndex)) tasks.dropIndex(oldIndex);
    printjson({ migrationCompleted: true, closedLegacyTasks: legacy.length });
}
