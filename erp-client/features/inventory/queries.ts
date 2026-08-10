"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    createAdjustmentDraft,
    fetchBalanceDetail,
    fetchInventoryList,
    resolveAdjustmentUnknown,
    startInventoryExport,
    submitAdjustment,
} from "@/features/inventory/api"
import type { InventoryQuery } from "@/features/inventory/types"

const inventoryKeys = {
    all: ["inventory"] as const,
    list: (query: InventoryQuery) =>
        [...inventoryKeys.all, "list", query] as const,
    detail: (balanceId: string) =>
        [...inventoryKeys.all, "detail", balanceId] as const,
    draft: (stockAdjustmentId: string) =>
        [...inventoryKeys.all, "draft", stockAdjustmentId] as const,
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
                await queryClient.invalidateQueries({
                    queryKey: inventoryKeys.all,
                })
            }
        },
    })
}

export function useResolveAdjustmentUnknownMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: resolveAdjustmentUnknown,
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: inventoryKeys.all,
                })
            }
        },
    })
}

export function useStartInventoryExportMutation() {
    return useMutation({
        mutationFn: startInventoryExport,
    })
}
