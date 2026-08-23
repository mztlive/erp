/** W13 卡券票款复核 API · 纯映射/格式化辅助函数（无 IO，可单测）。 */

import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueQuery,
} from "@/features/card-funds-review/types"

export function instantToIso(secs: number | undefined | null): string {
    if (secs == null || !Number.isFinite(Number(secs))) return ""
    return new Date(Number(secs) * 1000).toISOString()
}

export function mapPriority(p: string | number): number {
    if (typeof p === "number") return p
    switch (p) {
        case "urgent":
            return 100
        case "high":
            return 80
        case "low":
            return 20
        default:
            return 50
    }
}

export function mapReviewResultFrontend(r: string): "APPROVED" | "REJECTED" {
    return r === "passed" || r === "APPROVED" ? "APPROVED" : "REJECTED"
}

export function mapReviewTypeFrontend(
    t: string,
): CardFundsReviewItemView["reviewType"] {
    if (t === "sync_delta" || t === "SYNC_DELTA") return "SYNC_DELTA"
    return "OPENING"
}

export function filterSummary(q: CardFundsReviewQueueQuery): string {
    const parts = [
        q.scope === "mine" ? "仅我的" : "处理历史",
        q.type === "opening"
            ? "期初"
            : q.type === "delta"
              ? "同步差额"
              : "全部类型",
        q.status === "COMPLETED"
            ? "已完成"
            : q.status === "CLOSED"
              ? "已关闭"
              : "待处理有效队列",
        q.due === "overdue"
            ? "已超期"
            : q.due === "today"
              ? "今日到期"
              : "全部时限",
    ]
    if (q.q) parts.push(`搜索 ${q.q}`)
    return parts.join(" · ")
}
