"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    adjustProcurementRejectionDraft,
    cancelCardSalesApproval,
    createSalesOrder,
    createSalesOrderExportJob,
    fetchSalesOrderDetail,
    fetchSalesOrderDraftForResume,
    fetchSalesOrders,
    resolveProcurementRejection,
    saveSalesOrderDraft,
    startSalesChangeOrder,
    submitSalesChangeReviewDecision,
    submitSalesOrder,
    submitCardSalesApprovalDecision,
    type SalesOrdersListQuery,
} from "@/features/sales-orders/api/sales-orders"

export const salesOrderKeys = {
    all: ["sales-orders"] as const,
    list: (query: SalesOrdersListQuery) =>
        [...salesOrderKeys.all, "list", query] as const,
    detail: (id: string) => [...salesOrderKeys.all, "detail", id] as const,
    acceptanceRoot: (id: string) =>
        [...salesOrderKeys.all, "acceptance", id] as const,
    acceptance: (
        id: string,
        filters: { remainingOnly?: boolean; workItemId?: string | null },
    ) => [...salesOrderKeys.acceptanceRoot(id), filters] as const,
}

export function useSalesOrdersQuery(
    query: SalesOrdersListQuery,
    enabled = true,
) {
    return useQuery({
        queryKey: salesOrderKeys.list(query),
        queryFn: () => fetchSalesOrders(query),
        enabled,
    })
}

export function useSalesOrderDetailQuery(salesOrderId: string) {
    return useQuery({
        queryKey: salesOrderKeys.detail(salesOrderId),
        queryFn: () => fetchSalesOrderDetail(salesOrderId),
    })
}

export function useSalesOrderDraftResumeQuery(salesOrderId: string) {
    return useQuery({
        queryKey: [
            ...salesOrderKeys.detail(salesOrderId),
            "draft-resume",
        ] as const,
        queryFn: () => fetchSalesOrderDraftForResume(salesOrderId),
        enabled: salesOrderId.length > 0,
    })
}

export function useCreateSalesOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createSalesOrder,
        onSuccess: async (data) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.all,
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(data.salesOrderId),
            })
            await queryClient.invalidateQueries({ queryKey: ["contracts"] })
        },
    })
}

export function useSaveSalesOrderDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: saveSalesOrderDraft,
        onSuccess: async (_data, variables) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(variables.salesOrderId),
            })
        },
    })
}

export function useSubmitSalesOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesOrder,
        onSuccess: async (data) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.all,
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(data.salesOrderId),
            })
        },
    })
}

export function useAdjustProcurementRejectionDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: adjustProcurementRejectionDraft,
        onSuccess: async (_data, variables) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(variables.salesOrderId),
            })
        },
    })
}

export function useResolveProcurementRejectionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: resolveProcurementRejection,
        onSuccess: async (_data, variables) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(variables.salesOrderId),
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.all,
            })
        },
    })
}

export function useStartSalesChangeOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: startSalesChangeOrder,
        onSuccess: async (_data, variables) => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(variables.salesOrderId),
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.all,
            })
        },
    })
}

export function useSalesChangeReviewDecisionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitSalesChangeReviewDecision,
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: salesOrderKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: ["work-items"],
                }),
            ])
        },
    })
}

export function useSubmitCardSalesApprovalDecisionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitCardSalesApprovalDecision,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.all,
            })
        },
    })
}

/** 撤回卡券审批后刷新对象详情与销售单列表。 */
export const useCancelCardSalesApprovalMutation = () => {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: cancelCardSalesApproval,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.all,
            })
        },
    })
}

export function useCreateSalesOrderExportJobMutation() {
    return useMutation({
        mutationFn: createSalesOrderExportJob,
    })
}
