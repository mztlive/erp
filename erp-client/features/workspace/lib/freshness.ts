import type { DataFreshnessState } from "@/components/business/page"
import type { TodayWorkspaceView } from "@/features/workspace/types"

function formatClock(iso: string): string {
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
