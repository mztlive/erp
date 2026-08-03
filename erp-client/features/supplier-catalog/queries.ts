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
  promoteSupplierProductToPool,
  resolveUnknownSupplierCatalogResult,
  reviseSupplierCatalogProduct,
  saveSessionDraft,
} from "@/features/supplier-catalog/api"
import type {
  CreateSupplierCatalogItemInput,
  DemoRole,
  PromoteSupplierProductInput,
  ReviseSupplierCatalogProductInput,
  SupplierCatalogQueueQuery,
} from "@/features/supplier-catalog/types"

export const supplierCatalogKeys = {
  all: ["supplier-catalog"] as const,
  queue: (query: SupplierCatalogQueueQuery) =>
    [...supplierCatalogKeys.all, "queue", query] as const,
  center: (id: string, section: string, role: DemoRole, mask: boolean) =>
    [...supplierCatalogKeys.all, "center", id, section, role, mask] as const,
  companySkuOptions: () => [...supplierCatalogKeys.all, "company-skus"] as const,
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

export function useSupplierCatalogCenterQuery(input: {
  supplierProductId: string
  section?: string
  demoRole?: DemoRole
  maskCost?: boolean
  enabled?: boolean
}) {
  const role = input.demoRole ?? "procurement"
  const mask = Boolean(input.maskCost)
  return useQuery({
    queryKey: supplierCatalogKeys.center(
      input.supplierProductId,
      input.section ?? "overview",
      role,
      mask
    ),
    queryFn: () =>
      fetchSupplierCatalogCenter({
        supplierProductId: input.supplierProductId,
        section: input.section,
        demoRole: input.demoRole,
        maskCost: input.maskCost,
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

export function useResolveUnknownSupplierCatalogMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: resolveUnknownSupplierCatalogResult,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: supplierCatalogKeys.all,
        })
      }
    },
  })
}

export function useAttemptUnregisteredWriteMutation() {
  return useMutation({
    mutationFn: attemptUnregisteredFormalWrite,
  })
}
