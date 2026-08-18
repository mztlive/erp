"use client"

import {
    useMutation,
    useQuery,
    useQueryClient,
    type QueryClient,
} from "@tanstack/react-query"

import { workItemKeys } from "@/features/work-items/queries"
import {
    completeProcurementDecision,
    fetchProcurementQueue,
    fetchProcurementRecommendation,
    fetchProcurementSupplyOptions,
    isServerIssuedQueueContextId,
    saveProcurementConfirmation,
    type QueueFilters,
} from "@/features/procurement-confirmation/api"
import type { ProcurementQueueView } from "@/features/procurement-confirmation/types"

/** 列表查询键：不含 URL/W02 的 queueContextId，避免把外页哈希当成 W07 查询条件。 */
export function procurementListQueryKey(filters: QueueFilters) {
    return {
        scope: filters.scope,
        due: filters.due,
        sort: filters.sort,
        orderNo: filters.orderNo,
        currentWorkItemId: filters.currentWorkItemId,
    }
}

const procurementConfirmKeys = {
    all: ["procurement-confirmation"] as const,
    queue: (filters: QueueFilters) =>
        [
            ...procurementConfirmKeys.all,
            "queue",
            procurementListQueryKey(filters),
        ] as const,
    supplyOptions: (skuIds: readonly string[]) =>
        [
            ...procurementConfirmKeys.all,
            "supply-options",
            [...skuIds].sort(),
        ] as const,
    recommendation: (confirmationId: string) =>
        [
            ...procurementConfirmKeys.all,
            "recommendation",
            confirmationId,
        ] as const,
}

/** 当前采购确认的后端最低可执行成本方案。 */
export function useProcurementRecommendationQuery(
    confirmationId: string,
    enabled = true,
) {
    return useQuery({
        queryKey: procurementConfirmKeys.recommendation(confirmationId),
        queryFn: () => fetchProcurementRecommendation(confirmationId),
        enabled: enabled && confirmationId.length > 0,
    })
}

/** 当前销售提交行可用的供给修订与能力修订。 */
export function useProcurementSupplyOptionsQuery(skuIds: readonly string[]) {
    return useQuery({
        queryKey: procurementConfirmKeys.supplyOptions(skuIds),
        queryFn: () => fetchProcurementSupplyOptions(skuIds),
        enabled: skuIds.some(Boolean),
    })
}

function sameListIdentity(left: QueueFilters, right: QueueFilters): boolean {
    return (
        left.scope === right.scope &&
        (left.due ?? "active") === (right.due ?? "active") &&
        (left.sort ?? "due_at") === (right.sort ?? "due_at") &&
        (left.orderNo ?? "") === (right.orderNo ?? "")
    )
}

/** 从同一组 W07 筛选的缓存里取出上次服务端下发的队列上下文。 */
function findIssuedListContext(
    queryClient: QueryClient,
    filters: QueueFilters,
): string | undefined {
    const matches = queryClient.getQueriesData<ProcurementQueueView>({
        queryKey: [...procurementConfirmKeys.all, "queue"],
    })
    for (const [queryKey, data] of matches) {
        const cached = queryKey[2]
        if (!cached || typeof cached !== "object") continue
        if (!sameListIdentity(filters, cached as QueueFilters)) continue
        const issued = data?.context.queueContextId
        if (isServerIssuedQueueContextId(issued)) return issued
    }
    return undefined
}

export function useProcurementConfirmationQuery(filters: QueueFilters) {
    const queryClient = useQueryClient()
    return useQuery({
        queryKey: procurementConfirmKeys.queue(filters),
        queryFn: () =>
            fetchProcurementQueue(
                filters,
                findIssuedListContext(queryClient, filters),
            ),
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
                    queryKey: workItemKeys.all,
                })
            }
        },
    })
}
