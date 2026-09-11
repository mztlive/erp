// 执行：mongosh "$ERP_MONGO_URI" --file scripts/migrations/sales-selection-rate-windows.js
// 先停止旧版 Web API/worker，再执行本脚本，最后启动新版。只处理临时分钟计数。
// 旧窗口保留两分钟后由 TTL 回收；脚本可重复执行，不更改已有过期时间。
db.sales_selection_rate_windows.updateMany(
    { expires_at: { $exists: false } },
    { $set: { expires_at: new Date(Date.now() + 120000) } },
)
db.sales_selection_rate_windows.createIndex(
    { expires_at: 1 },
    { name: "ttl_sales_selection_rate_windows", expireAfterSeconds: 0 },
)
// 回滚：停止新版进程后删除 ttl_sales_selection_rate_windows 索引即可停用自动回收。
// 已过期计数不恢复；不影响选品册、选择、方案及幂等记录。
