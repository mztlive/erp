/**
 * W30 历史消费回填 · 真实 HTTP API
 * 路径：/admin/mall-consumption-backfill-jobs、/admin/mall-consumption-backfill-items
 *
 * 兼容入口：实现已按资源拆分到同目录子模块，公共导出保持不变。
 * - wire.ts     后端 wire 类型
 * - mapping.ts  后端字段 → 客户端契约映射
 * - jobs.ts     任务列表 / 详情查询
 * - commands.ts 命令提交
 */

export {
    fetchHistoryBackfillDetail,
    fetchHistoryBackfillList,
} from "@/features/history-backfill/api/jobs"
export { submitHistoryBackfillCommand } from "@/features/history-backfill/api/commands"
