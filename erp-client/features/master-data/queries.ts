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
import { optionKeys } from "@/hooks/use-options"

export {
  buildMasterDataExportCsv,
  downloadCsv,
} from "@/features/master-data/export-csv"
export { revealMasterDataSensitive } from "@/features/master-data/api"

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

/** 主数据变更后同步失效相关缓存（含计量单位下拉）。 */
async function invalidateMasterDataCaches(
  queryClient: ReturnType<typeof useQueryClient>
) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: masterDataKeys.all }),
    queryClient.invalidateQueries({
      queryKey: ["supplier-catalog", "company-skus"],
    }),
    queryClient.invalidateQueries({ queryKey: optionKeys.units }),
  ])
}

export function useCreateMasterDataMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateMasterDataInput) => createMasterDataObject(input),
    onSuccess: async (result) => {
      if (result.outcome === "succeeded") {
        await invalidateMasterDataCaches(queryClient)
      }
    },
  })
}

export function useCreateRevisionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: CreateRevisionInput) => createMasterDataRevision(input),
    onSuccess: async (result) => {
      // 冲突时也刷新详情与列表，让 lockVersion 回到最新，避免「关闭-重填-再冲突」死循环。
      if (result.outcome === "succeeded" || result.outcome === "conflict") {
        await invalidateMasterDataCaches(queryClient)
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
      if (result.outcome === "succeeded" || result.outcome === "conflict") {
        await invalidateMasterDataCaches(queryClient)
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
