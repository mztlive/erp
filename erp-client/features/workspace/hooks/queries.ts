"use client"

import { useQuery } from "@tanstack/react-query"

import { getStartedApprovalUnreadCount } from "@/features/approval-workflow/api"

import { useNotificationFeed } from "@/features/notifications/hooks/queries"
import { hasPermission } from "@/lib/permissions"
import type { AccountProfile } from "@/features/auth/api"
import { fetchWorkspaceDashboard } from "@/features/workspace/api/dashboard"
import type { TodayWorkspaceQuery } from "@/features/workspace/types"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    fetchWorkspaceApprovalDetail,
    fetchWorkspaceTaskDetail,
} from "../api/approval-detail"

const workspaceHomeKeys = {
    all: queryKeyRoots.workspaceHome,
    dashboard: (query: TodayWorkspaceQuery, profile?: AccountProfile) =>
        [
            ...workspaceHomeKeys.all,
            "dashboard",
            query,
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
            profile ? [...profile.role_ids].sort() : [],
            profile ? [...profile.permissions].sort() : [],
        ] as const,
}

/**
 * 工作台主查询。切换范围或分类后不得沿用旧队列，避免打开上一分类的任务。
 */
export function useWorkspaceDashboardQuery(
    query: TodayWorkspaceQuery,
    profile?: AccountProfile,
    enabled = true,
) {
    return useQuery({
        queryKey: workspaceHomeKeys.dashboard(query, profile),
        queryFn: () => {
            if (!profile) throw new Error("账号信息加载中，请稍后重试")
            return fetchWorkspaceDashboard(query, profile)
        },
        enabled: enabled && Boolean(profile),
        refetchInterval: 60_000,
    })
}

/** 审批详情不依赖队列页、筛选条件或列表缓存。 */
export function useWorkspaceApprovalDetailQuery(
    instanceId: string | undefined,
    profile?: AccountProfile,
) {
    return useQuery({
        queryKey: [
            ...workspaceHomeKeys.all,
            "approval-detail",
            instanceId,
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
            profile?.permissions,
            profile?.role_ids,
        ],
        queryFn: () => {
            if (!instanceId) throw new Error("审批信息暂不可用")
            return fetchWorkspaceApprovalDetail(instanceId, {
                canReadRework: hasPermission(
                    profile?.permissions,
                    "work_item:read",
                ),
            })
        },
        enabled: Boolean(instanceId && profile),
        retry: false,
        staleTime: 0,
        refetchOnMount: "always",
        meta: { roleWorkbench: true },
    })
}

/** 按路由 ID 读取任务详情，缓存按当前账号与权限隔离。 */
export function useWorkspaceTaskDetailQuery(
    workItemId: string | undefined,
    profile?: AccountProfile,
    enabled = true,
) {
    return useQuery({
        queryKey: [
            ...workspaceHomeKeys.all,
            "task-detail",
            workItemId,
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
            profile ? [...profile.permissions].sort() : [],
            profile ? [...profile.role_ids].sort() : [],
        ],
        queryFn: async () => {
            if (!workItemId || !profile) throw new Error("任务信息暂不可用")
            return fetchWorkspaceTaskDetail(workItemId)
        },
        enabled: enabled && Boolean(workItemId && profile),
        retry: false,
        staleTime: 0,
        refetchOnMount: "always",
        meta: { roleWorkbench: true },
    })
}

export { workspaceHomeKeys }

/** 菜单按账号隔离：待我处理取待办数，我发起的审批只取未读进展数。 */
export function useWorkspaceNavigationCounts(profile?: AccountProfile) {
    const feed = useNotificationFeed(profile)
    const started = useQuery({
        queryKey: [
            ...workspaceHomeKeys.all,
            "navigation-count",
            "started-unread",
            profile?.userid,
            profile?.policy_version,
            profile?.organization_version,
            profile?.role_ids,
            profile?.permissions,
        ],
        queryFn: getStartedApprovalUnreadCount,
        enabled:
            Boolean(profile) &&
            hasPermission(profile?.permissions, "approval_instance:read"),
        meta: { workspaceNavigation: true },
    })
    return {
        inbox: {
            data: feed.data?.todo_count ?? undefined,
            isError: feed.isError,
            error: feed.error,
        },
        started,
    }
}
