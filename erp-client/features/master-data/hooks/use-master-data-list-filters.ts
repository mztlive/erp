"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import {
    csvFilterValue,
    productSalesPriceRangeError,
    PRODUCT_COVERAGE_FILTER_OPTIONS,
    PRODUCT_LISTING_FILTER_OPTIONS,
    selectedSupplierOptionValues,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_HEALTH_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import {
    PRODUCT_KIND_VALUES,
    type ProductKind,
    type ProductListingFilter,
    type ProductSkuCoverageFilter,
    type SupplierQualificationHealth,
} from "@/features/master-data/types"

type UseMasterDataListFiltersArgs = {
    isProductResource: boolean
    isSupplierResource: boolean
    isSellableResource: boolean
    searchInputRef: React.RefObject<HTMLInputElement | null>
}

/**
 * 列表筛选与分页唯一事实源 = URL（刷新/后退/分享一致）。
 * 面板草稿仅在挂载、外部 URL 变化与显式提交时同步。
 */
export function useMasterDataListFilters({
    isProductResource,
    isSupplierResource,
    isSellableResource,
    searchInputRef,
}: UseMasterDataListFiltersArgs) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    // ── 筛选与分页唯一事实源 = URL（刷新/后退/分享一致） ──
    const q = searchParams.get("q") ?? ""
    const lifecycleStatusParam = searchParams.get("lifecycleStatus")
    const lifecycleStatus: "enabled" | "disabled" | "all" =
        lifecycleStatusParam === "enabled" ||
        lifecycleStatusParam === "disabled"
            ? lifecycleStatusParam
            : "all"
    const revisionTimingParam = searchParams.get("revisionTiming")
    const revisionTiming: "current" | "future" | "all" =
        revisionTimingParam === "current" || revisionTimingParam === "future"
            ? revisionTimingParam
            : "all"
    const productKind =
        isProductResource || isSellableResource
            ? PRODUCT_KIND_VALUES.find(
                  (value) => value === searchParams.get("productKind"),
              )
            : undefined
    const productCategoryId =
        isProductResource || isSellableResource
            ? searchParams.get("productCategoryId")?.trim() || undefined
            : undefined
    const productBrandId =
        isProductResource || isSellableResource
            ? searchParams.get("productBrandId")?.trim() || undefined
            : undefined
    const productSupplierId =
        isProductResource || isSellableResource
            ? searchParams.get("productSupplierId")?.trim() || undefined
            : undefined
    const supplyRegion = isSellableResource
        ? searchParams.get("supplyRegion")?.trim() || undefined
        : undefined
    const productListingStatus = isProductResource
        ? PRODUCT_LISTING_FILTER_OPTIONS.find(
              (option) =>
                  option.value === searchParams.get("productListingStatus"),
          )?.value
        : undefined
    const productSupplyCoverage = isProductResource
        ? PRODUCT_COVERAGE_FILTER_OPTIONS.find(
              (option) =>
                  option.value === searchParams.get("productSupplyCoverage"),
          )?.value
        : undefined
    const productSalesPriceMin =
        isProductResource || isSellableResource
            ? searchParams.get("productSalesPriceMin")?.trim() || undefined
            : undefined
    const productSalesPriceMax =
        isProductResource || isSellableResource
            ? searchParams.get("productSalesPriceMax")?.trim() || undefined
            : undefined
    const supplierCapabilityCodesParam = searchParams.get(
        "supplierCapabilityCodes",
    )
    const supplierQualificationTypesParam = searchParams.get(
        "supplierQualificationTypes",
    )
    const supplierCapabilityCodes = React.useMemo(
        () =>
            isSupplierResource
                ? selectedSupplierOptionValues(
                      supplierCapabilityCodesParam,
                      SUPPLIER_CAPABILITY_OPTIONS,
                  )
                : [],
        [isSupplierResource, supplierCapabilityCodesParam],
    )
    const supplierQualificationTypes = React.useMemo(
        () =>
            isSupplierResource
                ? selectedSupplierOptionValues(
                      supplierQualificationTypesParam,
                      SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
                  )
                : [],
        [isSupplierResource, supplierQualificationTypesParam],
    )
    const supplierQualificationHealthParam = searchParams.get(
        "supplierQualificationHealth",
    )
    const supplierQualificationHealth = isSupplierResource
        ? (SUPPLIER_QUALIFICATION_HEALTH_OPTIONS.find(
              (option) =>
                  option.value !== "all" &&
                  option.value === supplierQualificationHealthParam,
          )?.value as SupplierQualificationHealth | undefined)
        : undefined
    /** 指标态保留在 URL：与 lifecycleStatus 同源写入，只做展示不做筛选。 */
    const metricKey = searchParams.get("metricKey") ?? "all"
    /** 已生效（非草稿）的结构化商品筛选是否有任意一项非默认值，决定"高级筛选"默认展开与徽标。 */
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
        productSalesPriceMax,
    )
    /** 公司商品池结构化筛选（不含搜索词）。 */
    const hasStructuredSellableFilters = Boolean(
        productKind ||
        productCategoryId ||
        productBrandId ||
        productSupplierId ||
        supplyRegion ||
        productSalesPriceMin ||
        productSalesPriceMax,
    )
    const hasStructuredSupplierFilters = Boolean(
        lifecycleStatus !== "all" ||
        supplierQualificationHealth ||
        supplierCapabilityCodes.length ||
        supplierQualificationTypes.length,
    )
    /** 通用列表（品牌/计量单位/卡券类目/仓库）已生效结构化筛选（启停/版本）。 */
    const hasStructuredListFilters =
        lifecycleStatus !== "all" || revisionTiming !== "all"
    const pageParamRaw = Number(searchParams.get("page"))
    const pageParamIndex =
        Number.isFinite(pageParamRaw) && pageParamRaw > 0
            ? Math.max(0, Math.floor(pageParamRaw) - 1)
            : 0

    const [searchDraft, setSearchDraft] = React.useState(q)
    /** 高级筛选面板展开态：本地 UI 状态，不入 URL；有已生效筛选时默认展开。 */
    const [productFilterPanelOpen, setProductFilterPanelOpen] = React.useState(
        hasStructuredProductFilters,
    )
    /** 公司商品池 §3.6 筛选面板展开态；深链带入条件时自动展开。 */
    const [sellableFilterPanelOpen, setSellableFilterPanelOpen] =
        React.useState(hasStructuredSellableFilters)
    /** 供应商显式提交筛选面板展开态；深链带入条件时自动展开。 */
    const [supplierFilterPanelOpen, setSupplierFilterPanelOpen] =
        React.useState(hasStructuredSupplierFilters)
    /** 通用列表显式提交筛选面板展开态；深链带入条件时自动展开（§3.6）。 */
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(
        hasStructuredListFilters,
    )
    const [supplierCapabilityCodesDraft, setSupplierCapabilityCodesDraft] =
        React.useState<string[]>(supplierCapabilityCodes)
    const [
        supplierQualificationTypesDraft,
        setSupplierQualificationTypesDraft,
    ] = React.useState<string[]>(supplierQualificationTypes)
    const [
        supplierQualificationHealthDraft,
        setSupplierQualificationHealthDraft,
    ] = React.useState<SupplierQualificationHealth | "all">(
        supplierQualificationHealth ?? "all",
    )
    // 商品与供应商筛选面板均为"编辑草稿 + 点击搜索才提交"模式：以下均为本地草稿，
    // 与 URL 事实源的同步只发生在挂载、外部 URL 变化（后退/清除）与提交时。
    const [productKindDraft, setProductKindDraft] = React.useState<
        ProductKind | "all"
    >(productKind ?? "all")
    const [lifecycleStatusDraft, setLifecycleStatusDraft] = React.useState<
        "enabled" | "disabled" | "all"
    >(lifecycleStatus)
    const [revisionTimingDraft, setRevisionTimingDraft] = React.useState<
        "current" | "future" | "all"
    >(revisionTiming)
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
    const [supplyRegionDraft, setSupplyRegionDraft] = React.useState(
        supplyRegion ?? "",
    )
    const [productSalesPriceError, setProductSalesPriceError] = React.useState<
        string | null
    >(null)
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageParamIndex,
        pageSize: 20,
    })

    const patchUrl = React.useCallback(
        (patch: Record<string, string | null>) => {
            const next = new URLSearchParams(searchParams.toString())
            for (const [key, value] of Object.entries(patch)) {
                if (value == null || value === "") next.delete(key)
                else next.set(key, value)
            }
            const qs = next.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
        },
        [pathname, router, searchParams],
    )

    const resetPagination = React.useCallback(() => {
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }, [])

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

    /** 搜索提交：写 URL 并回第 1 页；与草稿相同时跳过。 */
    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

    /** 公司商品池筛选面板整体提交（§3.6）：草稿一次性写入 URL。 */
    const applySellableFilters = React.useCallback(() => {
        const minimum = productSalesPriceMinDraft.trim()
        const maximum = productSalesPriceMaxDraft.trim()
        const error = productSalesPriceRangeError(minimum, maximum)
        setProductSalesPriceError(error)
        if (error) return
        patchUrl({
            q: searchDraft.trim() || null,
            productKind: productKindDraft === "all" ? null : productKindDraft,
            productCategoryId: productCategoryIdDraft,
            productBrandId: productBrandIdDraft,
            productSupplierId: productSupplierIdDraft,
            supplyRegion: supplyRegionDraft.trim() || null,
            productSalesPriceMin: minimum || null,
            productSalesPriceMax: maximum || null,
            eligibilityAsOf: null,
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        productBrandIdDraft,
        productCategoryIdDraft,
        productKindDraft,
        productSalesPriceMaxDraft,
        productSalesPriceMinDraft,
        productSupplierIdDraft,
        resetPagination,
        searchDraft,
        supplyRegionDraft,
    ])

    /** 提交供应商筛选面板；全部草稿字段一次性写入 URL。 */
    const applySupplierFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            supplierCapabilityCodes: csvFilterValue(
                supplierCapabilityCodesDraft,
            ),
            supplierQualificationTypes: csvFilterValue(
                supplierQualificationTypesDraft,
            ),
            supplierQualificationHealth:
                supplierQualificationHealthDraft === "all"
                    ? null
                    : supplierQualificationHealthDraft,
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        resetPagination,
        searchDraft,
        lifecycleStatusDraft,
        supplierCapabilityCodesDraft,
        supplierQualificationHealthDraft,
        supplierQualificationTypesDraft,
    ])

    /** 提交通用列表筛选面板；草稿一次性写入 URL（§3.6）。 */
    const applyListFilters = React.useCallback(() => {
        patchUrl({
            q: searchDraft.trim() || null,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            revisionTiming:
                revisionTimingDraft === "all" ? null : revisionTimingDraft,
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        resetPagination,
        searchDraft,
        lifecycleStatusDraft,
        revisionTimingDraft,
    ])

    /** 商品筛选面板整体提交：点击"搜索"才把全部草稿字段一次性写入 URL。 */
    const applyProductFilters = React.useCallback(() => {
        const minimum = productSalesPriceMinDraft.trim()
        const maximum = productSalesPriceMaxDraft.trim()
        const error = productSalesPriceRangeError(minimum, maximum)
        setProductSalesPriceError(error)
        if (error) return
        patchUrl({
            q: searchDraft.trim() || null,
            productKind: productKindDraft === "all" ? null : productKindDraft,
            lifecycleStatus:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
            metricKey:
                lifecycleStatusDraft === "all" ? null : lifecycleStatusDraft,
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
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        resetPagination,
        searchDraft,
        productKindDraft,
        lifecycleStatusDraft,
        revisionTimingDraft,
        productListingStatusDraft,
        productSupplyCoverageDraft,
        productCategoryIdDraft,
        productBrandIdDraft,
        productSupplierIdDraft,
        productSalesPriceMinDraft,
        productSalesPriceMaxDraft,
    ])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setProductKindDraft("all")
        setLifecycleStatusDraft("all")
        setRevisionTimingDraft("all")
        setProductListingStatusDraft("all")
        setProductSupplyCoverageDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setSupplyRegionDraft("")
        setProductSalesPriceError(null)
        setProductFilterPanelOpen(false)
        setSellableFilterPanelOpen(false)
        setSupplierCapabilityCodesDraft([])
        setSupplierQualificationTypesDraft([])
        setSupplierQualificationHealthDraft("all")
        setSupplierFilterPanelOpen(false)
        setFilterPanelOpen(false)
        patchUrl({
            q: null,
            lifecycleStatus: null,
            metricKey: null,
            revisionTiming: null,
            productKind: null,
            eligibilityAsOf: null,
            productCategoryId: null,
            productBrandId: null,
            productSupplierId: null,
            supplyRegion: null,
            productListingStatus: null,
            productSupplyCoverage: null,
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            supplierCapabilityCodes: null,
            supplierQualificationTypes: null,
            supplierQualificationHealth: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination])

    // URL 回填草稿（后退/前进）；搜索框聚焦时不覆盖正在输入的内容（§4.1 焦点保护）。
    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q)
        }
    }, [q, searchInputRef])

    React.useEffect(() => {
        setProductKindDraft(productKind ?? "all")
        setLifecycleStatusDraft(lifecycleStatus)
        setRevisionTimingDraft(revisionTiming)
        setProductListingStatusDraft(productListingStatus ?? "all")
        setProductSupplyCoverageDraft(productSupplyCoverage ?? "all")
        setProductCategoryIdDraft(productCategoryId ?? null)
        setProductBrandIdDraft(productBrandId ?? null)
        setProductSupplierIdDraft(productSupplierId ?? null)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setSupplyRegionDraft(supplyRegion ?? "")
        setProductSalesPriceError(null)
        setProductFilterPanelOpen(hasStructuredProductFilters)
        setSellableFilterPanelOpen(hasStructuredSellableFilters)
    }, [
        productKind,
        lifecycleStatus,
        revisionTiming,
        productListingStatus,
        productSupplyCoverage,
        productCategoryId,
        productBrandId,
        productSupplierId,
        productSalesPriceMax,
        productSalesPriceMin,
        supplyRegion,
        hasStructuredProductFilters,
        hasStructuredSellableFilters,
    ])

    React.useEffect(() => {
        setSupplierCapabilityCodesDraft(supplierCapabilityCodes)
        setSupplierQualificationTypesDraft(supplierQualificationTypes)
        setSupplierQualificationHealthDraft(
            supplierQualificationHealth ?? "all",
        )
        setSupplierFilterPanelOpen(hasStructuredSupplierFilters)
    }, [
        hasStructuredSupplierFilters,
        supplierCapabilityCodes,
        supplierQualificationHealth,
        supplierQualificationTypes,
    ])

    // URL 外部变化（后退/前进/清除）时，同步通用列表面板展开态（§3.6）。
    React.useEffect(() => {
        setFilterPanelOpen(hasStructuredListFilters)
    }, [hasStructuredListFilters])

    // URL page 回读（后退/前进/分享恢复）
    React.useEffect(() => {
        setPagination((p) => ({ ...p, pageIndex: pageParamIndex }))
    }, [pageParamIndex])
    return {
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
    }
}
