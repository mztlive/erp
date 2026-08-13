"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    acquireDraftEditToken,
    createPurchaseOrderFromBasis,
    fetchCreationBases,
    fetchPurchaseOrderCenter,
    fetchPurchaseOrderExportData,
    fetchPurchaseOrders,
    reviewPurchaseOrder,
    savePurchaseOrderDraft,
    startPurchaseChange,
    submitPurchaseOrderForReview,
} from "@/features/purchase-orders/api/purchase-orders"
import type { PurchaseOrderListQuery } from "@/features/purchase-orders/api/purchase-orders"

export const purchaseOrderKeys = {
    all: ["purchase-orders"] as const,
    list: (query: PurchaseOrderListQuery) =>
        [...purchaseOrderKeys.all, "list", query] as const,
    detail: (id: string) => [...purchaseOrderKeys.all, "detail", id] as const,
    bases: () => [...purchaseOrderKeys.all, "creation-bases"] as const,
    exportData: (query: PurchaseOrderListQuery) =>
        [...purchaseOrderKeys.all, "export", query] as const,
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

export function usePurchaseOrderCenterQuery(purchaseOrderId: string) {
    return useQuery({
        queryKey: purchaseOrderKeys.detail(purchaseOrderId),
        queryFn: () => fetchPurchaseOrderCenter(purchaseOrderId),
        enabled: Boolean(purchaseOrderId),
    })
}

export function useCreationBasesQuery() {
    return useQuery({
        queryKey: purchaseOrderKeys.bases(),
        queryFn: fetchCreationBases,
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
            await queryClient.invalidateQueries({
                queryKey: purchaseOrderKeys.all,
            })
            void variables.purchaseOrderId
        },
    })
}

export function useSubmitPurchaseOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: submitPurchaseOrderForReview,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await queryClient.invalidateQueries({
                queryKey: purchaseOrderKeys.all,
            })
        },
    })
}

export function useReviewPurchaseOrderMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: reviewPurchaseOrder,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await queryClient.invalidateQueries({
                queryKey: purchaseOrderKeys.all,
            })
        },
    })
}

export function useStartPurchaseChangeMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: startPurchaseChange,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await queryClient.invalidateQueries({
                queryKey: purchaseOrderKeys.all,
            })
        },
    })
}

export function useCreateFromBasisMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: createPurchaseOrderFromBasis,
        onSuccess: async (result) => {
            if (result.status !== "succeeded") return
            await queryClient.invalidateQueries({
                queryKey: purchaseOrderKeys.all,
            })
        },
    })
}
