"use client"

import { useQuery } from "@tanstack/react-query"

import type { AccountProfile } from "@/features/auth/api"
import { getWorkItem } from "@/features/work-items/api"
import { mapWorkspaceWorkItem } from "../api/dashboard"
import { workItemKeys } from "@/features/work-items/queries"
import {
    fetchWorkspaceDashboard,
    fetchWorkspaceInboxCount,
} from "@/features/workspace/api/dashboard"
import type { TodayWorkspaceQuery } from "@/features/workspace/types"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { fetchWorkspaceApprovalDetail } from "../api/approval-detail"

const workspaceHomeKeys = {
    all: queryKeyRoots.workspaceHome,
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
            profile?.permissions,
            profile?.role_ids,
        ],
        queryFn: () => {
            if (!instanceId) throw new Error("审批信息暂不可用")
            return fetchWorkspaceApprovalDetail(instanceId)
        },
        enabled: Boolean(instanceId && profile),
        retry: false,
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
            profile ? [...profile.permissions].sort() : [],
            profile ? [...profile.role_ids].sort() : [],
        ],
        queryFn: async () => {
            if (!workItemId || !profile) throw new Error("任务信息暂不可用")
            return mapWorkspaceWorkItem(
                await getWorkItem(workItemId),
                "Asia/Shanghai",
            )
        },
        enabled: enabled && Boolean(workItemId && profile),
        retry: false,
    })
}

/**
 * 顶栏待办角标。数量来自服务端统计，不对列表求和。
 */
export function useWorkspaceInboxCountQuery(enabled = true) {
    return useQuery({
        queryKey: workItemKeys.inboxCount(),
        queryFn: fetchWorkspaceInboxCount,
        enabled,
    })
}

export { workspaceHomeKeys }
