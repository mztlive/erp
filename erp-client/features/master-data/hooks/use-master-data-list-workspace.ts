"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/lib/export-csv"
import { resourceLabel } from "@/features/master-data/lib/data"
import { useMasterDataColumns } from "@/features/master-data/hooks/use-master-data-columns"
import { CREATE_PERMISSION_BY_RESOURCE } from "@/features/master-data/lib/list-filters"
import { getMasterDataResourceFlags } from "@/features/master-data/lib/master-data-list-resource"
import {
    buildFilterSnapshotLabel,
    buildListTableDescription,
    buildSellableTableDescription,
    syncListMetrics,
} from "@/features/master-data/lib/master-data-list-summaries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    useMasterDataCenterQuery,
    useMasterDataExportMutation,
    useMasterDataListQuery,
    useProductFilterOptionsQuery,
    useProductListSkusQuery,
    useProductListingMutation,
} from "@/features/master-data/hooks/queries"
import type {
    MasterDataListItem,
    MasterDataResource,
    ProductListSkuSummary,
} from "@/features/master-data/types"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/queries"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import { useMasterDataListFilters } from "@/features/master-data/hooks/use-master-data-list-filters"

export function useMasterDataListWorkspace({
    resource,
    searchInputRef,
    resultsHeadingRef,
    lastFocusedRowId,
}: {
    resource: MasterDataResource
    searchInputRef: React.RefObject<HTMLInputElement | null>
    resultsHeadingRef: React.RefObject<HTMLHeadingElement | null>
    lastFocusedRowId: React.MutableRefObject<string | null>
}) {
    const router = useRouter()
    const accountQuery = useAccountProfileQuery()
    const {
        isProductResource,
        isSupplierResource,
        isBrandResource,
        isVoucherCategoryResource,
        isUnitOfMeasureResource,
        isSellableResource,
        isWarehouse,
        skipPreviewSheet,
        showEffectiveColumn,
    } = getMasterDataResourceFlags(resource)

    const createPermission = CREATE_PERMISSION_BY_RESOURCE[resource]
    const canCreate = createPermission
        ? hasPermission(accountQuery.data?.permissions, createPermission)
        : false
    const createBlockedReason = accountQuery.isPending
        ? "正在核对创建权限，请稍候。"
        : accountQuery.isError
          ? getErrorMessage(
                accountQuery.error,
                "暂时无法核对创建权限，请刷新后重试。",
            )
          : "当前账号没有新建此类资料的权限。"

    const filters = useMasterDataListFilters({
        isProductResource,
        isSupplierResource,
        isSellableResource,
        searchInputRef,
    })

    const {
        q,
        lifecycleStatus,
        revisionTiming,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        supplyRegion,
        productListingStatus,
        productSupplyCoverage,
        productSalesPriceMin,
        productSalesPriceMax,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        metricKey,
        hasStructuredProductFilters,
        hasStructuredSellableFilters,
        hasStructuredSupplierFilters,
        hasStructuredListFilters,
        searchDraft,
        setSearchDraft,
        productFilterPanelOpen,
        setProductFilterPanelOpen,
        sellableFilterPanelOpen,
        setSellableFilterPanelOpen,
        supplierFilterPanelOpen,
        setSupplierFilterPanelOpen,
        filterPanelOpen,
        setFilterPanelOpen,
        supplierCapabilityCodesDraft,
        setSupplierCapabilityCodesDraft,
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
        productKindDraft,
        setProductKindDraft,
        lifecycleStatusDraft,
        setLifecycleStatusDraft,
        revisionTimingDraft,
        setRevisionTimingDraft,
        productListingStatusDraft,
        setProductListingStatusDraft,
        productSupplyCoverageDraft,
        setProductSupplyCoverageDraft,
        productCategoryIdDraft,
        setProductCategoryIdDraft,
        productBrandIdDraft,
        setProductBrandIdDraft,
        productSupplierIdDraft,
        setProductSupplierIdDraft,
        productSalesPriceMinDraft,
        setProductSalesPriceMinDraft,
        productSalesPriceMaxDraft,
        setProductSalesPriceMaxDraft,
        supplyRegionDraft,
        setSupplyRegionDraft,
        productSalesPriceError,
        setProductSalesPriceError,
        pagination,
        setPagination,
        patchUrl,
        changeLifecycle,
        commitSearch,
        applySellableFilters,
        applySupplierFilters,
        applyListFilters,
        applyProductFilters,
        clearAllFilters,
    } = filters

    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [createOpen, setCreateOpen] = React.useState(false)
    const [reviseTarget, setReviseTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyProduct, setSupplyProduct] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyDialogSku, setSupplyDialogSku] =
        React.useState<FixedSku | null>(null)
    const [exportMeta, setExportMeta] = React.useState<{
        jobId: string
        rowCount: number
        filterSnapshotLabel: string
    } | null>(null)
    const [listingError, setListingError] = React.useState<string | null>(null)

    // 切换资源时重置本地 UI 状态（筛选来自新 URL，天然为空）
    React.useEffect(() => {
        setPreviewId(null)
        setExportMeta(null)
        setSupplyProduct(null)
        setSupplyDialogSku(null)
        setListingError(null)
    }, [resource])

    const listQuery = useMasterDataListQuery({
        resource,
        q: q.trim() || undefined,
        lifecycleStatus: isSellableResource ? undefined : lifecycleStatus,
        revisionTiming: isSellableResource ? undefined : revisionTiming,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        supplyRegion,
        productListingStatus,
        productSupplyCoverage,
        productSalesPriceMin,
        productSalesPriceMax,
        supplierCapabilityCodes,
        supplierQualificationTypes,
        supplierQualificationHealth,
        // metricKey 只做展示不做筛选：指标与 ToggleGroup 共用 lifecycleStatus 状态源
        metricKey: undefined,
    })
    const productFilterOptionsQuery = useProductFilterOptionsQuery(
        isProductResource || isSellableResource,
    )
    const exportMutation = useMasterDataExportMutation()
    const productListingMutation = useProductListingMutation()
    const canUpdateProductListing =
        isProductResource &&
        hasPermission(accountQuery.data?.permissions, "product:update")

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )

    const previewDetailQuery = useMasterDataCenterQuery(
        resource,
        isSellableResource ? "" : (previewId ?? ""),
    )

    const previewRow = React.useMemo(
        () => rows.find((r) => r.stableId === previewId) ?? null,
        [previewId, rows],
    )

    const pageRows = React.useMemo(() => {
        const start = pagination.pageIndex * pagination.pageSize
        return rows.slice(start, start + pagination.pageSize)
    }, [pagination.pageIndex, pagination.pageSize, rows])

    const productPageIds = React.useMemo(
        () => (isProductResource ? pageRows.map((row) => row.stableId) : []),
        [isProductResource, pageRows],
    )
    const productSkusQuery = useProductListSkusQuery(productPageIds)
    const productSkusByProduct = React.useMemo(() => {
        const grouped = new Map<string, ProductListSkuSummary[]>()
        for (const sku of productSkusQuery.data ?? []) {
            const skus = grouped.get(sku.productId) ?? []
            skus.push(sku)
            grouped.set(sku.productId, skus)
        }
        return grouped
    }, [productSkusQuery.data])
    const productPageSkuIds = React.useMemo(
        () => (productSkusQuery.data ?? []).map((sku) => sku.skuId),
        [productSkusQuery.data],
    )
    const supplierOfferingsQuery =
        useSupplierOfferingsForSkusQuery(productPageSkuIds)
    const currentSupplySkuIds = React.useMemo(
        () =>
            new Set(
                (supplierOfferingsQuery.data ?? [])
                    .filter(
                        (offering) =>
                            offering.status === "ACTIVE" &&
                            Boolean(offering.current_revision_id),
                    )
                    .map((offering) => offering.sku_id),
            ),
        [supplierOfferingsQuery.data],
    )
    const selectedCategoryLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.categories.find(
                (option) => option.categoryId === productCategoryId,
            )?.categoryName ?? productCategoryId,
        [productCategoryId, productFilterOptionsQuery.data?.categories],
    )
    const selectedBrandLabel = React.useMemo(
        () =>
            productFilterOptionsQuery.data?.brands.find(
                (option) => option.value === productBrandId,
            )?.label ?? productBrandId,
        [productBrandId, productFilterOptionsQuery.data?.brands],
    )
    const selectedSupplierLabel = React.useMemo(
        () =>
            (productFilterOptionsQuery.data?.suppliers ?? []).find(
                (option) => option.value === productSupplierId,
            )?.label ?? productSupplierId,
        [productFilterOptionsQuery.data?.suppliers, productSupplierId],
    )

    const syncedMetrics = React.useMemo(() => {
        const base = listQuery.data?.metrics ?? []
        if (rows.length === 0 || listQuery.data == null) return base
        return syncListMetrics(base, rows)
    }, [listQuery.data, rows])

    const filterSnapshotLabel = React.useMemo(
        () =>
            buildFilterSnapshotLabel({
                resource,
                q,
                isSellableResource,
                isSupplierResource,
                lifecycleStatus,
                revisionTiming,
                productKind,
                productListingStatus,
                productSupplyCoverage,
                productSalesPriceMin,
                productSalesPriceMax,
                supplyRegion,
                supplierCapabilityCodes,
                supplierQualificationTypes,
                supplierQualificationHealth,
                selectedCategoryLabel,
                selectedBrandLabel,
                selectedSupplierLabel,
            }),
        [
            isSellableResource,
            lifecycleStatus,
            productKind,
            productListingStatus,
            productSalesPriceMax,
            productSalesPriceMin,
            productSupplyCoverage,
            q,
            resource,
            revisionTiming,
            isSupplierResource,
            supplyRegion,
            supplierCapabilityCodes,
            supplierQualificationHealth,
            supplierQualificationTypes,
            selectedBrandLabel,
            selectedCategoryLabel,
            selectedSupplierLabel,
        ],
    )

    const sellableTableDescription = React.useMemo(() => {
        if (!isSellableResource) return null
        return buildSellableTableDescription({
            q,
            productKind,
            productSalesPriceMin,
            productSalesPriceMax,
            supplyRegion,
            selectedCategoryLabel,
            selectedBrandLabel,
            selectedSupplierLabel,
            rowCount: rows.length,
        })
    }, [
        isSellableResource,
        productKind,
        productSalesPriceMax,
        productSalesPriceMin,
        q,
        rows.length,
        selectedBrandLabel,
        selectedCategoryLabel,
        selectedSupplierLabel,
        supplyRegion,
    ])

    const listTableDescription = React.useMemo(() => {
        if (isProductResource || isSupplierResource || isSellableResource) {
            return null
        }
        return buildListTableDescription({
            q,
            lifecycleStatus,
            revisionTiming,
            rowCount: rows.length,
        })
    }, [
        isProductResource,
        isSellableResource,
        isSupplierResource,
        lifecycleStatus,
        q,
        revisionTiming,
        rows.length,
    ])

    const handleExport = React.useCallback(async () => {
        if (!listQuery.data || rows.length === 0) return
        const refreshed = await exportMutation.mutateAsync({
            resource,
            q: q.trim() || undefined,
            lifecycleStatus: isSellableResource ? undefined : lifecycleStatus,
            revisionTiming: isSellableResource ? undefined : revisionTiming,
            productKind,
            productCategoryId,
            productBrandId,
            productSupplierId,
            supplyRegion,
            productListingStatus,
            productSupplyCoverage,
            productSalesPriceMin,
            productSalesPriceMax,
            supplierCapabilityCodes,
            supplierQualificationTypes,
            supplierQualificationHealth,
        })
        const exportRows = refreshed.rows
        if (exportRows.length === 0) return
        const csv = buildMasterDataExportCsv(exportRows, filterSnapshotLabel)
        downloadCsv(csv, `基础资料-${resourceLabel(resource)}`)
        const datePart = new Date().toISOString().slice(0, 10).replace(/-/g, "")
        setExportMeta({
            jobId: `导出-${datePart}-${String(Date.now() % 100000).padStart(5, "0")}`,
            rowCount: exportRows.length,
            filterSnapshotLabel,
        })
    }, [
        exportMutation,
        filterSnapshotLabel,
        isSellableResource,
        lifecycleStatus,
        listQuery.data,
        productBrandId,
        productCategoryId,
        productKind,
        productListingStatus,
        productSupplierId,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplyCoverage,
        q,
        resource,
        revisionTiming,
        rows.length,
        supplyRegion,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
    ])

    const updateProductListing = React.useCallback(
        async (item: MasterDataListItem, listed: boolean) => {
            if (
                !listed &&
                !window.confirm(
                    `下架后，商品「${item.name}」下的全部 SKU 都会退出公司商品池。确定继续？`,
                )
            ) {
                return
            }
            setListingError(null)
            try {
                await productListingMutation.mutateAsync({
                    productId: item.stableId,
                    listingStatus: listed ? "LISTED" : "UNLISTED",
                })
            } catch (error) {
                setListingError(
                    getErrorMessage(error, "上架状态更新失败，请刷新后重试。"),
                )
            }
        },
        [productListingMutation],
    )

    const columns = useMasterDataColumns({
        isProductResource,
        isSupplierResource,
        isBrandResource,
        isUnitOfMeasureResource,
        isVoucherCategoryResource,
        isSellableResource,
        canUpdateProductListing,
        currentSupplySkuIds,
        lastFocusedRowId,
        productSkusByProduct,
        productSkusPending: productSkusQuery.isPending,
        productSkusError: productSkusQuery.isError,
        productListingPending: productListingMutation.isPending,
        productListingProductId: productListingMutation.variables?.productId,
        resource,
        rows,
        showEffectiveColumn,
        supplierOfferingsPending: supplierOfferingsQuery.isPending,
        supplierOfferingsError: supplierOfferingsQuery.isError,
        onUpdateProductListing: updateProductListing,
        onSupplyProduct: setSupplyProduct,
        onReviseTarget: setReviseTarget,
        onDisableTarget: setDisableTarget,
        onPreview: setPreviewId,
        onNavigate: router.push,
    })

    return {
        router,
        accountQuery,
        isProductResource,
        isSupplierResource,
        isBrandResource,
        isVoucherCategoryResource,
        isUnitOfMeasureResource,
        isSellableResource,
        isWarehouse,
        skipPreviewSheet,
        showEffectiveColumn,
        canCreate,
        createBlockedReason,
        filters,
        previewId,
        setPreviewId,
        createOpen,
        setCreateOpen,
        reviseTarget,
        setReviseTarget,
        disableTarget,
        setDisableTarget,
        supplyProduct,
        setSupplyProduct,
        supplyDialogSku,
        setSupplyDialogSku,
        exportMeta,
        setExportMeta,
        listingError,
        setListingError,
        listQuery,
        productFilterOptionsQuery,
        exportMutation,
        productListingMutation,
        canUpdateProductListing,
        rows,
        previewDetailQuery,
        previewRow,
        pageRows,
        productPageSkuIds,
        productSkusByProduct,
        productSkusQuery,
        supplierOfferingsQuery,
        currentSupplySkuIds,
        selectedCategoryLabel,
        selectedBrandLabel,
        selectedSupplierLabel,
        syncedMetrics,
        filterSnapshotLabel,
        sellableTableDescription,
        listTableDescription,
        handleExport,
        updateProductListing,
        columns,
    }
}

export type MasterDataListWorkspaceState = ReturnType<
    typeof useMasterDataListWorkspace
>
