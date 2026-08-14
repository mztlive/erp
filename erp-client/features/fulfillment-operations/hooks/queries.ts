"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchFulfillmentQueue,
    postFulfillmentOperation,
    resolveUnknownFulfillmentResult,
    saveFulfillmentOperation,
    type FulfillmentQueueFilters,
} from "@/features/fulfillment-operations/api"
import type { FulfillmentLane } from "@/features/fulfillment-operations/lib/lanes"

export const fulfillmentKeys = {
    all: ["fulfillment-operations"] as const,
    queue: (filters: FulfillmentQueueFilters) =>
        [...fulfillmentKeys.all, "queue", filters] as const,
    counts: (lane: FulfillmentLane) =>
        [...fulfillmentKeys.all, "counts", lane] as const,
}

export function useFulfillmentQueueQuery(filters: FulfillmentQueueFilters) {
    return useQuery({
        queryKey: fulfillmentKeys.queue(filters),
        queryFn: () => fetchFulfillmentQueue(filters),
    })
}

/** 角标计数：当前岗位可处理的草稿单据数。 */
export function useFulfillmentCountQuery(lane: FulfillmentLane) {
    return useQuery({
        queryKey: fulfillmentKeys.counts(lane),
        queryFn: async () => {
            const view = await fetchFulfillmentQueue({
                role: lane,
            })
            return { pending: view.context.total }
        },
    })
}

export function useSaveFulfillmentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveFulfillmentOperation,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: fulfillmentKeys.all,
            })
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
                await queryClient.invalidateQueries({
                    queryKey: fulfillmentKeys.all,
                })
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
                await queryClient.invalidateQueries({
                    queryKey: fulfillmentKeys.all,
                })
            }
        },
    })
}
