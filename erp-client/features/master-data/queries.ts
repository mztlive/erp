"use client"

import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query"

import {
  createMasterDataObject,
  createMasterDataRevision,
  disableMasterDataObject,
  fetchMasterDataCenter,
  fetchMasterDataList,
  queryMasterDataIdempotency,
  revealMasterDataSensitive,
} from "@/features/master-data/api"
import type {
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  MasterDataListQuery,
  MasterDataResource,
} from "@/features/master-data/types"

export const masterDataKeys = {
  all: ["master-data"] as const,
  list: (query: MasterDataListQuery) =>
    [...masterDataKeys.all, "list", query] as const,
  detail: (resource: MasterDataResource, stableId: string) =>
    [...masterDataKeys.all, "detail", resource, stableId] as const,
}

export function useMasterDataListQuery(query: MasterDataListQuery) {
  return useQuery({
    queryKey: masterDataKeys.list(query),
    queryFn: () => fetchMasterDataList(query),
  })
}

export function useMasterDataCenterQuery(
  resource: MasterDataResource,
  stableId: string
) {
  return useQuery({
    queryKey: masterDataKeys.detail(resource, stableId),
    queryFn: () => fetchMasterDataCenter(resource, stableId),
    enabled: Boolean(stableId),
  })
}

export function useCreateMasterDataMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateMasterDataInput) => createMasterDataObject(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
      }
    },
  })
}

export function useCreateRevisionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateRevisionInput) => createMasterDataRevision(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
      }
    },
  })
}

export function useDisableMasterDataMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: DisableMasterDataInput) =>
      disableMasterDataObject(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
      }
    },
  })
}

export function useQueryMasterDataIdempotencyMutation() {
  return useMutation({
    mutationFn: (idempotencyKey: string) =>
      queryMasterDataIdempotency(idempotencyKey),
  })
}

export function useRevealMasterDataSensitiveMutation() {
  return useMutation({
    mutationFn: (revealToken: string) => revealMasterDataSensitive(revealToken),
  })
}
