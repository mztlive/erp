"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  assignMappingTask,
  claimMappingWorkItem,
  confirmMapping,
  deferMapping,
  fetchMallSyncPage,
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
import type { OwnershipStage } from "@/features/mall-sync/types"

export const mallSyncKeys = {
  all: ["mall-sync"] as const,
  page: (input: MallSyncQueryInput) =>
    [...mallSyncKeys.all, "page", input] as const,
}

export function useMallSyncPageQuery(input: MallSyncQueryInput) {
  return useQuery({
    queryKey: mallSyncKeys.page(input),
    queryFn: () => fetchMallSyncPage(input),
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
