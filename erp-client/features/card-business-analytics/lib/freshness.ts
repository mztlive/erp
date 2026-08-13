import type { DataFreshnessState } from "@/components/business/page"

import type { ProjectionFreshnessState } from "../types"

export function mapFreshnessUi(
    state: ProjectionFreshnessState,
    options?: {
        refreshFailed?: boolean
        refreshing?: boolean
        breached?: boolean
    },
): { uiState: DataFreshnessState; statusLabel: string } {
    if (options?.refreshing) {
        return { uiState: "syncing", statusLabel: "正在刷新数据" }
    }
    if (options?.refreshFailed) {
        return { uiState: "failed", statusLabel: "刷新失败 · 保留旧数据" }
    }
    if (options?.breached || state === "stale") {
        return {
            uiState: "stale",
            statusLabel: "SLA 超时 · 数据陈旧 · 非实时",
        }
    }
    switch (state) {
        case "rebuilding":
            return { uiState: "syncing", statusLabel: "数据更新中" }
        case "failed":
            return { uiState: "failed", statusLabel: "数据更新失败" }
        default:
            return { uiState: "fresh", statusLabel: "数据已更新" }
    }
}
