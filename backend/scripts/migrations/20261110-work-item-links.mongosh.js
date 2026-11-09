// MODE=audit（默认）|apply|rollback；每次只处理 BATCH_SIZE 条，输出游标供续跑。
// apply/rollback 必须在停止相关业务写入后执行。连接由调用方提供，脚本不输出连接信息。
(async () => {
    const mode = process.env.MODE || "audit";
    const limit = Number(process.env.BATCH_SIZE || 200);
    const after = process.env.AFTER_ID || "";
    const collection = process.env.COLLECTION || "approval_subject_snapshots";
    if (!["audit", "apply", "rollback"].includes(mode) || !Number.isInteger(limit) || limit < 1 || limit > 1000)
        throw new Error("MODE 或 BATCH_SIZE 无效");
    if (!["approval_subject_snapshots", "work_items"].includes(collection)) throw new Error("COLLECTION 无效");
    if (mode !== "audit" && process.env.WRITES_STOPPED !== "yes") throw new Error("必须先停止业务写入并设置 WRITES_STOPPED=yes");
    const backups = db.getCollection("work_item_link_migration_backups");
    const target = db.getCollection(collection);
    const migration = "20261110-work-item-links";
    const missing = { $or: [{ links: { $exists: false } }, { links: { $size: 0 } }] };
    const stats = { collection, mode, scanned: 0, changed: 0, conflicts: 0, unresolved: 0, last_id: after };
    const collections = {
        purchase_order: "purchase_orders", sales_change_order: "sales_change_orders", purchase_change_order: "purchase_change_orders",
        purchase_receipt: "purchase_receipts", delivery: "deliveries", electronic_delivery: "electronic_deliveries", service_fulfillment: "service_fulfillments", customer_acceptance: "customer_acceptances",
        receivable_account: "receivable_accounts", receivable_entry: "receivable_entries", payable_account: "payable_accounts", payable_entry: "payable_entries",
        sales_invoice_request: "sales_invoice_requests", purchase_payment_request: "purchase_payment_requests", customer_receipt: "customer_receipts", supplier_payment: "supplier_payments", invoice: "invoices",
        sales_return_case: "sales_return_cases", purchase_return_order: "purchase_return_orders", customer_refund: "customer_refunds", supplier_refund: "supplier_refunds", receipt_reversal: "receipt_reversals", payment_reversal: "payment_reversals",
    };
    const reference = (kind, id) => {
        if (typeof id !== "string" || !id.trim() || id.trim().length > 128) throw new Error("关联对象 ID 无效");
        return { object_type: kind, object_id: id.trim() };
    };
    const read = async (kind, id) => {
        const row = await db.getCollection(collections[kind]).findOne({ id, deleted_at: 0 });
        if (!row) throw new Error("关联对象缺失或已删除");
        return row;
    };
    async function parents(kind, id, frozenRequest) {
        if (kind === "voucher_sales_order") return [reference("sales_order", id)];
        if (!collections[kind]) return [];
        const row = kind === "purchase_payment_request" && frozenRequest?.id === id ? frozenRequest : await read(kind, id);
        const refs = [];
        const add = (type, value) => { if (value != null) refs.push(reference(type, value)); };
        if (["purchase_order", "sales_change_order", "customer_acceptance", "sales_return_case", "receivable_account"].includes(kind)) add("sales_order", row.sales_order_id);
        if (["purchase_change_order", "purchase_receipt", "electronic_delivery", "service_fulfillment", "purchase_return_order"].includes(kind)) add("purchase_order", row.purchase_order_id);
        if (kind === "delivery") { add("sales_order", row.sales_order_id); add("purchase_order", row.purchase_order_id); }
        if (kind === "purchase_return_order") add("sales_return_case", row.sales_return_case_id);
        if (kind === "receivable_entry") add("receivable_account", row.receivable_account_id);
        if (kind === "payable_entry") add("payable_account", row.payable_account_id);
        if (kind === "payable_account") {
            if (!["purchase_order", "supplier_settlement"].includes(row.source_type)) throw new Error("应付来源类型不可识别");
            add(row.source_type, row.source_document_id);
        }
        if (kind === "sales_invoice_request") { add("sales_order", row.sales_order_id); add("receivable_account", row.receivable_account_id); }
        if (kind === "purchase_payment_request") for (const line of row.lines || []) {
            add("purchase_order", line.purchase_order_id); add("payable_account", line.payable_account_id);
            for (const sales of line.source_snapshot.sales_sources) add("sales_order", sales.sales_order_id);
        }
        if (kind === "customer_receipt") for (const allocation of row.pending_allocations || []) add("receivable_entry", allocation.receivable_entry_id);
        if (kind === "supplier_payment") {
            add("purchase_payment_request", row.payment_request_id);
            for (const allocation of row.request_allocations || []) add("payable_account", allocation.payable_account_id);
        }
        if (kind === "invoice") { add("sales_invoice_request", row.sales_invoice_request_id); add("invoice", row.original_invoice_id); }
        if (kind === "customer_refund") {
            add("sales_return_case", row.sales_return_case_id); add("customer_receipt", row.original_receipt_id); add("receivable_entry", row.original_receivable_entry_id);
        }
        if (kind === "supplier_refund") {
            add("purchase_return_order", row.purchase_return_order_id); add("supplier_payment", row.original_payment_id); add("payable_entry", row.original_payable_entry_id); add("purchase_order", row.source_purchase_order_id);
        }
        if (kind === "receipt_reversal") add("customer_receipt", row.original_customer_receipt_id);
        if (kind === "payment_reversal") add("supplier_payment", row.original_supplier_payment_id);
        return refs;
    }
    async function capture(kind, id, frozenRequest) {
        const seen = new Map();
        const pending = [reference(kind, id)];
        while (pending.length) {
            const ref = pending.pop(); const key = JSON.stringify([ref.object_type, ref.object_id]);
            if (seen.has(key)) continue;
            seen.set(key, ref);
            if (seen.size > 1000) throw new Error("关联链路超过1000个");
            pending.push(...await parents(ref.object_type, ref.object_id, frozenRequest));
        }
        return [...seen.values()].sort((a, b) => a.object_type.localeCompare(b.object_type) || a.object_id.localeCompare(b.object_id));
    }
    async function snapshotLinks(snapshot) {
        const kind = snapshot.document_type;
        const id = snapshot.business_object_id;
        let frozenRequest;
        if (kind === "purchase_payment_request") {
            const source = await db.getCollection("purchase_payment_request_snapshots").findOne({ request_id: id, submission_version: snapshot.subject_version, approval_instance_id: snapshot.approval_process_instance_id, deleted_at: 0 });
            if (!source) throw new Error("付款申请缺少精确版本快照，禁止使用当前申请");
            frozenRequest = source.request;
        }
        if (kind === "customer_receipt") {
            const row = await read(kind, id);
            if (String(row.approval_subject_version) !== String(snapshot.subject_version) || row.status === "DRAFT") throw new Error("回款来源版本无法证明，禁止使用当前草稿");
        }
        return capture(kind, id, frozenRequest);
    }
    async function taskLinks(task) {
        let instanceId;
        if (task.approval_node_execution_id) {
            const node = await db.getCollection("approval_node_executions").findOne({ id: task.approval_node_execution_id, deleted_at: 0 });
            if (!node) throw new Error("审批执行节点缺失");
            instanceId = node.process_instance_id;
        } else if (task.work_item_type === "APPROVAL_REWORK") {
            if (!task.responsibility_key?.startsWith("approval_rework:")) throw new Error("驳回任务实例关联缺失");
            instanceId = task.responsibility_key.slice("approval_rework:".length);
        }
        if (instanceId) {
            const snapshot = await db.getCollection("approval_subject_snapshots").findOne({ approval_process_instance_id: instanceId, business_object_id: task.business_object_id, document_type: task.business_object_type, deleted_at: 0 });
            if (!snapshot || String(snapshot.subject_version) !== task.subject_version) throw new Error("任务与审批冻结版本不匹配");
            return snapshot.links?.length ? snapshot.links : snapshotLinks(snapshot);
        }
        if (task.business_object_type === "purchase_payment_request") {
            const source = await db.getCollection("purchase_payment_request_snapshots").findOne({ request_id: task.business_object_id, submission_version: Number(task.subject_version), deleted_at: 0 });
            if (!source) throw new Error("付款执行任务缺少精确申请快照");
            return capture(task.business_object_type, task.business_object_id, source.request);
        }
        if (task.work_item_type === "SALES_INVOICE_EXECUTION") {
            const request = await db.getCollection("sales_invoice_requests").findOne({ work_item_id: task.id, receivable_account_id: task.business_object_id, deleted_at: 0 });
            if (!request) throw new Error("开票任务缺少对应申请");
            const refs = await capture("sales_invoice_request", request.id);
            if (!refs.some(ref => ref.object_type === task.business_object_type && ref.object_id === task.business_object_id)) throw new Error("开票任务直接对象与申请不一致");
            return refs;
        }
        return capture(task.business_object_type, task.business_object_id);
    }
    if (mode === "rollback") {
        const rows = await backups.find({ migration, collection, id: { $gt: after }, rolled_back: { $ne: true } }).sort({ id: 1 }).limit(limit).toArray();
        for (const saved of rows) {
            stats.scanned++; stats.last_id = saved.id;
            const update = saved.had_links ? { $set: { links: saved.before }, $inc: { version: 1 } } : { $unset: { links: "" }, $inc: { version: 1 } };
            const result = await target.updateOne({ id: saved.id, version: saved.after_version, links: saved.after }, update);
            if (!result.matchedCount) { stats.conflicts++; continue; }
            await backups.updateOne({ _id: saved._id }, { $set: { rolled_back: true } }); stats.changed++;
        }
    } else {
        const rows = await target.find({ id: { $gt: after }, deleted_at: 0, ...missing }).sort({ id: 1 }).limit(limit).toArray();
        for (const row of rows) {
            stats.scanned++; stats.last_id = row.id;
            try {
                const links = collection === "work_items" ? await taskLinks(row) : await snapshotLinks(row);
                if (mode === "apply") {
                    const backup = { _id: `${migration}:${collection}:${row.id}`, migration, collection, id: row.id, had_links: Object.hasOwn(row, "links"), before: row.links || [], after: links, before_version: row.version, after_version: NumberLong((BigInt(row.version.toString()) + 1n).toString()) };
                    await backups.updateOne({ _id: backup._id }, { $setOnInsert: backup }, { upsert: true });
                    const saved = await backups.findOne({ _id: backup._id });
                    if (String(saved.before_version) !== String(row.version) || EJSON.stringify(saved.after) !== EJSON.stringify(links) || saved.rolled_back) throw new Error("备份与当前版本不同，必须人工核对");
                    const result = await target.updateOne({ id: row.id, version: row.version, ...missing }, { $set: { links }, $inc: { version: 1 } });
                    if (!result.matchedCount) { stats.conflicts++; continue; }
                }
                stats.changed++;
            } catch (error) {
                stats.unresolved++; print(JSON.stringify({ collection, id: row.id, error: error.message }));
            }
        }
        if (mode === "apply") await db.getCollection("work_items").createIndex({ "links.object_type": 1, "links.object_id": 1, owner_user_id: 1, status: 1, deleted_at: 1 }, { name: "idx_work_items_links_owner_status" });
    }
    print(JSON.stringify(stats));
    if (stats.conflicts || stats.unresolved) throw new Error("本批存在未完成项；修复后从原游标重跑，禁止直接切换读取");
})();
