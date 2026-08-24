"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { approvalKeys } from "@/features/approval-workflow/queries"
import {
    acquireDraftEditToken,
    createPurchaseOrderFromBasis,
    fetchCreationBases,
    fetchPurchaseChangeOrderDetail,
    fetchPurchaseOrderCenter,
    fetchPurchaseOrderExportData,
    fetchPurchaseOrders,
    reviewPurchaseOrder,
    savePurchaseOrderDraft,
    startPurchaseChange,
    submitPurchaseChange,
    submitPurchaseOrderForReview,
    voidPurchaseOrderDraft,
} from "@/features/purchase-orders/api/purchase-orders"
import type {
    CreationBasesQuery,
    PurchaseOrderListQuery,
} from "@/features/purchase-orders/api/purchase-orders"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import { workItemKeys } from "@/features/work-items/queries"
import { workspaceHomeKeys } from "@/features/workspace/hooks/queries"

export const purchaseOrderKeys = {
    all: ["purchase-orders"] as const,
    lists: () => [...purchaseOrderKeys.all, "list"] as const,
    list: (query: PurchaseOrderListQuery) =>
        [...purchaseOrderKeys.lists(), query] as const,
    detail: (id: string) => [...purchaseOrderKeys.all, "detail", id] as const,
    changeOrder: (id: string) =>
        [...purchaseOrderKeys.all, "change-order", id] as const,
    creationBases: () => [...purchaseOrderKeys.all, "creation-bases"] as const,
    bases: (query: CreationBasesQuery = {}) =>
        [...purchaseOrderKeys.creationBases(), query] as const,
    exportData: (query: PurchaseOrderListQuery) =>
        [...purchaseOrderKeys.all, "export", query] as const,
}

/**
 * 采购单写操作成功后失效单据、审批实例与任务缓存。
 *
 * @param queryClient 当前 QueryClient。
 */
const invalidatePurchaseOrderApprovalCaches = async (
    queryClient: ReturnType<typeof useQueryClient>,
) => {
    await Promise.all([
        queryClient.invalidateQueries({ queryKey: purchaseOrderKeys.all }),
        queryClient.invalidateQueries({ queryKey: approvalKeys.all }),
        queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
        queryClient.invalidateQueries({ queryKey: workspaceHomeKeys.all }),
        queryClient.invalidateQueries({ queryKey: salesOrderKeys.all }),
    ])
}

export function usePurchaseOrdersQuery(query: PurchaseOrderListQuery) {
    return useQuery({
        queryKey: purchaseOrderKeys.list(query),
        queryFn: () => fetchPurchaseOrders(query),
    })
}

export function usePurchaseOrderExportDataQuery(query: PurchaseOrderListQuery) {
    return useQuery({
        queryKey: purchaseOrderKeys.exportData(query),
        queryFn: () => fetchPurchaseOrderExportData(query),
        enabled: false,
    })
}

/**
 * 读取采购单对象中心。可按变更单 ID 精确挂上对应审批投影。
 *
 * @param purchaseOrderId 采购单 ID。
 * @param options.changeOrderId 任务或 URL 指定的采购变更单。
 */
export function usePurchaseOrderCenterQuery(
    purchaseOrderId: string,
    options?: { changeOrderId?: string },
) {
    return useQuery({
        queryKey: [
            ...purchaseOrderKeys.detail(purchaseOrderId),
            options?.changeOrderId ?? "",
        ],
        queryFn: () =>
            options?.changeOrderId
                ? fetchPurchaseOrderCenter(purchaseOrderId, options)
                : fetchPurchaseOrderCenter(purchaseOrderId),
        enabled: Boolean(purchaseOrderId),
    })
}

/**
 * 读取指定采购变更单详情。成功后页面消费只读审批投影。
 *
 * @param changeOrderId 变更单 ID。
 * @param enabled 是否发起请求。
 */
export function usePurchaseChangeOrderQuery(
    changeOrderId: string,
    enabled = true,
) {
    return useQuery({
        queryKey: purchaseOrderKeys.changeOrder(changeOrderId),
        queryFn: () => fetchPurchaseChangeOrderDetail(changeOrderId),
        enabled: enabled && Boolean(changeOrderId),
    })
}

export function useCreationBasesQuery(
    query: CreationBasesQuery = {},
    options?: { enabled?: boolean },
) {
    return useQuery({
        queryKey: purchaseOrderKeys.bases(query),
        queryFn: () => fetchCreationBases(query),
        enabled: options?.enabled ?? true,
    })
}

export function useAcquireDraftTokenMutation() {
    return useMutation({
        mutationFn: acquireDraftEditToken,
    })
}

export function useSavePurchaseOrderDraftMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: savePurchaseOrderDraft,
        onSuccess: async (result, variables) => {
            if (result.status !== "succeeded") return
            await invalidatePurchaseOrderApprovalCaches(queryClient)
            void variables.purchaseOrderId
        },
    })
}

/**
 * 作废采购草稿。成功后刷新采购覆盖、采购任务、工作台与销售单缓存。
 */
export function useVoidPurchaseOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: voidPurchaseOrderDraft,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await invalidatePurchaseOrderApprovalCaches(queryClient)
        },
    })
}

/**
 * 提交采购单并启动统一审批。成功后失效单据、审批与任务缓存。
 */
export function useSubmitPurchaseOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPurchaseOrderForReview,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await invalidatePurchaseOrderApprovalCaches(queryClient)
        },
    })
}

/**
 * 旧财务审核命令。成功后同样失效审批缓存，避免与统一决定双写。
 */
export function useReviewPurchaseOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reviewPurchaseOrder,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await invalidatePurchaseOrderApprovalCaches(queryClient)
        },
    })
}

/**
 * 发起采购变更工作副本。成功后失效单据与审批缓存。
 */
export function useStartPurchaseChangeMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: startPurchaseChange,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await invalidatePurchaseOrderApprovalCaches(queryClient)
        },
    })
}

/**
 * 提交采购变更并启动统一审批。成功后失效单据、审批与任务缓存。
 */
export function useSubmitPurchaseChangeMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPurchaseChange,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await invalidatePurchaseOrderApprovalCaches(queryClient)
        },
    })
}

/**
 * 按创建依据建草稿。成功后失效列表、审批绑定与任务缓存。
 */
export function useCreateFromBasisMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createPurchaseOrderFromBasis,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: purchaseOrderKeys.lists(),
                }),
                queryClient.invalidateQueries({
                    queryKey: purchaseOrderKeys.creationBases(),
                    refetchType: "none",
                }),
                queryClient.invalidateQueries({ queryKey: approvalKeys.all }),
                queryClient.invalidateQueries({ queryKey: workItemKeys.all }),
                queryClient.invalidateQueries({
                    queryKey: workspaceHomeKeys.all,
                }),
                queryClient.invalidateQueries({ queryKey: salesOrderKeys.all }),
            ])
        },
    })
}
