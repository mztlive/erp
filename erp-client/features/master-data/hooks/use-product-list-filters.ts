"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

import { useListUrl } from "@/features/master-data/hooks/use-list-url"
import {
    changeListLifecycle,
    commitTrimmedListSearch,
} from "@/features/master-data/lib/list-url-writes"
import { useSearchDraft } from "@/lib/use-search-draft"
import {
    parseLifecycleStatus,
    parseRevisionTiming,
    productSalesPriceRangeError,
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import {
    PRODUCT_KIND_VALUES,
    type ProductKind,
    type ProductListingFilter,
    type ProductSkuCoverageFilter,
} from "@/features/master-data/types"

/** 可被单独移除的已生效条件。 */
export type ProductFilterKey =
    | "q"
    | "productKind"
    | "revisionTiming"
    | "productListingStatus"
    | "productSupplyCoverage"
    | "productSourceDiffPending"
    | "productCategoryId"
    | "productBrandId"
    | "productSupplierId"
    | "salesPrice"
    | "ownerUserIds"
    | "procurementOwnerUserIds"

/** 商品列表：搜索 + 类型 / 启停 / 版本 / 上架 / 供给 / 归属 / 售价。 */
export function useProductListFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const account = useAccountProfileQuery()
    const permissions = account.isSuccess ? account.data.permissions : undefined
    const canFilterCategory = hasPermission(
        permissions,
        "product_category:list",
    )
    const canFilterBrand = hasPermission(permissions, "product_brand:list")
    const canFilterSupplier = hasPermission(permissions, "supplier:list")
    const canFilterOwner = hasPermission(permissions, "business_person:list")
    const canFilterProcurementOwner = hasPermission(
        permissions,
        "procurement_person:list",
    )
    const {
        searchParams,
        patchUrl,
        q,
        pagination,
        setPagination,
        resetPagination,
        changePagination,
    } = useListUrl(
        {
            productCategoryId: !canFilterCategory,
            productBrandId: !canFilterBrand,
            productSupplierId: !canFilterSupplier,
            ownerUserIds: !canFilterOwner,
            procurementOwnerUserIds: !canFilterProcurementOwner,
            orgUnitIds: true,
            includeDescendants: true,
        },
        account.isSuccess,
    )
    const { searchDraft, setSearchDraft } = useSearchDraft(q, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })

    const lifecycleStatus = parseLifecycleStatus(
        searchParams.get("lifecycleStatus"),
    )
    const revisionTiming = parseRevisionTiming(
        searchParams.get("revisionTiming"),
    )
    const productKind = PRODUCT_KIND_VALUES.find(
        (value) => value === searchParams.get("productKind"),
    )
    const productCategoryId = canFilterCategory
        ? searchParams.get("productCategoryId")?.trim() || undefined
        : undefined
    const productBrandId = canFilterBrand
        ? searchParams.get("productBrandId")?.trim() || undefined
        : undefined
    const productSupplierId = canFilterSupplier
        ? searchParams.get("productSupplierId")?.trim() || undefined
        : undefined
    const productListingStatus = PRODUCT_LISTING_FILTER_OPTIONS.find(
        (option) => option.value === searchParams.get("productListingStatus"),
    )?.value
    const productSupplyCoverage = PRODUCT_COVERAGE_FILTER_OPTIONS.find(
        (option) => option.value === searchParams.get("productSupplyCoverage"),
    )?.value
    const productSourceDiffPending =
        searchParams.get("productSourceDiffPending") === "true"
    const productSalesPriceMin =
        searchParams.get("productSalesPriceMin")?.trim() || undefined
    const productSalesPriceMax =
        searchParams.get("productSalesPriceMax")?.trim() || undefined
    const ownerUserIds = canFilterOwner
        ? searchParams.get("ownerUserIds")?.trim() || undefined
        : undefined
    const procurementOwnerUserIds = canFilterProcurementOwner
        ? searchParams.get("procurementOwnerUserIds")?.trim() || undefined
        : undefined
    const metricKey = lifecycleStatus
    const hasStructuredProductFilters = Boolean(
        productKind ||
        lifecycleStatus !== "all" ||
        revisionTiming !== "all" ||
        productCategoryId ||
        productBrandId ||
        productSupplierId ||
        productListingStatus ||
        productSupplyCoverage ||
        productSourceDiffPending ||
        productSalesPriceMin ||
        productSalesPriceMax ||
        ownerUserIds ||
        procurementOwnerUserIds,
    )

    // 深链只显示已生效标签，不自动打开筛选浮层。
    const [productFilterPanelOpen, setProductFilterPanelOpen] =
        React.useState(false)
    const [productKindDraft, setProductKindDraft] = React.useState<
        ProductKind | "all"
    >(productKind ?? "all")
    const [revisionTimingDraft, setRevisionTimingDraft] =
        React.useState(revisionTiming)
    const [productListingStatusDraft, setProductListingStatusDraft] =
        React.useState<ProductListingFilter | "all">(
            productListingStatus ?? "all",
        )
    const [productSupplyCoverageDraft, setProductSupplyCoverageDraft] =
        React.useState<ProductSkuCoverageFilter | "all">(
            productSupplyCoverage ?? "all",
        )
    const [productSourceDiffPendingDraft, setProductSourceDiffPendingDraft] =
        React.useState(productSourceDiffPending)
    const [productCategoryIdDraft, setProductCategoryIdDraft] = React.useState<
        string | null
    >(productCategoryId ?? null)
    const [productBrandIdDraft, setProductBrandIdDraft] = React.useState<
        string | null
    >(productBrandId ?? null)
    const [productSupplierIdDraft, setProductSupplierIdDraft] = React.useState<
        string | null
    >(productSupplierId ?? null)
    const [productSalesPriceMinDraft, setProductSalesPriceMinDraft] =
        React.useState(productSalesPriceMin ?? "")
    const [productSalesPriceMaxDraft, setProductSalesPriceMaxDraft] =
        React.useState(productSalesPriceMax ?? "")
    const [productSalesPriceError, setProductSalesPriceError] = React.useState<
        string | null
    >(null)
    const [ownerUserIdsDraft, setOwnerUserIdsDraft] = React.useState(
        ownerUserIds ?? "",
    )
    const [procurementOwnerUserIdsDraft, setProcurementOwnerUserIdsDraft] =
        React.useState(procurementOwnerUserIds ?? "")

    const commitSearch = React.useCallback(() => {
        commitTrimmedListSearch({
            searchDraft,
            appliedQuery: q,
            patchUrl,
            resetPagination,
        })
    }, [patchUrl, q, resetPagination, searchDraft])

    const changeLifecycle = React.useCallback(
        (next: "enabled" | "disabled" | "all") => {
            changeListLifecycle({
                next,
                current: lifecycleStatus,
                patchUrl,
                resetPagination,
            })
        },
        [lifecycleStatus, patchUrl, resetPagination],
    )

    const applyProductFilters = React.useCallback(() => {
        const minimum = productSalesPriceMinDraft.trim()
        const maximum = productSalesPriceMaxDraft.trim()
        const error = productSalesPriceRangeError(minimum, maximum)
        setProductSalesPriceError(error)
        if (error) {
            setProductFilterPanelOpen(true)
            return
        }
        patchUrl({
            q: searchDraft.trim() || null,
            productKind: productKindDraft === "all" ? null : productKindDraft,
            revisionTiming:
                revisionTimingDraft === "all" ? null : revisionTimingDraft,
            productListingStatus:
                productListingStatusDraft === "all"
                    ? null
                    : productListingStatusDraft,
            productSupplyCoverage:
                productSupplyCoverageDraft === "all"
                    ? null
                    : productSupplyCoverageDraft,
            productSourceDiffPending: productSourceDiffPendingDraft
                ? "true"
                : null,
            productCategoryId: canFilterCategory
                ? productCategoryIdDraft
                : null,
            productBrandId: canFilterBrand ? productBrandIdDraft : null,
            productSupplierId: canFilterSupplier
                ? productSupplierIdDraft
                : null,
            productSalesPriceMin: minimum || null,
            productSalesPriceMax: maximum || null,
            ownerUserIds: canFilterOwner
                ? ownerUserIdsDraft.trim() || null
                : null,
            procurementOwnerUserIds: canFilterProcurementOwner
                ? procurementOwnerUserIdsDraft.trim() || null
                : null,
            page: null,
        })
        resetPagination()
        setProductFilterPanelOpen(false)
    }, [
        canFilterCategory,
        canFilterBrand,
        canFilterSupplier,
        canFilterOwner,
        canFilterProcurementOwner,
        patchUrl,
        productBrandIdDraft,
        productCategoryIdDraft,
        productKindDraft,
        productListingStatusDraft,
        productSalesPriceMaxDraft,
        productSalesPriceMinDraft,
        productSupplierIdDraft,
        productSupplyCoverageDraft,
        productSourceDiffPendingDraft,
        ownerUserIdsDraft,
        procurementOwnerUserIdsDraft,
        resetPagination,
        revisionTimingDraft,
        searchDraft,
    ])

    /** 移除单个已生效条件；一件代发价按区间整体移除。 */
    const removeFilter = React.useCallback(
        (key: ProductFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "productKind") setProductKindDraft("all")
            if (key === "revisionTiming") setRevisionTimingDraft("all")
            if (key === "productListingStatus") {
                setProductListingStatusDraft("all")
            }
            if (key === "productSupplyCoverage") {
                setProductSupplyCoverageDraft("all")
            }
            if (key === "productSourceDiffPending") {
                setProductSourceDiffPendingDraft(false)
            }
            if (key === "productCategoryId") setProductCategoryIdDraft(null)
            if (key === "productBrandId") setProductBrandIdDraft(null)
            if (key === "productSupplierId") setProductSupplierIdDraft(null)
            if (key === "salesPrice") {
                setProductSalesPriceMinDraft("")
                setProductSalesPriceMaxDraft("")
                setProductSalesPriceError(null)
            }
            if (key === "ownerUserIds") setOwnerUserIdsDraft("")
            if (key === "procurementOwnerUserIds") {
                setProcurementOwnerUserIdsDraft("")
            }
            patchUrl(
                key === "salesPrice"
                    ? {
                          productSalesPriceMin: null,
                          productSalesPriceMax: null,
                          page: null,
                      }
                    : { [key]: null, page: null },
            )
            resetPagination()
        },
        [patchUrl, resetPagination, setSearchDraft],
    )

    /** 仅重置更多条件草稿；保留关键词、类型、分类及当前查询结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setRevisionTimingDraft("all")
        setProductListingStatusDraft("all")
        setProductSupplyCoverageDraft("all")
        setProductSourceDiffPendingDraft(false)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft("")
        setProcurementOwnerUserIdsDraft("")
    }, [])

    /** 取消、外点和 Esc 只恢复低频草稿，保留搜索、类型和分类。 */
    const cancelMoreFilters = React.useCallback(() => {
        setRevisionTimingDraft(revisionTiming)
        setProductListingStatusDraft(productListingStatus ?? "all")
        setProductSupplyCoverageDraft(productSupplyCoverage ?? "all")
        setProductSourceDiffPendingDraft(productSourceDiffPending)
        setProductBrandIdDraft(productBrandId ?? null)
        setProductSupplierIdDraft(productSupplierId ?? null)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft(ownerUserIds ?? "")
        setProcurementOwnerUserIdsDraft(procurementOwnerUserIds ?? "")
        setProductFilterPanelOpen(false)
    }, [
        ownerUserIds,
        procurementOwnerUserIds,
        productBrandId,
        productListingStatus,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplierId,
        productSupplyCoverage,
        productSourceDiffPending,
        revisionTiming,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        productKindDraft !== (productKind ?? "all") ||
        revisionTimingDraft !== revisionTiming ||
        productListingStatusDraft !== (productListingStatus ?? "all") ||
        productSupplyCoverageDraft !== (productSupplyCoverage ?? "all") ||
        productSourceDiffPendingDraft !== productSourceDiffPending ||
        (canFilterCategory &&
            productCategoryIdDraft !== (productCategoryId ?? null)) ||
        (canFilterBrand && productBrandIdDraft !== (productBrandId ?? null)) ||
        (canFilterSupplier &&
            productSupplierIdDraft !== (productSupplierId ?? null)) ||
        productSalesPriceMinDraft.trim() !== (productSalesPriceMin ?? "") ||
        productSalesPriceMaxDraft.trim() !== (productSalesPriceMax ?? "") ||
        (canFilterOwner && ownerUserIdsDraft !== (ownerUserIds ?? "")) ||
        (canFilterProcurementOwner &&
            procurementOwnerUserIdsDraft !== (procurementOwnerUserIds ?? ""))

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setProductKindDraft("all")
        setRevisionTimingDraft("all")
        setProductListingStatusDraft("all")
        setProductSupplyCoverageDraft("all")
        setProductSourceDiffPendingDraft(false)
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft("")
        setProcurementOwnerUserIdsDraft("")
        setProductFilterPanelOpen(false)
        patchUrl({
            q: null,
            lifecycleStatus: null,
            metricKey: null,
            revisionTiming: null,
            productKind: null,
            productCategoryId: null,
            productBrandId: null,
            productSupplierId: null,
            productListingStatus: null,
            productSupplyCoverage: null,
            productSourceDiffPending: null,
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            ownerUserIds: null,
            procurementOwnerUserIds: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination, setSearchDraft])

    React.useEffect(() => {
        setProductKindDraft(productKind ?? "all")
        setRevisionTimingDraft(revisionTiming)
        setProductListingStatusDraft(productListingStatus ?? "all")
        setProductSupplyCoverageDraft(productSupplyCoverage ?? "all")
        setProductSourceDiffPendingDraft(productSourceDiffPending)
        setProductCategoryIdDraft(productCategoryId ?? null)
        setProductBrandIdDraft(productBrandId ?? null)
        setProductSupplierIdDraft(productSupplierId ?? null)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft(ownerUserIds ?? "")
        setProcurementOwnerUserIdsDraft(procurementOwnerUserIds ?? "")
    }, [
        canFilterCategory,
        canFilterBrand,
        canFilterSupplier,
        canFilterOwner,
        canFilterProcurementOwner,
        lifecycleStatus,
        productBrandId,
        productCategoryId,
        productKind,
        productListingStatus,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplierId,
        productSupplyCoverage,
        productSourceDiffPending,
        ownerUserIds,
        procurementOwnerUserIds,
        revisionTiming,
    ])

    return {
        canFilterCategory,
        canFilterBrand,
        canFilterSupplier,
        canFilterOwner,
        canFilterProcurementOwner,
        q,
        lifecycleStatus,
        revisionTiming,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        productListingStatus,
        productSupplyCoverage,
        productSourceDiffPending,
        productSalesPriceMin,
        productSalesPriceMax,
        ownerUserIds,
        procurementOwnerUserIds,
        metricKey,
        hasStructuredProductFilters,
        searchDraft,
        setSearchDraft,
        productFilterPanelOpen,
        setProductFilterPanelOpen,
        productKindDraft,
        setProductKindDraft,
        revisionTimingDraft,
        setRevisionTimingDraft,
        productListingStatusDraft,
        setProductListingStatusDraft,
        productSupplyCoverageDraft,
        setProductSupplyCoverageDraft,
        productSourceDiffPendingDraft,
        setProductSourceDiffPendingDraft,
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
        productSalesPriceError,
        setProductSalesPriceError,
        ownerUserIdsDraft,
        setOwnerUserIdsDraft,
        procurementOwnerUserIdsDraft,
        setProcurementOwnerUserIdsDraft,
        hasPendingChanges,
        pagination,
        setPagination,
        changePagination,
        changeLifecycle,
        commitSearch,
        applyProductFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
    }
}
