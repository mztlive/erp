"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  createAdjustmentDraft,
  fetchBalanceDetail,
  fetchInventoryList,
  getAdjustmentDraft,
  resolveAdjustmentUnknown,
  saveAdjustmentDraft,
  startInventoryExport,
  submitAdjustment,
} from "@/features/inventory/api"
import type { InventoryQuery } from "@/features/inventory/types"

export const inventoryKeys = {
  all: ["inventory"] as const,
  list: (query: InventoryQuery) =>
    [...inventoryKeys.all, "list", query] as const,
  detail: (balanceId: string) =>
    [...inventoryKeys.all, "detail", balanceId] as const,
  draft: (stockAdjustmentId: string) =>
    [...inventoryKeys.all, "draft", stockAdjustmentId] as const,
}

export function useInventoryListQuery(query: InventoryQuery) {
  return useQuery({
    queryKey: inventoryKeys.list(query),
    queryFn: () => fetchInventoryList(query),
  })
}

export function useBalanceDetailQuery(balanceId: string | null) {
  return useQuery({
    queryKey: inventoryKeys.detail(balanceId ?? ""),
    queryFn: () => fetchBalanceDetail(balanceId!),
    enabled: Boolean(balanceId),
  })
}

export function useAdjustmentDraftQuery(stockAdjustmentId: string | null) {
  return useQuery({
    queryKey: inventoryKeys.draft(stockAdjustmentId ?? ""),
    queryFn: () => getAdjustmentDraft(stockAdjustmentId!),
    enabled: Boolean(stockAdjustmentId),
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

export function useSaveAdjustmentDraftMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveAdjustmentDraft,
    onSuccess: async (draft) => {
      await queryClient.invalidateQueries({
        queryKey: inventoryKeys.draft(draft.stockAdjustmentId),
      })
    },
  })
}

export function useSubmitAdjustmentMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: submitAdjustment,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: inventoryKeys.all })
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
        await queryClient.invalidateQueries({ queryKey: inventoryKeys.all })
      }
    },
  })
}

export function useStartInventoryExportMutation() {
  return useMutation({
    mutationFn: startInventoryExport,
  })
}
