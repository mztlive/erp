"use client"

import {
    useMutation,
    useQuery,
    useQueryClient,
    type QueryClient,
} from "@tanstack/react-query"

import {
    getWorkItem,
    getWorkItemStats,
    listBlockedApprovals,
    listWorkItems,
    parseWorkItemConflict,
    recoverApproval,
    submitWorkItemResponsibility,
    type WorkItemListParams,
    type WorkItemStatsParams,
} from "./api"
import type { WorkItemDto, WorkItemResponsibilityCommand } from "./types"

export const workItemKeys = {
    all: ["work-items"] as const,
    detail: (workItemId: string) =>
        [...workItemKeys.all, "detail", workItemId] as const,
    list: (params: WorkItemListParams) =>
        [...workItemKeys.all, "list", params] as const,
    stats: (params: WorkItemStatsParams) =>
        [...workItemKeys.all, "stats", params] as const,
    approvalBlockers: () => ["approval-instances", "blocked"] as const,
}

/** 冲突后保留可见最新摘要，并令列表、详情、统计与统一队列全部重新验证。 */
export const synchronizeWorkItemConflict = async (
    queryClient: QueryClient,
    command: WorkItemResponsibilityCommand,
    error: unknown,
): Promise<void> => {
    const conflict = parseWorkItemConflict(error)
    const isHttpConflict =
        typeof error === "object" &&
        error !== null &&
        "status" in error &&
        error.status === 409
    if (!conflict) {
        if (isHttpConflict) {
            await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
        }
        return
    }
    const detailKey = workItemKeys.detail(command.workItemId)
    if (conflict.currentWorkItem?.id === command.workItemId) {
        queryClient.setQueryData<WorkItemDto | null>(
            detailKey,
            conflict.currentWorkItem,
        )
    } else {
        queryClient.setQueryData<WorkItemDto | null>(detailKey, null)
    }
    await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
}

export function useWorkItemsQuery(params: WorkItemListParams) {
    return useQuery({
        queryKey: workItemKeys.list(params),
        queryFn: () => listWorkItems(params),
    })
}

/** 查询单条权限安全任务；冲突摘要会写入同一缓存键。 */
export const useWorkItemDetailQuery = (workItemId: string) =>
    useQuery<WorkItemDto | null>({
        queryKey: workItemKeys.detail(workItemId),
        queryFn: () => getWorkItem(workItemId),
        enabled: workItemId.trim().length > 0,
    })

/** 查询与队列使用同一授权边界的任务统计。 */
export const useWorkItemStatsQuery = (params: WorkItemStatsParams) =>
    useQuery({
        queryKey: workItemKeys.stats(params),
        queryFn: () => getWorkItemStats(params),
    })

export function useBlockedApprovalsQuery(enabled = true) {
    return useQuery({
        queryKey: workItemKeys.approvalBlockers(),
        queryFn: listBlockedApprovals,
        enabled,
    })
}

export function useWorkItemResponsibilityMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitWorkItemResponsibility,
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: workItemKeys.all })
        },
        onError: async (error, command) => {
            await synchronizeWorkItemConflict(queryClient, command, error)
        },
    })
}

export function useRecoverApprovalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: recoverApproval,
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.approvalBlockers(),
                }),
                queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
            ])
        },
    })
}
