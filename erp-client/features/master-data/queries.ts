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
  fetchSkuSupplierCounts,
  queryMasterDataIdempotency,
  revealMasterDataSensitive,
  updateProductListingStatus,
} from "@/features/master-data/api"
import type {
  CreateMasterDataInput,
  CreateRevisionInput,
  DisableMasterDataInput,
  MasterDataListQuery,
  MasterDataResource,
  ProductListingStatus,
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

/** 导出时重新查询服务端，确保资格与权限按导出时点重新计算。 */
export function useMasterDataExportMutation() {
  return useMutation({
    mutationFn: (query: MasterDataListQuery) => fetchMasterDataList(query),
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

/** W14 SKU 行的正式供给供应商数量，不从入库队列反推。 */
export function useSkuSupplierCountsQuery(skuIds: readonly string[]) {
  const normalized = [...new Set(skuIds.filter(Boolean))].sort()
  return useQuery({
    queryKey: [...masterDataKeys.all, "sku-supplier-counts", normalized],
    queryFn: () => fetchSkuSupplierCounts(normalized),
    enabled: normalized.length > 0,
  })
}

/** 主数据变更后同步失效相关缓存（含计量单位下拉）。 */
async function invalidateMasterDataCaches(
  queryClient: ReturnType<typeof useQueryClient>
) {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: masterDataKeys.all }),
    queryClient.invalidateQueries({
      queryKey: ["supplier-offerings", "company-skus"],
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

/** 商品列表整组上/下架，成功后同步商品列表、详情与公司商品池。 */
export function useProductListingMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (input: {
      productId: string
      listingStatus: Exclude<ProductListingStatus, "PARTIALLY_LISTED">
    }) => updateProductListingStatus(input.productId, input.listingStatus),
    onSuccess: async () => {
      await invalidateMasterDataCaches(queryClient)
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
