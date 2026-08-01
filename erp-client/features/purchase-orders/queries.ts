"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import {
  acquireDraftEditToken,
  createPurchaseOrderFromBasis,
  fetchCreationBases,
  fetchPurchaseOrderCenter,
  fetchPurchaseOrders,
  reviewPurchaseOrder,
  savePurchaseOrderDraft,
  startPurchaseChange,
  submitPurchaseOrderForReview,
} from "@/features/purchase-orders/api"
import type { ViewerRole } from "@/features/purchase-orders/types"

export const purchaseOrderKeys = {
  all: ["purchase-orders"] as const,
  list: (role: ViewerRole) =>
    [...purchaseOrderKeys.all, "list", role] as const,
  detail: (id: string, role: ViewerRole) =>
    [...purchaseOrderKeys.all, "detail", id, role] as const,
  bases: () => [...purchaseOrderKeys.all, "creation-bases"] as const,
}

export function usePurchaseOrdersQuery(role: ViewerRole = "procurement") {
  return useQuery({
    queryKey: purchaseOrderKeys.list(role),
    queryFn: () => fetchPurchaseOrders(role),
  })
}

export function usePurchaseOrderCenterQuery(
  purchaseOrderId: string,
  role: ViewerRole = "procurement"
) {
  return useQuery({
    queryKey: purchaseOrderKeys.detail(purchaseOrderId, role),
    queryFn: () => fetchPurchaseOrderCenter(purchaseOrderId, role),
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
