"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  applySupplierCatalogWorkItemAction,
  attemptUnregisteredFormalWrite,
  claimSupplierCatalogWorkItem,
  completeSupplierCatalogWorkItem,
  createSupplierCatalogItem,
  fetchSupplierCatalogCenter,
  fetchCompanySkuOptions,
  fetchSupplierCatalogQueue,
  fetchSupplierProductPoolMatch,
  linkPromoteToCompanyPool,
  promoteSupplierProductToPool,
  reversePromoteToCompanyPool,
  reviseSupplierCatalogProduct,
  saveSessionDraft,
} from "@/features/supplier-catalog/api"
import type {
  CreateSupplierCatalogItemInput,
  LinkPromoteToCompanyPoolInput,
  PromoteSupplierProductInput,
  ReversePromoteToCompanyPoolInput,
  ReviseSupplierCatalogProductInput,
  SupplierCatalogQueueQuery,
} from "@/features/supplier-catalog/types"

export const supplierCatalogKeys = {
  all: ["supplier-catalog"] as const,
  queue: (query: SupplierCatalogQueueQuery) =>
    [...supplierCatalogKeys.all, "queue", query] as const,
  center: (id: string, section: string) =>
    [...supplierCatalogKeys.all, "center", id, section] as const,
  companySkuOptions: () => [...supplierCatalogKeys.all, "company-skus"] as const,
  poolMatch: (supplierProductId: string) =>
    [...supplierCatalogKeys.all, "pool-match", supplierProductId] as const,
}

export function useSupplierCatalogQueueQuery(query: SupplierCatalogQueueQuery) {
  return useQuery({
    queryKey: supplierCatalogKeys.queue(query),
    queryFn: () => fetchSupplierCatalogQueue(query),
  })
}

export function useCompanySkuOptionsQuery() {
  return useQuery({
    queryKey: supplierCatalogKeys.companySkuOptions(),
    queryFn: fetchCompanySkuOptions,
  })
}

export function useCreateSupplierCatalogItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateSupplierCatalogItemInput) =>
      createSupplierCatalogItem(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierCatalogKeys.all })
    },
  })
}

export function useReviseSupplierCatalogProductMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: ReviseSupplierCatalogProductInput) =>
      reviseSupplierCatalogProduct(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierCatalogKeys.all })
    },
  })
}

export function usePromoteSupplierProductMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: PromoteSupplierProductInput) =>
      promoteSupplierProductToPool(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierCatalogKeys.all })
    },
  })
}

export function useSupplierProductPoolMatchQuery(
  supplierProductId: string | undefined,
  enabled = true
) {
  return useQuery({
    queryKey: supplierCatalogKeys.poolMatch(supplierProductId ?? ""),
    queryFn: () => fetchSupplierProductPoolMatch(supplierProductId!),
    enabled: Boolean(supplierProductId) && enabled,
  })
}

export function useReversePromoteToCompanyPoolMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: ReversePromoteToCompanyPoolInput) =>
      reversePromoteToCompanyPool(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierCatalogKeys.all })
      await queryClient.invalidateQueries({ queryKey: ["master-data"] })
    },
  })
}

export function useLinkPromoteToCompanyPoolMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: LinkPromoteToCompanyPoolInput) =>
      linkPromoteToCompanyPool(input),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: supplierCatalogKeys.all })
      await queryClient.invalidateQueries({ queryKey: ["master-data"] })
    },
  })
}

/** @deprecated 使用 useReversePromoteToCompanyPoolMutation */
export function useCreateCompanyProductFromSupplierSkuMutation() {
  return useReversePromoteToCompanyPoolMutation()
}

export function useSupplierCatalogCenterQuery(input: {
  supplierProductId: string
  section?: string
  enabled?: boolean
}) {
  return useQuery({
    queryKey: supplierCatalogKeys.center(
      input.supplierProductId,
      input.section ?? "overview"
    ),
    queryFn: () =>
      fetchSupplierCatalogCenter({
        supplierProductId: input.supplierProductId,
        section: input.section,
      }),
    enabled: input.enabled !== false && Boolean(input.supplierProductId),
  })
}

export function useClaimSupplierCatalogMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: claimSupplierCatalogWorkItem,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: supplierCatalogKeys.all,
      })
    },
  })
}

export function useSupplierCatalogActionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: applySupplierCatalogWorkItemAction,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: supplierCatalogKeys.all,
        })
      }
    },
  })
}

export function useCompleteSupplierCatalogMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: completeSupplierCatalogWorkItem,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: supplierCatalogKeys.all,
        })
      }
    },
  })
}

export function useSaveSupplierCatalogDraftMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveSessionDraft,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: supplierCatalogKeys.all,
      })
    },
  })
}

export function useAttemptUnregisteredWriteMutation() {
  return useMutation({
    mutationFn: attemptUnregisteredFormalWrite,
  })
}
