"use client"

import { useChangeConfirmation } from "./use-change-confirmation"

import * as React from "react"
import type { SortingState } from "@tanstack/react-table"

import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { usePersonDirectorySelected } from "@/features/entity-selectors/hooks/person-directory"
import { useCreatePermission } from "@/features/master-data/hooks/use-create-permission"
import { useClientPagedRows } from "@/features/master-data/hooks/use-client-paged-rows"
import { useProductListFilters } from "@/features/master-data/hooks/use-product-list-filters"
import type { ProductFilterKey } from "@/features/master-data/hooks/use-product-list-filters"
import { useProductListSelection } from "@/features/master-data/hooks/use-product-list-selection"
import {
    useBatchProductListingMutation,
    useMasterDataListQuery,
    useProductFilterOptionsQuery,
    useProductListSkusQuery,
    useProductListingMutation,
} from "@/features/master-data/hooks/queries"
import { useMasterDataListExport } from "@/features/master-data/hooks/use-master-data-list-export"
import {
    partitionProductsForBatchListing,
    type BatchProductListingProgress,
    type BatchProductListingResult,
    type BatchProductListingSkippedItem,
} from "@/features/master-data/lib/product-batch-listing"
import {
    buildProductFilterSnapshotLabel,
    buildProductTableDescription,
} from "@/features/master-data/lib/master-data-list-summaries"
import { syncListMetrics } from "@/features/master-data/lib/master-data-list-summaries"
import {
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
    selectedCsvValues,
} from "@/features/master-data/lib/list-filters"
import { revisionTimingFilterLabel } from "@/features/master-data/lib/copy"
import {
    directoryBrandLabel,
    directoryCategoryLabel,
    directorySupplierLabel,
    productFilterDirectoryState,
} from "@/features/master-data/lib/product-filter-directory"
import { resourceLabel } from "@/features/master-data/lib/data"
import type {
    MasterDataListItem,
    ProductListSkuSummary,
} from "@/features/master-data/types"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import { useSupplierOfferingsForSkusQuery } from "@/features/supplier-offerings/queries"
import type { FixedSku } from "@/features/supplier-offerings/types"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"

export type ProductAppliedChip = Readonly<{
    key: ProductFilterKey
    label: string
}>

const zhCollator = new Intl.Collator("zh-CN", { numeric: true })

function formatSelectedPeopleSummary(
    ids: readonly string[],
    items: readonly { id: string; name: string }[] | undefined,
): string | undefined {
    if (ids.length === 0) return undefined
    const namesById = new Map(
        (items ?? []).map((item) => [
            item.id,
            item.name.trim() || "未命名人员",
        ]),
    )
    const resolvedNames = ids
        .map((id) => namesById.get(id))
        .filter((name): name is string => Boolean(name))
    if (resolvedNames.length === 0) return `已选 ${ids.length} 人`
    if (ids.length === 1) return resolvedNames[0]
    if (ids.length === 2 && resolvedNames.length === 2) {
        return `${resolvedNames[0]}、${resolvedNames[1]}`
    }
    return `${resolvedNames[0]} 等 ${ids.length} 人`
}

function sortProductRows(
    rows: readonly MasterDataListItem[],
    sorting: SortingState,
): readonly MasterDataListItem[] {
    const active = sorting[0]
    if (!active || rows.length <= 1) return rows
    const direction = active.desc ? -1 : 1
    return [...rows].sort((left, right) => {
        if (active.id === "name") {
            return zhCollator.compare(left.name, right.name) * direction
        }
        if (active.id === "stableNo") {
            return zhCollator.compare(left.stableNo, right.stableNo) * direction
        }
        if (active.id === "skuCount") {
            const diff = (left.skuCount ?? 0) - (right.skuCount ?? 0)
            return diff !== 0
                ? diff * direction
                : zhCollator.compare(left.name, right.name)
        }
        if (active.id === "maintainer") {
            const leftOwner = left.ownerName?.trim() ?? ""
            const rightOwner = right.ownerName?.trim() ?? ""
            if (!leftOwner && !rightOwner) {
                return zhCollator.compare(left.name, right.name)
            }
            if (!leftOwner) return 1
            if (!rightOwner) return -1
            const diff = zhCollator.compare(leftOwner, rightOwner)
            return diff !== 0
                ? diff * direction
                : zhCollator.compare(left.name, right.name)
        }
        return 0
    })
}

export function useProductListState(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const listingConfirmation = useChangeConfirmation(
        "master-data-product-listing",
    )
    const { canCreate, createBlockedReason } =
        useCreatePermission("product:create")
    const accountQuery = useAccountProfileQuery()
    const permissions = accountQuery.isSuccess
        ? accountQuery.data.permissions
        : undefined
    const canReadProductDetail = hasPermission(permissions, "product:detail")
    const canReadSkus =
        hasPermission(permissions, "sku:list") &&
        hasPermission(permissions, "sku_revision:list")
    const canReadUnits = hasPermission(permissions, "unit_of_measure:list")
    const canReadOfferings =
        canReadSkus && hasPermission(permissions, "supplier_offering:list")
    const canAddSupply =
        canReadSkus && hasPermission(permissions, "supplier_offering:create")
    const filters = useProductListFilters(searchInputRef)
    const listQueryInput = React.useMemo(
        () => ({
            resource: "products" as const,
            q: filters.q.trim() || undefined,
            lifecycleStatus: filters.lifecycleStatus,
            revisionTiming: filters.revisionTiming,
            productKind: filters.productKind,
            productCategoryId: filters.productCategoryId,
            productBrandId: filters.productBrandId,
            productSupplierId: filters.productSupplierId,
            productListingStatus: filters.productListingStatus,
            productSupplyCoverage: filters.productSupplyCoverage,
            productSourceDiffPending:
                filters.productSourceDiffPending || undefined,
            productSalesPriceMin: filters.productSalesPriceMin,
            productSalesPriceMax: filters.productSalesPriceMax,
            ownerUserIds: filters.ownerUserIds,
            procurementOwnerUserIds: filters.procurementOwnerUserIds,
        }),
        [
            filters.lifecycleStatus,
            filters.ownerUserIds,
            filters.procurementOwnerUserIds,
            filters.productBrandId,
            filters.productCategoryId,
            filters.productKind,
            filters.productListingStatus,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.productSourceDiffPending,
            filters.productSupplierId,
            filters.productSupplyCoverage,
            filters.q,
            filters.revisionTiming,
        ],
    )
    const selectionResetKey = React.useMemo(
        () => JSON.stringify(listQueryInput),
        [listQueryInput],
    )
    const listQuery = useMasterDataListQuery(listQueryInput)
    const productFilterOptionsQuery = useProductFilterOptionsQuery(true)
    const { exportMeta, exportPending, handleExport } =
        useMasterDataListExport()
    const productListingMutation = useProductListingMutation()
    const batchProductListingMutation = useBatchProductListingMutation()
    const canUpdateProductListing = hasPermission(permissions, "product:update")
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyProduct, setSupplyProduct] =
        React.useState<MasterDataListItem | null>(null)
    const [supplyDialogSkus, setSupplyDialogSkus] = React.useState<
        readonly FixedSku[] | null
    >(null)
    const [listingError, setListingError] = React.useState<string | null>(null)
    const [sorting, setSorting] = React.useState<SortingState>([])
    const [batchListingOpen, setBatchListingOpen] = React.useState(false)
    const [batchListingProgress, setBatchListingProgress] =
        React.useState<BatchProductListingProgress | null>(null)
    const [batchListingResult, setBatchListingResult] =
        React.useState<BatchProductListingResult | null>(null)
    const batchListingRunningRef = React.useRef(false)
    const clearListingError = React.useCallback(() => setListingError(null), [])
    const onSortingChange = React.useCallback(
        (next: SortingState) => {
            setSorting(next)
            if (filters.pagination.pageIndex !== 0) {
                filters.changePagination({
                    pageIndex: 0,
                    pageSize: filters.pagination.pageSize,
                })
            }
        },
        [filters],
    )

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const sortedRows = React.useMemo(
        () => sortProductRows(rows, sorting),
        [rows, sorting],
    )
    const pageRows = useClientPagedRows(sortedRows, filters.pagination)
    const selection = useProductListSelection({
        pageRows,
        latestRows: rows,
        resetKey: selectionResetKey,
    })
    const batchListingPartition = React.useMemo(
        () => partitionProductsForBatchListing(selection.selectedRows),
        [selection.selectedRows],
    )
    const productPageIds = React.useMemo(
        () => pageRows.map((row) => row.stableId),
        [pageRows],
    )
    const productSkusQuery = useProductListSkusQuery(
        canReadSkus ? productPageIds : [],
        canReadUnits,
    )
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
    const supplierOfferingsQuery = useSupplierOfferingsForSkusQuery(
        canReadOfferings ? productPageSkuIds : [],
    )
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
    const ownerIds = React.useMemo(
        () => selectedCsvValues(filters.ownerUserIds ?? null),
        [filters.ownerUserIds],
    )
    const procurementOwnerIds = React.useMemo(
        () => selectedCsvValues(filters.procurementOwnerUserIds ?? null),
        [filters.procurementOwnerUserIds],
    )
    const selectedOwnersQuery = usePersonDirectorySelected(
        "business",
        ownerIds,
        filters.canFilterOwner,
    )
    const selectedProcurementOwnersQuery = usePersonDirectorySelected(
        "procurement",
        procurementOwnerIds,
        filters.canFilterProcurementOwner,
    )
    const selectedOwnerLabel = React.useMemo(
        () =>
            formatSelectedPeopleSummary(
                ownerIds,
                selectedOwnersQuery.data?.items,
            ),
        [ownerIds, selectedOwnersQuery.data?.items],
    )
    const selectedProcurementOwnerLabel = React.useMemo(
        () =>
            formatSelectedPeopleSummary(
                procurementOwnerIds,
                selectedProcurementOwnersQuery.data?.items,
            ),
        [procurementOwnerIds, selectedProcurementOwnersQuery.data?.items],
    )
    const directory = productFilterDirectoryState(productFilterOptionsQuery)
    const selectedCategoryLabel = directoryCategoryLabel(
        directory,
        filters.productCategoryId,
    )
    const selectedBrandLabel = directoryBrandLabel(
        directory,
        filters.productBrandId,
    )
    const selectedSupplierLabel = directorySupplierLabel(
        directory,
        filters.productSupplierId,
    )
    /** 所有已生效条件均可从 chip 单独撤销。 */
    const appliedChips = React.useMemo<readonly ProductAppliedChip[]>(() => {
        const chips: ProductAppliedChip[] = []
        if (filters.q.trim()) {
            chips.push({ key: "q", label: `搜索：${filters.q.trim()}` })
        }
        if (filters.productKind) {
            chips.push({
                key: "productKind",
                label: `类型：${PRODUCT_KIND_LABELS[filters.productKind]}`,
            })
        }

        if (filters.revisionTiming !== "all") {
            chips.push({
                key: "revisionTiming",
                label: `版本：${revisionTimingFilterLabel(filters.revisionTiming)}`,
            })
        }
        if (filters.productListingStatus) {
            chips.push({
                key: "productListingStatus",
                label: `上架：${
                    PRODUCT_LISTING_FILTER_OPTIONS.find(
                        (option) =>
                            option.value === filters.productListingStatus,
                    )?.label ?? filters.productListingStatus
                }`,
            })
        }
        if (filters.productSupplyCoverage) {
            chips.push({
                key: "productSupplyCoverage",
                label: `供给覆盖：${
                    PRODUCT_COVERAGE_FILTER_OPTIONS.find(
                        (option) =>
                            option.value === filters.productSupplyCoverage,
                    )?.label ?? filters.productSupplyCoverage
                }`,
            })
        }
        if (filters.productSourceDiffPending) {
            chips.push({
                key: "productSourceDiffPending",
                label: "资料更新：待确认",
            })
        }
        if (filters.productCategoryId) {
            chips.push({
                key: "productCategoryId",
                label: `分类：${selectedCategoryLabel}`,
            })
        }
        if (filters.productBrandId) {
            chips.push({
                key: "productBrandId",
                label: `品牌：${selectedBrandLabel}`,
            })
        }
        if (filters.productSupplierId) {
            chips.push({
                key: "productSupplierId",
                label: `供应商：${selectedSupplierLabel}`,
            })
        }
        if (selectedOwnerLabel) {
            chips.push({
                key: "ownerUserIds",
                label: `维护人：${selectedOwnerLabel}`,
            })
        }
        if (selectedProcurementOwnerLabel) {
            chips.push({
                key: "procurementOwnerUserIds",
                label: `采购负责人：${selectedProcurementOwnerLabel}`,
            })
        }
        if (filters.productSalesPriceMin || filters.productSalesPriceMax) {
            const minimum = filters.productSalesPriceMin ?? "不限"
            const maximum = filters.productSalesPriceMax ?? "不限"
            chips.push({
                key: "salesPrice",
                label: `含税售价：${minimum} 至 ${maximum}`,
            })
        }
        return chips
    }, [
        filters.productBrandId,
        filters.productCategoryId,
        filters.productKind,
        filters.productListingStatus,
        filters.productSalesPriceMax,
        filters.productSalesPriceMin,
        filters.productSupplierId,
        filters.productSupplyCoverage,
        filters.productSourceDiffPending,
        filters.q,
        filters.revisionTiming,
        selectedBrandLabel,
        selectedCategoryLabel,
        selectedOwnerLabel,
        selectedProcurementOwnerLabel,
        selectedSupplierLabel,
    ])
    const listTableDescription = React.useMemo(
        () =>
            buildProductTableDescription({
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
                productKind: filters.productKind,
                productListingStatus: filters.productListingStatus,
                productSupplyCoverage: filters.productSupplyCoverage,
                productSourceDiffPending: filters.productSourceDiffPending,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                selectedCategoryLabel,
                selectedBrandLabel,
                selectedSupplierLabel,
                selectedOwnerLabel,
                selectedProcurementOwnerLabel,
            }),
        [
            filters.lifecycleStatus,
            filters.productKind,
            filters.productListingStatus,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.productSupplyCoverage,
            filters.productSourceDiffPending,
            filters.q,
            filters.revisionTiming,
            selectedBrandLabel,
            selectedCategoryLabel,
            selectedOwnerLabel,
            selectedProcurementOwnerLabel,
            selectedSupplierLabel,
        ],
    )
    const syncedMetrics = React.useMemo(() => {
        const base = listQuery.data?.metrics ?? []
        if (rows.length === 0 || listQuery.data == null) return base
        return syncListMetrics(base, rows)
    }, [listQuery.data, rows])
    const filterSnapshotLabel = React.useMemo(
        () =>
            buildProductFilterSnapshotLabel({
                q: filters.q,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
                productKind: filters.productKind,
                productListingStatus: filters.productListingStatus,
                productSupplyCoverage: filters.productSupplyCoverage,
                productSourceDiffPending: filters.productSourceDiffPending,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                selectedCategoryLabel,
                selectedBrandLabel,
                selectedSupplierLabel,
                selectedOwnerLabel,
                selectedProcurementOwnerLabel,
            }),
        [
            filters.lifecycleStatus,
            filters.productKind,
            filters.productListingStatus,
            filters.productSalesPriceMax,
            filters.productSalesPriceMin,
            filters.productSupplyCoverage,
            filters.productSourceDiffPending,
            filters.q,
            filters.revisionTiming,
            selectedBrandLabel,
            selectedCategoryLabel,
            selectedOwnerLabel,
            selectedProcurementOwnerLabel,
            selectedSupplierLabel,
        ],
    )

    const onExport = React.useCallback(() => {
        if (!listQuery.data || rows.length === 0) return
        void handleExport(
            {
                resource: "products",
                q: filters.q.trim() || undefined,
                lifecycleStatus: filters.lifecycleStatus,
                revisionTiming: filters.revisionTiming,
                productKind: filters.productKind,
                productCategoryId: filters.productCategoryId,
                productBrandId: filters.productBrandId,
                productSupplierId: filters.productSupplierId,
                productListingStatus: filters.productListingStatus,
                productSupplyCoverage: filters.productSupplyCoverage,
                productSourceDiffPending:
                    filters.productSourceDiffPending || undefined,
                productSalesPriceMin: filters.productSalesPriceMin,
                productSalesPriceMax: filters.productSalesPriceMax,
                ownerUserIds: filters.ownerUserIds,
                procurementOwnerUserIds: filters.procurementOwnerUserIds,
            },
            filterSnapshotLabel,
            resourceLabel("products"),
        )
    }, [
        filterSnapshotLabel,
        filters,
        handleExport,
        listQuery.data,
        rows.length,
    ])

    const applyProductListing = React.useCallback(
        async (item: MasterDataListItem, listed: boolean) => {
            setListingError(null)
            try {
                await productListingMutation.mutateAsync({
                    productId: item.stableId,
                    listingStatus: listed ? "LISTED" : "UNLISTED",
                })
            } catch (error) {
                const message = getErrorMessage(
                    error,
                    "上架状态更新失败，请刷新后重试。",
                )
                setListingError(message)
                if (!listed) throw new Error(message)
            }
        },
        [productListingMutation],
    )

    const updateProductListing = async (
        item: MasterDataListItem,
        listed: boolean,
    ) => {
        if (listed) return applyProductListing(item, true)
        listingConfirmation.confirm({
            title: "下架商品",
            description: `商品「${item.name}」下的全部 SKU 将退出公司商品池。`,
            confirmLabel: "下架商品",
            destructive: true,
            onConfirm: () => applyProductListing(item, false),
        })
    }

    const openBatchListingDialog = React.useCallback(() => {
        setBatchListingResult(null)
        setBatchListingProgress(null)
        setBatchListingOpen(true)
    }, [])

    const closeBatchListingDialog = React.useCallback(() => {
        if (batchListingRunningRef.current) return
        setBatchListingOpen(false)
        setBatchListingResult(null)
        setBatchListingProgress(null)
    }, [])

    const runBatchListing = React.useCallback(
        async (
            actionable: readonly MasterDataListItem[],
            skipped: readonly BatchProductListingSkippedItem[],
            isRetry: boolean,
        ) => {
            if (batchListingRunningRef.current || actionable.length === 0)
                return
            batchListingRunningRef.current = true
            setListingError(null)
            setBatchListingProgress({ completed: 0, total: actionable.length })
            try {
                const result = await batchProductListingMutation.mutateAsync({
                    items: actionable,
                    skipped,
                    listingStatus: "LISTED",
                    onProgress: setBatchListingProgress,
                })
                if (result.failed.length === 0) {
                    selection.clear()
                    setBatchListingOpen(false)
                    setBatchListingResult(null)
                    toast.add({
                        title: "批量上架完成",
                        description: isRetry
                            ? `已重新上架 ${result.succeeded.length} 个商品。`
                            : result.skipped.length > 0
                              ? `已上架 ${result.succeeded.length} 个商品，跳过 ${result.skipped.length} 个无需或不可上架商品。`
                              : `已上架 ${result.succeeded.length} 个商品，所选商品下启用中的 SKU 已设为已上架。`,
                        type: "success",
                        timeout: 5000,
                    })
                    return
                }
                selection.retainOnly(
                    result.failed.map((item) => item.productId),
                )
                setBatchListingResult(result)
            } finally {
                batchListingRunningRef.current = false
                setBatchListingProgress(null)
            }
        },
        [batchProductListingMutation, selection],
    )

    const submitBatchListing = React.useCallback(
        (
            actionable: readonly MasterDataListItem[],
            skipped: readonly BatchProductListingSkippedItem[],
        ) => runBatchListing(actionable, skipped, false),
        [runBatchListing],
    )

    const retryFailedBatchListing = React.useCallback(
        (failedRows: readonly MasterDataListItem[]) =>
            runBatchListing(failedRows, [], true),
        [runBatchListing],
    )

    return {
        listingConfirmationDialog: listingConfirmation.dialog,
        filters,
        listQuery,
        productFilterOptionsQuery,
        exportMeta,
        exportPending,
        productListingMutation,
        batchProductListingMutation,
        canCreate,
        createBlockedReason,
        canUpdateProductListing,
        canReadProductDetail,
        canReadSkus,
        canReadUnits,
        canReadOfferings,
        canAddSupply,
        disableTarget,
        setDisableTarget,
        supplyProduct,
        setSupplyProduct,
        supplyDialogSkus,
        setSupplyDialogSkus,
        listingError,
        clearListingError,
        sorting,
        onSortingChange,
        rows,
        pageRows,
        selection,
        batchListingPartition,
        batchListingOpen,
        batchListingProgress,
        batchListingResult,
        openBatchListingDialog,
        closeBatchListingDialog,
        submitBatchListing,
        retryFailedBatchListing,
        productPageSkuIds,
        productSkusByProduct,
        productSkusQuery,
        supplierOfferingsQuery,
        currentSupplySkuIds,
        appliedChips,
        filterSnapshotLabel,
        listTableDescription,
        syncedMetrics,
        onExport,
        updateProductListing,
    }
}
