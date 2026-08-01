"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  applyExternalCatalogWorkItemAction,
  attemptUnregisteredFormalWrite,
  claimExternalCatalogWorkItem,
  completeExternalCatalogWorkItem,
  fetchExternalCatalogCenter,
  fetchExternalCatalogQueue,
  resolveUnknownExternalCatalogResult,
  saveSessionDraft,
} from "@/features/external-product-supply/api"
import type {
  DemoRole,
  ExternalCatalogQueueQuery,
} from "@/features/external-product-supply/types"

export const externalCatalogKeys = {
  all: ["external-product-supply"] as const,
  queue: (query: ExternalCatalogQueueQuery) =>
    [...externalCatalogKeys.all, "queue", query] as const,
  center: (id: string, section: string, role: DemoRole, mask: boolean) =>
    [...externalCatalogKeys.all, "center", id, section, role, mask] as const,
}

export function useExternalCatalogQueueQuery(query: ExternalCatalogQueueQuery) {
  return useQuery({
    queryKey: externalCatalogKeys.queue(query),
    queryFn: () => fetchExternalCatalogQueue(query),
  })
}

export function useExternalCatalogCenterQuery(input: {
  externalProductId: string
  section?: string
  demoRole?: DemoRole
  maskCost?: boolean
  enabled?: boolean
}) {
  const role = input.demoRole ?? "procurement"
  const mask = Boolean(input.maskCost)
  return useQuery({
    queryKey: externalCatalogKeys.center(
      input.externalProductId,
      input.section ?? "overview",
      role,
      mask
    ),
    queryFn: () =>
      fetchExternalCatalogCenter({
        externalProductId: input.externalProductId,
        section: input.section,
        demoRole: input.demoRole,
        maskCost: input.maskCost,
      }),
    enabled: input.enabled !== false && Boolean(input.externalProductId),
  })
}

export function useClaimExternalCatalogMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: claimExternalCatalogWorkItem,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: externalCatalogKeys.all,
      })
    },
  })
}

export function useExternalCatalogActionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: applyExternalCatalogWorkItemAction,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: externalCatalogKeys.all,
        })
      }
    },
  })
}

export function useCompleteExternalCatalogMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: completeExternalCatalogWorkItem,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: externalCatalogKeys.all,
        })
      }
    },
  })
}

export function useSaveExternalCatalogDraftMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: saveSessionDraft,
    onSuccess: async () => {
      await queryClient.invalidateQueries({
        queryKey: externalCatalogKeys.all,
      })
    },
  })
}

export function useResolveUnknownExternalCatalogMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: resolveUnknownExternalCatalogResult,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({
          queryKey: externalCatalogKeys.all,
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
