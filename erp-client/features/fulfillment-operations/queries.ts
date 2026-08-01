"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  claimFulfillmentWorkItem,
  deferFulfillmentOperation,
  fetchFulfillmentQueue,
  postFulfillmentOperation,
  renewFulfillmentLease,
  resolveUnknownFulfillmentResult,
  saveFulfillmentOperation,
  type FulfillmentQueueFilters,
} from "@/features/fulfillment-operations/api"

export const fulfillmentKeys = {
  all: ["fulfillment-operations"] as const,
  queue: (filters: FulfillmentQueueFilters) =>
    [...fulfillmentKeys.all, "queue", filters] as const,
}

export function useFulfillmentQueueQuery(filters: FulfillmentQueueFilters) {
  return useQuery({
    queryKey: fulfillmentKeys.queue(filters),
    queryFn: () => fetchFulfillmentQueue(filters),
  })
}

export function useClaimFulfillmentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: claimFulfillmentWorkItem,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
    },
  })
}

export function useRenewFulfillmentLeaseMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: renewFulfillmentLease,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
    },
  })
}

export function useSaveFulfillmentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveFulfillmentOperation,
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
    },
  })
}

export function usePostFulfillmentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: postFulfillmentOperation,
    onSuccess: async (result) => {
      // 仅在明确成功后失效；unknown 时不触碰缓存中的库存/队列假设
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
      }
    },
  })
}

export function useDeferFulfillmentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deferFulfillmentOperation,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
      }
    },
  })
}

export function useResolveUnknownFulfillmentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: resolveUnknownFulfillmentResult,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: fulfillmentKeys.all })
      }
    },
  })
}
