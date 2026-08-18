"use client"

import { useQuery } from "@tanstack/react-query"

import type { AccountProfile } from "@/features/auth/api"
import { workItemKeys } from "@/features/work-items/queries"
import {
    fetchWorkspaceDashboard,
    fetchWorkspaceInboxCount,
} from "@/features/workspace/api/dashboard"
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

/**
 * 工作台主查询。筛选切换保留上一页数据，避免整页闪烁。
 */
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
        placeholderData: (previous) => previous,
        refetchInterval: 60_000,
    })
}

/**
 * 顶栏待办角标。数量来自服务端统计，不对列表求和。
 */
export function useWorkspaceInboxCountQuery() {
    return useQuery({
        queryKey: workItemKeys.inboxCount(),
        queryFn: fetchWorkspaceInboxCount,
    })
}

export { workspaceHomeKeys }
