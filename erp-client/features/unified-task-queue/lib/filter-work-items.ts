import { FAMILY_LABELS } from "../types"
import type { UnifiedQueueFilters } from "../types"

/** 服务端负责筛选与排序；这里只生成当前筛选的可读摘要。 */
export function buildFilterSummary(
    filters: UnifiedQueueFilters,
    total: number,
): string {
    const parts = [
        {
            mine: "我的待办",
            team: "团队待处理",
            managed: "团队任务",
            history: "处理历史",
        }[filters.scope],
    ]
    if (filters.family) parts.push(FAMILY_LABELS[filters.family])
    if (filters.workItemType) parts.push("已限定任务类型")
    if (filters.due === "overdue") parts.push("已超期")
    if (filters.due === "today") parts.push("今日到期")
    if (filters.priorities?.length) parts.push("已限定优先级")
    if (filters.historyStatus === "COMPLETED") parts.push("已完成")
    if (filters.historyStatus === "CLOSED") parts.push("已关闭")
    if (filters.query?.trim()) parts.push(`搜索“${filters.query.trim()}”`)
    parts.push(`共 ${total.toLocaleString("zh-CN")} 项`)
    return parts.join(" · ")
}
