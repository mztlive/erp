// 停止 Worker 后，在已核对环境的业务库执行；只调整调度回执索引，不修改业务任务。
// 回退：停调度并核实 pending callback 后保留集合与索引；禁止删除未回报证据。
db.scheduled_runs.createIndex({ id: 1 }, { name: "uk_scheduled_runs_id", unique: true });
db.scheduled_runs.createIndex({ app: 1, job_id: 1 }, {
  name: "uk_scheduled_runs_active_job", unique: true,
  partialFilterExpression: { state: { $in: ["accepted", "running"] } }
});
db.scheduled_runs.createIndex({ app: 1, state: 1, created_at: 1, id: 1 }, { name: "idx_scheduled_runs_claim" });
db.scheduled_runs.createIndex({ app: 1, state: 1, callback_done: 1, callback_after: 1, id: 1 }, { name: "idx_scheduled_runs_callback" });
db.scheduled_runs.createIndex({ expires_at: 1 }, { name: "ttl_scheduled_runs", expireAfterSeconds: 0 });
print("scheduled_runs indexes ready; no business tasks changed");
