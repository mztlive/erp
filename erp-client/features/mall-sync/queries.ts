"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  assignMappingTask,
  claimMappingWorkItem,
  confirmMapping,
  deferMapping,
  fetchMallSyncPage,
  fetchSourceSystems,
  reapplyMallSnapshot,
  resolveUnknownReapply,
  retryFailedJob,
  setMallSyncDemoStage,
  setMallSyncPolicyConfigured,
  setMallSyncSourceUnavailable,
  triggerManualIncremental,
  triggerSingleOrderPull,
  type MallSyncQueryInput,
} from "@/features/mall-sync/api"
import type {
  OwnershipStage,
  SourceSystemListParams,
} from "@/features/mall-sync/types"
import { isAuthenticated, isFeatureReal } from "@/lib/api"

export const mallSyncKeys = {
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
    // 运行中任务自动刷新进度
    refetchInterval: (q) => {
      const hasRunning = q.state.data?.jobs.some(
        (j) => j.status === "RUNNING"
      )
      return hasRunning ? 4_000 : false
    },
  })
}

/** 来源系统分页默认参数：第一页 20 条（真实接口，页面为汇总卡片无分页控件）。 */
const SOURCE_SYSTEMS_DEFAULT_PARAMS: SourceSystemListParams = {
  page: 1,
  page_size: 20,
}

/**
 * 来源系统列表查询（P0-5 垂直样板：真实 useQuery 取数）。
 *
 * 数据源开关：仅当 lib/api feature-source 的 isFeatureReal("mall-sync") 为真
 * 且 session 已有 token 时启用真实请求（enabled 控制，开关关闭时本查询不发请求，
 * 页面继续走 mock 数据路径，无回归）。
 * 无 token 时不发请求，由页面给出「未能获取来源数据」的错误提示。
 */
export function useSourceSystemsQuery(
  params: SourceSystemListParams = SOURCE_SYSTEMS_DEFAULT_PARAMS
) {
  return useQuery({
    queryKey: mallSyncKeys.sourceSystems(params),
    queryFn: () => fetchSourceSystems(params),
    enabled: isFeatureReal("mall-sync") && isAuthenticated(),
  })
}

export function useTriggerIncrementalMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: triggerManualIncremental,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
      }
    },
  })
}

export function useAssignMappingMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: assignMappingTask,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
      }
    },
  })
}

/** 演示控制：切换主责阶段 / 策略 / 来源可用性后失效缓存 */
export function useMallSyncDemoControls() {
  const queryClient = useQueryClient()
  return {
    setStage: async (stage: OwnershipStage) => {
      setMallSyncDemoStage(stage)
      await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
    },
    setPolicy: async (configured: boolean) => {
      setMallSyncPolicyConfigured(configured)
      await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
    },
    setSourceUnavailable: async (unavailable: boolean) => {
      setMallSyncSourceUnavailable(unavailable)
      await queryClient.invalidateQueries({ queryKey: mallSyncKeys.all })
    },
  }
}
