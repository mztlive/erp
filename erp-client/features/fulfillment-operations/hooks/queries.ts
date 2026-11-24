"use client"

import { useMemo } from "react"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import type { FulfillmentOperation } from "../types"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { getDeliveryPackingDetail } from "../api/packing"
import { queryKeyRoots } from "@/lib/query-key-roots"

import {
    fetchFulfillmentQueue,
    postFulfillmentOperation,
    resolveUnknownFulfillmentResult,
    saveFulfillmentOperation,
    type FulfillmentQueueFilters,
} from "@/features/fulfillment-operations/api"

export const fulfillmentKeys = {
    all: queryKeyRoots.fulfillmentOperations,
    packing: (id: string) => [...fulfillmentKeys.all, "packing", id] as const,
    queue: (filters: FulfillmentQueueFilters) =>
        [...fulfillmentKeys.all, "queue", filters] as const,
}

export function useFulfillmentQueueQuery(filters: FulfillmentQueueFilters) {
    const result = useQuery({
        queryKey: fulfillmentKeys.queue(filters),
        queryFn: () => fetchFulfillmentQueue(filters),
    })
    const view = result.data
    const skuIds = view?.operations.flatMap((operation) =>
        operation.lines.map((line) => line.skuId),
    )
    const { nameForUnit } = useProductUnitNames(
        skuIds,
        Boolean(view?.operations.length),
    )
    const data = useMemo(() => {
        if (!view) return view
        const withUnits = (operation: FulfillmentOperation) => ({
            ...operation,
            lines: operation.lines.map((line) => ({
                ...line,
                unitName:
                    line.unitName ?? nameForUnit(line.unitCode, line.skuId),
            })),
        })
        return {
            ...view,
            operations: view.operations.map(withUnits),
            current: view.current ? withUnits(view.current) : view.current,
        }
    }, [view, nameForUnit])
    return { ...result, data }
}

/**
 * 保存入库/发货草稿。PurchaseReceipt、Delivery、ElectronicDelivery、
 * ServiceFulfillment 与 CustomerAcceptance 均为 NO_APPROVAL，不触达审批缓存。
 */
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

/**
 * 确认正式单据。PurchaseReceipt、Delivery、ElectronicDelivery、
 * ServiceFulfillment 与 CustomerAcceptance 均为 NO_APPROVAL，
 * 成功后只刷新履约队列，不触达审批键。
 */
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

/** 复用履约资源键，保存和确认后自动失效地址包裹详情。 */
export function useDeliveryPackingQuery(id: string, enabled: boolean) {
    return useQuery({
        queryKey: fulfillmentKeys.packing(id),
        queryFn: () => getDeliveryPackingDetail(id),
        enabled,
    })
}
