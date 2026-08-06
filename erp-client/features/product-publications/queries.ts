"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
  fetchPublicationDetail,
  fetchPublicationList,
  manualPausePublication,
  publishRevision,
  retryDelivery,
} from "@/features/product-publications/api"
import type { ProductPublicationListQuery } from "@/features/product-publications/types"

export const publicationKeys = {
  all: ["product-publications"] as const,
  list: (query: ProductPublicationListQuery) =>
    [...publicationKeys.all, "list", query] as const,
  detail: (publicationId: string, revisionId?: string) =>
    [...publicationKeys.all, "detail", publicationId, revisionId ?? "latest"] as const,
}

export function usePublicationListQuery(query: ProductPublicationListQuery) {
  return useQuery({
    queryKey: publicationKeys.list(query),
    queryFn: () => fetchPublicationList(query),
  })
}

export function usePublicationDetailQuery(
  publicationId: string | null,
  revisionId?: string | null
) {
  return useQuery({
    queryKey: publicationKeys.detail(
      publicationId ?? "",
      revisionId ?? undefined
    ),
    queryFn: () =>
      fetchPublicationDetail(publicationId!, revisionId ?? undefined),
    enabled: Boolean(publicationId),
  })
}

export function usePublishRevisionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: publishRevision,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: publicationKeys.all })
      }
    },
  })
}

export function useManualPauseMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: manualPausePublication,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: publicationKeys.all })
      }
    },
  })
}

export function useRetryDeliveryMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: retryDelivery,
    onSuccess: async (result) => {
      if (result.status === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: publicationKeys.all })
      }
    },
  })
}
