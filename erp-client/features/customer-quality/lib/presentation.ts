import type { DataFreshnessState } from "@/components/business/page"

export function formatClock(iso: string) {
    try {
        return new Intl.DateTimeFormat("zh-CN", {
            hour: "2-digit",
            minute: "2-digit",
            hour12: false,
        }).format(new Date(iso))
    } catch {
        return iso
    }
}

/** 来源更新位置形如 outbox:cq:2026-08-01T09:35:48+08:00，提取可读时间部分。 */
export function formatSourceWatermark(w: string): string {
    const m = w.match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
    return m ? m[0] : w
}

export function freshnessPresentation(
    state: "fresh" | "stale" | "rebuilding" | "failed",
    refreshFailed?: boolean,
    refreshing?: boolean,
): { state: DataFreshnessState; statusLabel: string } {
    if (refreshing) return { state: "syncing", statusLabel: "正在刷新" }
    if (refreshFailed)
        return { state: "failed", statusLabel: "刷新失败（保留旧数据）" }
    if (state === "failed")
        return { state: "failed", statusLabel: "数据加载失败" }
    if (state === "rebuilding")
        return { state: "syncing", statusLabel: "正在重建" }
    if (state === "stale")
        return { state: "stale", statusLabel: "数据可能不是最新" }
    return { state: "fresh", statusLabel: "数据已更新" }
}

export function metricReliabilityDetail(
    reliability: string,
    explanation?: string,
    fieldDenied?: boolean,
) {
    if (fieldDenied) return "当前角色不可查看"
    if (reliability === "partial") return explanation ?? "部分可靠"
    if (reliability === "unavailable") return explanation ?? "暂无可靠口径"
    return explanation
}
