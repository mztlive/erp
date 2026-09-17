"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
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
    | "productCategoryId"
    | "productBrandId"
    | "productSupplierId"
    | "salesPrice"
    | "ownerUserIds"
    | "procurementOwnerUserIds"
    | "orgUnitIds"

/** 商品列表：搜索 + 类型 / 启停 / 版本 / 上架 / 供给 / 归属 / 售价。 */
export function useProductListFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const {
        searchParams,
        patchUrl,
        q,
        pagination,
        setPagination,
        resetPagination,
        changePagination,
    } = useListUrl()
    const { searchDraft, setSearchDraft } = useSearchDraft(q, searchInputRef)

    const lifecycleStatus = parseLifecycleStatus(
        searchParams.get("lifecycleStatus"),
    )
    const revisionTiming = parseRevisionTiming(
        searchParams.get("revisionTiming"),
    )
    const productKind = PRODUCT_KIND_VALUES.find(
        (value) => value === searchParams.get("productKind"),
    )
    const productCategoryId =
        searchParams.get("productCategoryId")?.trim() || undefined
    const productBrandId =
        searchParams.get("productBrandId")?.trim() || undefined
    const productSupplierId =
        searchParams.get("productSupplierId")?.trim() || undefined
    const productListingStatus = PRODUCT_LISTING_FILTER_OPTIONS.find(
        (option) => option.value === searchParams.get("productListingStatus"),
    )?.value
    const productSupplyCoverage = PRODUCT_COVERAGE_FILTER_OPTIONS.find(
        (option) => option.value === searchParams.get("productSupplyCoverage"),
    )?.value
    const productSalesPriceMin =
        searchParams.get("productSalesPriceMin")?.trim() || undefined
    const productSalesPriceMax =
        searchParams.get("productSalesPriceMax")?.trim() || undefined
    const ownerUserIds = searchParams.get("ownerUserIds")?.trim() || undefined
    const procurementOwnerUserIds =
        searchParams.get("procurementOwnerUserIds")?.trim() || undefined
    const orgUnitIds = searchParams.get("orgUnitIds")?.trim() || undefined
    const includeDescendants = searchParams.get("includeDescendants") === "true"
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
        productSalesPriceMin ||
        productSalesPriceMax ||
        ownerUserIds ||
        procurementOwnerUserIds ||
        orgUnitIds ||
        includeDescendants,
    )

    const [productFilterPanelOpen, setProductFilterPanelOpen] = React.useState(
        hasStructuredProductFilters,
    )
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
    const [orgUnitIdsDraft, setOrgUnitIdsDraft] = React.useState(
        orgUnitIds ?? "",
    )
    const [includeDescendantsDraft, setIncludeDescendantsDraft] =
        React.useState(includeDescendants)

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

    const changeLifecycle = React.useCallback(
        (next: "enabled" | "disabled" | "all") => {
            if (next === lifecycleStatus) return
            patchUrl({
                lifecycleStatus: next === "all" ? null : next,
                metricKey: next === "all" ? null : next,
                page: null,
            })
            resetPagination()
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
            productCategoryId: productCategoryIdDraft,
            productBrandId: productBrandIdDraft,
            productSupplierId: productSupplierIdDraft,
            productSalesPriceMin: minimum || null,
            productSalesPriceMax: maximum || null,
            ownerUserIds: ownerUserIdsDraft.trim() || null,
            procurementOwnerUserIds:
                procurementOwnerUserIdsDraft.trim() || null,
            orgUnitIds: orgUnitIdsDraft.trim() || null,
            includeDescendants: includeDescendantsDraft ? "true" : null,
            page: null,
        })
        resetPagination()
        setProductFilterPanelOpen(false)
    }, [
        patchUrl,
        productBrandIdDraft,
        productCategoryIdDraft,
        productKindDraft,
        productListingStatusDraft,
        productSalesPriceMaxDraft,
        productSalesPriceMinDraft,
        productSupplierIdDraft,
        productSupplyCoverageDraft,
        ownerUserIdsDraft,
        procurementOwnerUserIdsDraft,
        orgUnitIdsDraft,
        includeDescendantsDraft,
        resetPagination,
        revisionTimingDraft,
        searchDraft,
    ])

    /** 移除单个已生效条件；销售价按区间整体移除。 */
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
            if (key === "orgUnitIds") {
                setOrgUnitIdsDraft("")
                setIncludeDescendantsDraft(false)
            }
            patchUrl(
                key === "salesPrice"
                    ? {
                          productSalesPriceMin: null,
                          productSalesPriceMax: null,
                          page: null,
                      }
                    : key === "orgUnitIds"
                      ? {
                            orgUnitIds: null,
                            includeDescendants: null,
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
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft("")
        setProcurementOwnerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
    }, [])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        productKindDraft !== (productKind ?? "all") ||
        revisionTimingDraft !== revisionTiming ||
        productListingStatusDraft !== (productListingStatus ?? "all") ||
        productSupplyCoverageDraft !== (productSupplyCoverage ?? "all") ||
        productCategoryIdDraft !== (productCategoryId ?? null) ||
        productBrandIdDraft !== (productBrandId ?? null) ||
        productSupplierIdDraft !== (productSupplierId ?? null) ||
        productSalesPriceMinDraft.trim() !== (productSalesPriceMin ?? "") ||
        productSalesPriceMaxDraft.trim() !== (productSalesPriceMax ?? "") ||
        ownerUserIdsDraft !== (ownerUserIds ?? "") ||
        procurementOwnerUserIdsDraft !== (procurementOwnerUserIds ?? "") ||
        orgUnitIdsDraft !== (orgUnitIds ?? "") ||
        includeDescendantsDraft !== includeDescendants

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setProductKindDraft("all")
        setRevisionTimingDraft("all")
        setProductListingStatusDraft("all")
        setProductSupplyCoverageDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft("")
        setProcurementOwnerUserIdsDraft("")
        setOrgUnitIdsDraft("")
        setIncludeDescendantsDraft(false)
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
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            ownerUserIds: null,
            procurementOwnerUserIds: null,
            orgUnitIds: null,
            includeDescendants: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination, setSearchDraft])

    React.useEffect(() => {
        setProductKindDraft(productKind ?? "all")
        setRevisionTimingDraft(revisionTiming)
        setProductListingStatusDraft(productListingStatus ?? "all")
        setProductSupplyCoverageDraft(productSupplyCoverage ?? "all")
        setProductCategoryIdDraft(productCategoryId ?? null)
        setProductBrandIdDraft(productBrandId ?? null)
        setProductSupplierIdDraft(productSupplierId ?? null)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
        setOwnerUserIdsDraft(ownerUserIds ?? "")
        setProcurementOwnerUserIdsDraft(procurementOwnerUserIds ?? "")
        setOrgUnitIdsDraft(orgUnitIds ?? "")
        setIncludeDescendantsDraft(includeDescendants)
    }, [
        lifecycleStatus,
        productBrandId,
        productCategoryId,
        productKind,
        productListingStatus,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplierId,
        productSupplyCoverage,
        ownerUserIds,
        procurementOwnerUserIds,
        orgUnitIds,
        includeDescendants,
        revisionTiming,
    ])

    return {
        q,
        lifecycleStatus,
        revisionTiming,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        productListingStatus,
        productSupplyCoverage,
        productSalesPriceMin,
        productSalesPriceMax,
        ownerUserIds,
        procurementOwnerUserIds,
        orgUnitIds,
        includeDescendants,
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
        orgUnitIdsDraft,
        setOrgUnitIdsDraft,
        includeDescendantsDraft,
        setIncludeDescendantsDraft,
        hasPendingChanges,
        pagination,
        setPagination,
        changePagination,
        changeLifecycle,
        commitSearch,
        applyProductFilters,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
