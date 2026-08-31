"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { approvalKeys } from "@/features/approval-workflow/queries"
import { workItemKeys } from "@/features/work-items/queries"
import {
    cancelStockAdjustmentApproval,
    createAdjustmentDraft,
    fetchAdjustmentDetail,
    fetchBalanceDetail,
    fetchInventoryList,
    resolveAdjustmentUnknown,
    startInventoryExport,
    submitAdjustment,
} from "@/features/inventory/api/inventory"
import type { InventoryQuery } from "@/features/inventory/types"

export const inventoryKeys = {
    all: ["inventory"] as const,
    list: (query: InventoryQuery) =>
        [...inventoryKeys.all, "list", query] as const,
    detail: (balanceId: string) =>
        [...inventoryKeys.all, "detail", balanceId] as const,
    adjustment: (stockAdjustmentId: string) =>
        [...inventoryKeys.all, "adjustment", stockAdjustmentId] as const,
    draft: (stockAdjustmentId: string) =>
        [...inventoryKeys.all, "draft", stockAdjustmentId] as const,
}

/**
 * 查询库存调整单详情（含只读审批绑定）。
 */
export function useAdjustmentDetailQuery(stockAdjustmentId: string | null) {
    return useQuery({
        queryKey: inventoryKeys.adjustment(stockAdjustmentId ?? ""),
        queryFn: () => fetchAdjustmentDetail(stockAdjustmentId!),
        enabled: Boolean(stockAdjustmentId),
    })
}

export function useInventoryListQuery(query: InventoryQuery, enabled = true) {
    return useQuery({
        queryKey: inventoryKeys.list(query),
        queryFn: () => fetchInventoryList(query),
        enabled,
    })
}

export function useBalanceDetailQuery(balanceId: string | null) {
    return useQuery({
        queryKey: inventoryKeys.detail(balanceId ?? ""),
        queryFn: () => fetchBalanceDetail(balanceId!),
        enabled: Boolean(balanceId),
    })
}

export function useCreateAdjustmentDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createAdjustmentDraft,
        onSuccess: async () => {
            await queryClient.invalidateQueries({ queryKey: inventoryKeys.all })
        },
    })
}

export function useSubmitAdjustmentMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitAdjustment,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: inventoryKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: approvalKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: workItemKeys.all,
                    }),
                ])
            }
        },
    })
}

/**
 * 撤回成功后刷新库存详情、审批运行事实与工作任务。
 */
export function useCancelStockAdjustmentApprovalMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: cancelStockAdjustmentApproval,
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: inventoryKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: approvalKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: workItemKeys.all,
                }),
            ])
        },
    })
}

export function useResolveAdjustmentUnknownMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: resolveAdjustmentUnknown,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: inventoryKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: approvalKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: workItemKeys.all,
                    }),
                ])
            }
        },
    })
}

export function useStartInventoryExportMutation() {
    return useMutation({
        mutationFn: startInventoryExport,
    })
}
