"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    applyWorkItemAction,
    batchClaimWorkItems,
    claimWorkItem,
    closeWorkItem,
    completeWorkItem,
    fetchUnifiedTaskQueue,
    fetchUnifiedTaskQueueCounts,
    transferWorkItem,
    WorkItemApiError,
} from "@/features/unified-task-queue/api"
import type {
    InTaskActionKind,
    SessionLease,
    UnifiedQueueFilters,
} from "@/features/unified-task-queue/types"

export const unifiedQueueKeys = {
    all: ["unified-task-queue"] as const,
    view: (filters: UnifiedQueueFilters) =>
        [...unifiedQueueKeys.all, "view", filters] as const,
    counts: () => [...unifiedQueueKeys.all, "counts"] as const,
    permission: () => [...unifiedQueueKeys.all, "permission"] as const,
}

export function useUnifiedTaskQueueQuery(filters: UnifiedQueueFilters) {
    return useQuery({
        queryKey: unifiedQueueKeys.view(filters),
        queryFn: () => fetchUnifiedTaskQueue(filters),
    })
}

export function useUnifiedTaskCountQuery() {
    return useQuery({
        queryKey: unifiedQueueKeys.counts(),
        queryFn: fetchUnifiedTaskQueueCounts,
    })
}

export function useClaimWorkItemMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (input: {
            workItemId: string
            subjectVersion?: string
        }): Promise<SessionLease> => claimWorkItem(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: unifiedQueueKeys.all,
            })
        },
    })
}

export function useBatchClaimWorkItemMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (input: {
            workItemIds: readonly string[]
            subjectVersions?: Readonly<Record<string, string>>
        }): Promise<SessionLease[]> => batchClaimWorkItems(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: unifiedQueueKeys.all,
            })
        },
    })
}

export function useWorkItemActionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (input: {
            workItemId: string
            expectedSubjectVersion?: string
            ownerUserId?: string
            action: { kind: InTaskActionKind; note?: string }
        }) => applyWorkItemAction(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: unifiedQueueKeys.all,
            })
        },
    })
}

export function useCompleteWorkItemMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (input: {
            workItemId: string
            expectedSubjectVersion?: string
            ownerUserId?: string
            decision: { kind: string; note?: string; summary?: string }
        }) => completeWorkItem(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: unifiedQueueKeys.all,
            })
        },
    })
}

export function useCloseWorkItemMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (input: {
            workItemId: string
            expectedSubjectVersion?: string
            ownerUserId?: string
            closeAllowed: boolean
            closure: {
                kind:
                    | "CLOSE_DUPLICATE"
                    | "CLOSE_MISROUTED"
                    | "CLOSE_WITH_REPLACEMENT"
                reasonCode: string
                replacementWorkItemId?: string
                comment?: string
            }
        }) => closeWorkItem(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: unifiedQueueKeys.all,
            })
        },
    })
}

export function useTransferWorkItemMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: async (input: {
            workItemId: string
            expectedSubjectVersion?: string
            transfer: { targetUserId: string; reason: string }
        }) => transferWorkItem(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: unifiedQueueKeys.all,
            })
        },
    })
}

export { WorkItemApiError }
