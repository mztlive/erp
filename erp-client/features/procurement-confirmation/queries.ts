"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { unifiedQueueKeys } from "@/features/unified-task-queue/queries"
import {
  claimProcurementWorkItem,
  completeProcurementDecision,
  deferProcurementConfirmation,
  fetchProcurementQueue,
  fetchProcurementRecommendation,
  fetchProcurementSupplyOptions,
  saveProcurementConfirmation,
  type QueueFilters,
} from "@/features/procurement-confirmation/api"

const procurementConfirmKeys = {
  all: ["procurement-confirmation"] as const,
  queue: (filters: QueueFilters) =>
    [...procurementConfirmKeys.all, "queue", filters] as const,
  supplyOptions: (skuIds: readonly string[]) =>
    [...procurementConfirmKeys.all, "supply-options", [...skuIds].sort()] as const,
  recommendation: (confirmationId: string) =>
    [...procurementConfirmKeys.all, "recommendation", confirmationId] as const,
}

/** 当前采购确认的后端最低可执行成本方案。 */
export function useProcurementRecommendationQuery(
  confirmationId: string,
  enabled = true
) {
  return useQuery({
    queryKey: procurementConfirmKeys.recommendation(confirmationId),
    queryFn: () => fetchProcurementRecommendation(confirmationId),
    enabled: enabled && confirmationId.length > 0,
  })
}

/** 当前销售提交行可用的供给修订与能力修订。 */
export function useProcurementSupplyOptionsQuery(
  skuIds: readonly string[]
) {
  return useQuery({
    queryKey: procurementConfirmKeys.supplyOptions(skuIds),
    queryFn: () => fetchProcurementSupplyOptions(skuIds),
    enabled: skuIds.some(Boolean),
  })
}

export function useProcurementConfirmationQuery(filters: QueueFilters) {
  return useQuery({
    queryKey: procurementConfirmKeys.queue(filters),
    queryFn: () => fetchProcurementQueue(filters),
  })
}

export function useClaimProcurementMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: claimProcurementWorkItem,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: procurementConfirmKeys.all,
      })
    },
  })
}

export function useSaveProcurementConfirmationMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveProcurementConfirmation,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: procurementConfirmKeys.all,
      })
    },
  })
}

export function useCompleteProcurementMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: completeProcurementDecision,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: procurementConfirmKeys.all,
        })
        // 同一终态存储也作用于 W02 采购确认族：同步角标与队列视图
        await queryClient.invalidateQueries({
          queryKey: unifiedQueueKeys.all,
        })
      }
    },
  })
}

export function useDeferProcurementMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deferProcurementConfirmation,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: procurementConfirmKeys.all,
        })
        await queryClient.invalidateQueries({
          queryKey: unifiedQueueKeys.all,
        })
      }
    },
  })
}
