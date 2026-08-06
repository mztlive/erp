import type { DataFreshnessState } from "@/components/business/page"
import type { TodayWorkspaceView } from "@/features/workspace/types"

const STALE_AFTER_MS = 60_000

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

export function deriveProjectionFreshness(
  freshness: TodayWorkspaceView["freshness"],
  options?: { refreshing?: boolean }
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
      dateTime: freshness.projectionUpdatedAt,
    }
  }

  if (freshness.projectionState === "failed") {
    return {
      state: "failed",
      updatedAtLabel: formatClock(freshness.projectionUpdatedAt),
      statusLabel: "数据更新失败",
      dateTime: freshness.projectionUpdatedAt,
    }
  }

  if (freshness.projectionState === "rebuilding") {
    return {
      state: "syncing",
      updatedAtLabel: formatClock(freshness.projectionUpdatedAt),
      statusLabel: "数据更新中",
      dateTime: freshness.projectionUpdatedAt,
    }
  }

  const ageMs = Date.now() - new Date(freshness.projectionUpdatedAt).getTime()
  const isStale =
    freshness.projectionState === "stale" ||
    (!Number.isNaN(ageMs) && ageMs > STALE_AFTER_MS)

  if (isStale) {
    return {
      state: "stale",
      updatedAtLabel: formatClock(freshness.projectionUpdatedAt),
      statusLabel: "数据可能不是最新（>1 分钟）",
      dateTime: freshness.projectionUpdatedAt,
    }
  }

  return {
    state: "fresh",
    updatedAtLabel: formatClock(freshness.projectionUpdatedAt),
    statusLabel: "数据已更新",
    dateTime: freshness.projectionUpdatedAt,
  }
}

export function deriveWorkItemsFreshness(
  freshness: TodayWorkspaceView["freshness"],
  options?: { refreshing?: boolean }
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

export function greetingForNow(displayName: string, now = new Date()): string {
  const hour = now.getHours()
  const salute =
    hour < 12 ? "早上好" : hour < 18 ? "下午好" : "晚上好"
  return `${salute}，${displayName}`
}
