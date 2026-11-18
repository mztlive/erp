// 执行前停止全部授权事件生产方、维护入口及 worker，并确认在途事务已排空。
// 设置全局 resourceGrantEventWritesStopped = true 后加载本文件；允许失败后重复执行。
// 完成回填和唯一索引创建后，统一部署支持 dedup_key 的应用，再恢复生产与消费。
// 不支持新旧写入方混跑；本脚本保留历史 id、处理状态、租约和业务证据。
// 回滚：新版本尚未写入时，先删除 uk_resource_grant_events_dedup_key 索引，再恢复旧应用；保留字段。
// 新版本已写入后禁止直接恢复旧生产方；须保持入口关闭并前向修复，禁止删除事件或改写 id。
(() => {
  if (typeof resourceGrantEventWritesStopped === "undefined" || resourceGrantEventWritesStopped !== true) {
    throw new Error("必须停止事件写入与消费并排空事务，显式设置 resourceGrantEventWritesStopped = true");
  }
  const events = db.getCollection("resource_grant_events");
  const missing = {dedup_key: {$exists: false}};
  // 回填前按授权事件的业务证据重建幂等键并与旧 ID 核对，避免把未知事件或任意 ID 写入唯一键字段。
  function legacyKey(row) {
    const payload = row.evidence?.payload;
    let identity;
    switch (payload?.type) {
      case "invoice_task_reassigned":
        identity = [payload.type, payload.command_receipt_id, "", ""];
        break;
      case "invoice_approved":
        identity = [payload.type, payload.invoice_request_id, payload.approval_instance_id, ""];
        break;
      case "sales_order_approval_assigned":
      case "purchase_order_approval_assigned":
        identity = [payload.type, payload.approval_instance_id, payload.node_id, payload.assignment_id];
        break;
      default:
        throw new Error("发现未知类型事件，必须核验业务证据后再迁移");
    }
    const version = row.evidence.fact_version?.toString();
    if (identity.some(value => typeof value !== "string") ||
        typeof row.evidence.recipient !== "string" || !/^[1-9][0-9]*$/.test(version)) {
      throw new Error("历史事件证据无效，停止迁移");
    }
    const key = `[${JSON.stringify(identity)},${JSON.stringify(row.evidence.recipient)},${version}]`;
    if (row.id !== key) throw new Error("历史事件 ID 与业务幂等键不符，停止迁移");
  }
  events.find(missing).forEach(legacyKey);
  events.updateMany(missing, [{$set: {dedup_key: "$id"}}]);
  if (events.findOne({$or: [{dedup_key: {$not: {$type: "string"}}}, {dedup_key: ""}]})) {
    throw new Error("存在无效幂等键，禁止启用新版本");
  }
  events.createIndex({dedup_key: 1}, {name: "uk_resource_grant_events_dedup_key", unique: true});
})();
