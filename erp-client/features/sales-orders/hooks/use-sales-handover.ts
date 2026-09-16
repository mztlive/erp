"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    fetchSalesHandoverCandidates,
    submitSalesHandover,
} from "@/features/sales-orders/api/sales-orders-handover"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { workItemKeys } from "@/features/work-items/queries"

export const salesHandoverKeys = {
    all: [...queryKeyRoots.salesOrders, "handover"] as const,
    candidates: (salesOrderId: string) =>
        [...salesHandoverKeys.all, "candidates", salesOrderId] as const,
}

/**
 * 交接随转预览键：与详情验收面同一键，命中缓存不新增请求。
 * 交接面 URL 即销售单详情 URL，对话框内展示未结单据摘要与开放验收。
 */
export const salesHandoverPreviewKey = (salesOrderId: string) =>
    [...salesOrderKeys.acceptanceRoot(salesOrderId), "readonly"] as const

/** 交接待选目标查询：只在打开交接面时启用。 */
export function useSalesHandoverCandidatesQuery(
    salesOrderId: string,
    enabled = true,
) {
    return useQuery({
        queryKey: salesHandoverKeys.candidates(salesOrderId),
        queryFn: () => fetchSalesHandoverCandidates(salesOrderId),
        enabled: enabled && salesOrderId.trim().length > 0,
    })
}

/** 提交显式销售责任交接；成功后刷新销售单、验收与任务缓存。 */
export function useSubmitSalesHandoverMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: submitSalesHandover,
        onSuccess: async (data) => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.detail(data.salesOrderId),
                }),
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: salesHandoverKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}
