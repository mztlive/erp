"use client"

import { useQuery } from "@tanstack/react-query"

import type { AccountProfile } from "@/features/auth/api"
import { fetchWorkspaceDashboard } from "@/features/workspace/api/dashboard"
import type { TodayWorkspaceQuery } from "@/features/workspace/types"

const workspaceHomeKeys = {
    all: ["workspace-home"] as const,
    dashboard: (query: TodayWorkspaceQuery, profile?: AccountProfile) =>
        [
            ...workspaceHomeKeys.all,
            "dashboard",
            query,
            profile?.userid,
            profile ? [...profile.role_ids].sort() : [],
            profile ? [...profile.permissions].sort() : [],
        ] as const,
}

export function useWorkspaceDashboardQuery(
    query: TodayWorkspaceQuery,
    profile?: AccountProfile,
) {
    return useQuery({
        queryKey: workspaceHomeKeys.dashboard(query, profile),
        queryFn: () => {
            if (!profile) throw new Error("当前账号资料未就绪")
            return fetchWorkspaceDashboard(query, profile)
        },
        enabled: Boolean(profile),
        // Keep previous filter results visible while the next filter loads (§6.2).
        placeholderData: (previous) => previous,
        // 60s 自动轮询，数据过期无需等用户手动刷新（P2-11）。
        refetchInterval: 60_000,
    })
}
