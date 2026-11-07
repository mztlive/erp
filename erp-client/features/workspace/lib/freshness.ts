import type { DataFreshnessState } from "@/components/business/page"
import type { TodayWorkspaceView } from "@/features/workspace/types"
import { formatClock } from "@/lib/datetime"

export function deriveWorkItemsFreshness(
    freshness: TodayWorkspaceView["freshness"],
    options?: { refreshing?: boolean },
): {
    state: DataFreshnessState
    updatedAtLabel: string
    statusLabel: string
    dateTime: string
} {
    if (options?.refreshing) {
        return {
            state: "syncing",
            updatedAtLabel: "正在刷新",
            statusLabel: "正在同步",
            dateTime: freshness.workItemsUpdatedAt,
        }
    }

    return {
        state: "fresh",
        updatedAtLabel: formatClock(freshness.workItemsUpdatedAt),
        statusLabel: "待办已更新",
        dateTime: freshness.workItemsUpdatedAt,
    }
}
