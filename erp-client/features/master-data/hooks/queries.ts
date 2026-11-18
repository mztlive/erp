"use client"

import {
    keepPreviousData,
    useInfiniteQuery,
    useMutation,
    useQuery,
    useQueryClient,
} from "@tanstack/react-query"

import {
    importBrands,
    type BrandImportRow,
} from "@/features/master-data/api/brand-import"

import { backgroundJobKeys } from "@/lib/background-jobs"

import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    createMasterDataObject,
    createMasterDataRevision,
    disableMasterDataObject,
    fetchMasterDataCenter,
    fetchMasterDataList,
    fetchProductFilterOptions,
    fetchProductListSkus,
    listSellableItemsPage,
    updateProductListingStatus,
} from "@/features/master-data/api"
import { fetchFileAsset } from "@/features/master-data/api/media-assets"
import { updateVoucherCategoryRevision } from "@/features/master-data/api/mutations/voucher"
import {
    fetchWarehouseFulfillmentHandlerOptions,
    updateWarehouseFulfillmentHandlers,
    type UpdateWarehouseFulfillmentHandlersInput,
} from "@/features/master-data/api/warehouse-fulfillment"
import type {
    CreateMasterDataInput,
    CreateRevisionInput,
    DisableMasterDataInput,
    MasterDataListQuery,
    MasterDataResource,
    ProductListingStatus,
} from "@/features/master-data/types"
import { hasPermission } from "@/lib/permissions"
import { fetchUnitOptions } from "@/lib/options"
import { optionKeys } from "@/hooks/use-options"
import { queryKeyRoots } from "@/lib/query-key-roots"

export const masterDataKeys = {
    all: ["master-data"] as const,
    list: (query: MasterDataListQuery) =>
        [...masterDataKeys.all, "list", query] as const,
    detail: (resource: MasterDataResource, stableId: string) =>
        [...masterDataKeys.all, "detail", resource, stableId] as const,
}

const warehouseFulfillmentHandlerOptionsKey = [
    ...masterDataKeys.all,
    "warehouse-fulfillment-handler-options",
] as const

/** 仓库责任配置候选；仅打开配置对话框时读取。 */
export function useWarehouseFulfillmentHandlerOptionsQuery(enabled: boolean) {
    return useQuery({
        queryKey: warehouseFulfillmentHandlerOptionsKey,
        queryFn: fetchWarehouseFulfillmentHandlerOptions,
        enabled,
    })
}

/** 更新仓库收发责任后刷新列表与对象中心。 */
export function useUpdateWarehouseFulfillmentHandlersMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: UpdateWarehouseFulfillmentHandlersInput) =>
            updateWarehouseFulfillmentHandlers(input),
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: masterDataKeys.all,
                }),
                queryClient.invalidateQueries({
                    queryKey: [...queryKeyRoots.entitySelectors, "warehouse"],
                }),
            ])
        },
    })
}

function masterDataListAccess(
    resource: MasterDataResource,
    permissions: readonly string[] | undefined,
) {
    return resource === "warehouses"
        ? hasPermission(permissions, "warehouse_revision:list")
        : resource === "voucher-categories"
          ? hasPermission(permissions, "sku:list")
          : undefined
}

export function useMasterDataListQuery(
    query: MasterDataListQuery,
    enabled = true,
) {
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    const extraAccess = masterDataListAccess(query.resource, permissions)
    return useQuery({
        queryKey:
            extraAccess === undefined
                ? masterDataKeys.list(query)
                : [...masterDataKeys.list(query), extraAccess],
        queryFn: () => fetchMasterDataList(query, extraAccess),
        placeholderData: (previous, query) =>
            query?.queryKey[3] === extraAccess ? previous : undefined,
        enabled,
    })
}

export function useProductUnitOptionsQuery(enabled: boolean) {
    return useQuery({
        queryKey: optionKeys.units,
        queryFn: fetchUnitOptions,
        staleTime: 5 * 60 * 1000,
        enabled,
    })
}

/** 公司商品池表格：服务端分页与排序，只取当前页。 */
export function useSellablePageQuery(
    query: MasterDataListQuery & { page: number; pageSize: number },
    enabled: boolean,
) {
    return useQuery({
        queryKey: [...masterDataKeys.all, "sellable-page", query] as const,
        queryFn: () => listSellableItemsPage(query),
        placeholderData: keepPreviousData,
        enabled,
    })
}

/** 公司商品池卡片：滚动到底时按页追加。 */
export function useSellableInfiniteQuery(
    query: MasterDataListQuery,
    pageSize: number,
    enabled: boolean,
) {
    return useInfiniteQuery({
        queryKey: [
            ...masterDataKeys.all,
            "sellable-infinite",
            query,
            pageSize,
        ] as const,
        queryFn: ({ pageParam }) =>
            listSellableItemsPage({ ...query, page: pageParam, pageSize }),
        initialPageParam: 1,
        getNextPageParam: (last, pages) =>
            pages.reduce((count, page) => count + page.rows.length, 0) <
            last.total
                ? pages.length + 1
                : undefined,
        placeholderData: keepPreviousData,
        enabled,
    })
}

/** 商品列表当前页的启用 SKU、一件代发价与新增供给所需固定身份。 */
export function useProductListSkusQuery(
    productIds: readonly string[],
    canReadUnits: boolean,
) {
    const normalized = [...new Set(productIds.filter(Boolean))].sort()
    return useQuery({
        queryKey: [
            ...masterDataKeys.all,
            "product-list-skus",
            normalized,
            canReadUnits,
        ],
        queryFn: () => fetchProductListSkus(normalized, canReadUnits),
        enabled: normalized.length > 0,
    })
}

/** 商品列表筛选的启用分类、品牌与供应商选项。 */
export function useProductFilterOptionsQuery(enabled: boolean) {
    const account = useAccountProfileQuery()
    const permissions = account.data?.permissions ?? []
    return useQuery({
        queryKey: [
            ...masterDataKeys.all,
            "product-filter-options",
            account.data?.userid,
            permissions,
        ],
        queryFn: () => fetchProductFilterOptions(permissions),
        enabled: enabled && account.isSuccess,
    })
}

/** 导出时重新查询服务端，确保资格与权限按导出时点重新计算。 */
export function useMasterDataExportMutation() {
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    return useMutation({
        mutationFn: (query: MasterDataListQuery) =>
            fetchMasterDataList(
                query,
                masterDataListAccess(query.resource, permissions),
            ),
    })
}

export function useMasterDataCenterQuery(
    resource: MasterDataResource,
    stableId: string,
) {
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    const extraAccess =
        resource === "products"
            ? {
                  assets: hasPermission(permissions, "file_asset:detail"),
                  units: hasPermission(permissions, "unit_of_measure:list"),
                  categories: hasPermission(
                      permissions,
                      "product_category:list",
                  ),
                  brands: hasPermission(permissions, "product_brand:list"),
              }
            : resource === "voucher-categories"
              ? { skus: hasPermission(permissions, "sku:list") }
              : resource === "suppliers" || resource === "brands"
                ? { assets: hasPermission(permissions, "file_asset:detail") }
                : undefined
    return useQuery({
        queryKey: extraAccess
            ? [...masterDataKeys.detail(resource, stableId), extraAccess]
            : masterDataKeys.detail(resource, stableId),
        queryFn: () => fetchMasterDataCenter(resource, stableId, extraAccess),
        enabled: Boolean(stableId) && (!extraAccess || !account.isPending),
    })
}

/** W14 SKU 行的正式供给供应商数量，不从入库队列反推。 */
/** 主数据变更后同步失效相关缓存（含计量单位下拉）。 */
async function invalidateMasterDataCaches(
    queryClient: ReturnType<typeof useQueryClient>,
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
        meta: { affectsDataScope: true },
        mutationFn: (input: CreateMasterDataInput) =>
            createMasterDataObject(input),
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
        meta: { affectsDataScope: true },
        mutationFn: (input: CreateRevisionInput) =>
            createMasterDataRevision(input),
        onSuccess: async (result) => {
            // 冲突时也刷新详情与列表，让 lockVersion 回到最新，避免「关闭-重填-再冲突」死循环。
            if (
                result.outcome === "succeeded" ||
                result.outcome === "conflict"
            ) {
                await invalidateMasterDataCaches(queryClient)
            }
        },
    })
}

/** 卡券启停通过既有更新权限提交，完成后同步列表、详情和选择器缓存。 */
export function useVoucherCategoryStatusMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ({
            input,
            status,
        }: {
            input: CreateRevisionInput
            status: "active" | "disabled"
        }) => updateVoucherCategoryRevision(input, status),
        onSettled: async () => {
            await Promise.all([
                invalidateMasterDataCaches(queryClient),
                queryClient.invalidateQueries({
                    queryKey: ["sales-orders", "voucher-category"],
                }),
            ])
        },
    })
}

export function useDisableMasterDataMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: DisableMasterDataInput) =>
            disableMasterDataObject(input),
        onSuccess: async (result) => {
            if (
                result.outcome === "succeeded" ||
                result.outcome === "conflict"
            ) {
                await invalidateMasterDataCaches(queryClient)
            }
        },
    })
}

const FILE_ASSET_STALE_TIME = 5 * 60 * 1000

/** 商品主图等文件资产详情（含公开地址）；按资产 ID 缓存。 */
export function useFileAssetQuery(assetId: string | undefined) {
    const account = useAccountProfileQuery()
    const canReadAsset =
        account.isSuccess &&
        hasPermission(account.data.permissions, "file_asset:detail")
    const allowedId = canReadAsset ? assetId : undefined
    return useQuery({
        queryKey: [...masterDataKeys.all, "file-asset", allowedId ?? ""],
        queryFn: () => fetchFileAsset(allowedId ?? ""),
        enabled: Boolean(allowedId?.trim()),
        staleTime: FILE_ASSET_STALE_TIME,
    })
}

/** 商品列表整组上/下架，成功后同步商品列表、详情与公司商品池。 */
export function useProductListingMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        meta: { affectsDataScope: true },
        mutationFn: (input: {
            productId: string
            listingStatus: Exclude<ProductListingStatus, "PARTIALLY_LISTED">
        }) => updateProductListingStatus(input.productId, input.listingStatus),
        onSuccess: async () => {
            await invalidateMasterDataCaches(queryClient)
        },
    })
}

/** 一次提交持久化后台任务，响应成功后即可关闭弹窗。 */
export function useBrandImportMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: ({
            requestId,
            fileName,
            rows,
        }: {
            requestId: string
            fileName: string
            rows: BrandImportRow[]
        }) => importBrands(requestId, fileName, rows),
        retry: false,
        onSuccess: () => {
            void queryClient.invalidateQueries({
                queryKey: backgroundJobKeys.all,
            })
        },
    })
}
