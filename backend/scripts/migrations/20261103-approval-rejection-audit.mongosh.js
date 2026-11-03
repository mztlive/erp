// Read-only inventory. Run in mongosh with a read-only database account.
// Required environment: APPROVAL_AUDIT_DB, APPROVAL_AUDIT_STATUS (RUNNING/BLOCKED),
// APPROVAL_AUDIT_FROM and APPROVAL_AUDIT_TO (Unix seconds, inclusive).
// Optional: APPROVAL_AUDIT_LIMIT (1..100, default 50), APPROVAL_AUDIT_AFTER_TIME,
// APPROVAL_AUDIT_AFTER_ID (both cursor fields), APPROVAL_AUDIT_HISTORY_LIMIT (1..500).
// No writes, index creation, or automatic disposition are performed.
(function auditRejectionRestarts() {
    "use strict";
    const env = process.env;
    function integer(name, fallback, min, max) {
        const raw = env[name] ?? fallback;
        if (raw === undefined || !/^\d+$/.test(String(raw))) throw new Error(`${name} must be an integer`);
        const value = Number(raw);
        if (!Number.isSafeInteger(value) || value < min || value > max) throw new Error(`${name} out of range`);
        return value;
    }
    const databaseName = env.APPROVAL_AUDIT_DB;
    if (!databaseName || !/^[A-Za-z0-9_-]+$/.test(databaseName)) throw new Error("APPROVAL_AUDIT_DB required");
    if (["admin", "config", "local"].includes(databaseName)) throw new Error("System database is not allowed");
    const status = env.APPROVAL_AUDIT_STATUS;
    if (!["RUNNING", "BLOCKED"].includes(status)) throw new Error("Specify RUNNING or BLOCKED explicitly");
    const from = integer("APPROVAL_AUDIT_FROM", undefined, 0, Number.MAX_SAFE_INTEGER);
    const to = integer("APPROVAL_AUDIT_TO", undefined, from, Number.MAX_SAFE_INTEGER);
    const limit = integer("APPROVAL_AUDIT_LIMIT", 50, 1, 100);
    const historyLimit = integer("APPROVAL_AUDIT_HISTORY_LIMIT", 500, 1, 500);
    const afterId = env.APPROVAL_AUDIT_AFTER_ID;
    const afterTime = env.APPROVAL_AUDIT_AFTER_TIME;
    if ((afterId === undefined) !== (afterTime === undefined)) throw new Error("Both cursor fields are required");
    if (afterId !== undefined && (!afterId.trim() || afterId.length > 200)) throw new Error("Invalid cursor ID");
    const database = db.getSiblingDB(databaseName);
    const instances = database.getCollection("approval_process_instances");
    const executions = database.getCollection("approval_node_executions");
    const tasks = database.getCollection("work_items");
    const filter = { status, deleted_at: 0, updated_at: { $gte: from, $lte: to } };
    if (afterId !== undefined) {
        const time = integer("APPROVAL_AUDIT_AFTER_TIME", undefined, from, to);
        filter.$or = [{ updated_at: { $lt: time } }, { updated_at: time, id: { $lt: afterId } }];
    }
    const instanceProjection = {
        _id: 0, id: 1, version: 1, updated_at: 1, status: 1, process_kind: 1,
        subject: 1, subject_version: 1, current_round_no: 1, current_node_execution_id: 1,
    };
    const page = instances.find(filter, instanceProjection)
        .hint("idx_approval_process_instances_status_updated")
        .sort({ updated_at: -1, id: -1 }).limit(limit + 1).maxTimeMS(10000).toArray();
    const scanned = page.slice(0, limit);
    let candidates = 0;
    for (const instance of scanned) {
        if (Number(instance.current_round_no) < 2) continue;
        const history = executions.find({ process_instance_id: instance.id, deleted_at: 0 }, {
            _id: 0, id: 1, version: 1, execution_no: 1, round_no: 1, status: 1,
            decision: 1, assignment_source: 1, replaces_execution_id: 1,
        }).hint("uk_approval_node_executions_instance_no")
            .sort({ execution_no: 1 }).limit(historyLimit + 1).maxTimeMS(10000).toArray();
        const overflow = history.length > historyLimit;
        const visibleHistory = history.slice(0, historyLimit);
        const rejected = visibleHistory.filter(item => item.status === "REJECTED");
        const current = visibleHistory.find(item => item.id === instance.current_node_execution_id);
        const firstRejection = rejected[0];
        const latestRejection = rejected[rejected.length - 1];
        const followed = firstRejection ? visibleHistory.filter(item =>
            Number(item.execution_no) > Number(firstRejection.execution_no)) : [];
        const continued = followed.some(item => ["APPROVED", "REJECTED"].includes(item.status));
        const untouched = !overflow && latestRejection && followed.length === 1 && current &&
            followed[0].id === current.id && ["ACTIVE", "BLOCKED"].includes(current.status) &&
            current.assignment_source === "DEFINITION" && !current.replaces_execution_id &&
            Number(current.round_no) === Number(latestRejection.round_no) + 1 &&
            Number(current.round_no) === Number(instance.current_round_no);
        const executionIds = visibleHistory.map(item => item.id);
        const openTasks = executionIds.length ? tasks.find({
            approval_node_execution_id: { $in: executionIds, $type: "string" },
            work_item_type: "DOCUMENT_APPROVAL", status: "OPEN", deleted_at: 0,
        }, { _id: 0, id: 1, version: 1, approval_node_execution_id: 1 })
            .hint("uk_work_items_approval_execution").limit(501).maxTimeMS(10000).toArray() : [];
        const fresh = instances.find({ id: instance.id }, instanceProjection)
            .hint("uk_approval_process_instances_id").limit(1).maxTimeMS(10000).toArray()[0];
        const unchanged = fresh && String(fresh.version) === String(instance.version) &&
            fresh.status === instance.status && fresh.current_node_execution_id === instance.current_node_execution_id;
        const taskShapeValid = openTasks.length <= 500 && openTasks.every(item =>
            item.approval_node_execution_id === instance.current_node_execution_id) &&
            (status === "BLOCKED" ? openTasks.length === 0 : openTasks.length > 0);
        let classification = "REVIEW_REQUIRED";
        if (!overflow && unchanged && taskShapeValid && current && latestRejection) {
            if (continued) classification = "CONTINUED_AFTER_REJECTION";
            else if (untouched) classification = "RESTARTED_UNTOUCHED";
        }
        candidates += 1;
        printjson({
            kind: "candidate", classification,
            instance: { ...instance, version: String(instance.version) },
            first_rejected_execution_id: firstRejection?.id ?? null,
            latest_rejected_execution_id: latestRejection?.id ?? null,
            current_execution_version: current ? String(current.version) : null,
            open_tasks: openTasks.slice(0, 500).map(item => ({ ...item, version: String(item.version) })),
            history_truncated: overflow, task_list_truncated: openTasks.length > 500,
            changed_during_read: !unchanged,
            requires_application_revalidation: true,
        });
    }
    const last = scanned[scanned.length - 1];
    printjson({
        kind: "page", database: databaseName, status, updated_from: from, updated_to: to,
        scanned: scanned.length, candidates, has_more: page.length > limit,
        next_cursor: page.length > limit && last ? { updated_at: String(last.updated_at), id: last.id } : null,
        read_only: true, consistent_snapshot: false,
    });
})();
