"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    claimMappingWorkItem,
    confirmMapping,
    deferMapping,
    fetchMallSyncPage,
    fetchSourceSystems,
    reapplyMallSnapshot,
    resolveUnknownReapply,
    retryFailedJob,
    triggerManualIncremental,
    triggerSingleOrderPull,
    type MallSyncQueryInput,
} from "@/features/mall-sync/api/index"
import type { SourceSystemListParams } from "@/features/mall-sync/types"
import { isAuthenticated } from "@/lib/api"

const mallSyncKeys = {
    all: ["mall-sync"] as const,
    page: (input: MallSyncQueryInput) =>
        [...mallSyncKeys.all, "page", input] as const,
    sourceSystems: (params: SourceSystemListParams) =>
        [...mallSyncKeys.all, "source-systems", params] as const,
}

export function useMallSyncPageQuery(input: MallSyncQueryInput) {
    return useQuery({
        queryKey: mallSyncKeys.page(input),
        queryFn: () => fetchMallSyncPage(input),
        refetchInterval: (q) => {
            const hasRunning = q.state.data?.jobs.some(
                (j) => j.status === "RUNNING",
            )
            return hasRunning ? 4_000 : false
        },
    })
}

/** 来源系统分页默认参数：第一页 20 条（页面为汇总卡片无分页控件）。 */
const SOURCE_SYSTEMS_DEFAULT_PARAMS: SourceSystemListParams = {
    page: 1,
    page_size: 20,
}

/**
 * 来源系统列表查询（真实 HTTP，需已登录）。
 */
export function useSourceSystemsQuery(
    params: SourceSystemListParams = SOURCE_SYSTEMS_DEFAULT_PARAMS,
) {
    return useQuery({
        queryKey: mallSyncKeys.sourceSystems(params),
        queryFn: () => fetchSourceSystems(params),
        enabled: isAuthenticated(),
    })
}

export function useTriggerIncrementalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: triggerManualIncremental,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}

export function useTriggerSingleOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: triggerSingleOrderPull,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}

export function useRetryJobMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: retryFailedJob,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}

export function useClaimMappingMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: claimMappingWorkItem,
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
        },
    })
}

export function useConfirmMappingMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: confirmMapping,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}

export function useDeferMappingMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: deferMapping,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}

export function useReapplyMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reapplyMallSnapshot,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}

export function useResolveUnknownReapplyMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: resolveUnknownReapply,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: mallSyncKeys.all,
                })
            }
        },
    })
}
