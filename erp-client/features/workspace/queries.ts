"use client"

import { useQuery } from "@tanstack/react-query"

import { fetchWorkspaceDashboard } from "@/features/workspace/api"
import type { TodayWorkspaceQuery } from "@/features/workspace/types"

const workspaceHomeKeys = {
    all: ["workspace-home"] as const,
    dashboard: (query: TodayWorkspaceQuery) =>
        [...workspaceHomeKeys.all, "dashboard", query] as const,
}

export function useWorkspaceDashboardQuery(query: TodayWorkspaceQuery) {
    return useQuery({
        queryKey: workspaceHomeKeys.dashboard(query),
        queryFn: () => fetchWorkspaceDashboard(query),
        // Keep previous filter results visible while the next filter loads (§6.2).
        placeholderData: (previous) => previous,
        // 60s 自动轮询，数据过期无需等用户手动刷新（P2-11）。
        refetchInterval: 60_000,
    })
}
