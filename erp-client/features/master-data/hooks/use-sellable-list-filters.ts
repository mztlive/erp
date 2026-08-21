"use client"

import * as React from "react"

import {
    useListUrl,
    useSearchDraft,
} from "@/features/master-data/hooks/use-list-url"
import { productSalesPriceRangeError } from "@/features/master-data/lib/list-filters"
import {
    PRODUCT_KIND_VALUES,
    type ProductKind,
    type SellableSupplyPreset,
} from "@/features/master-data/types"

export type SellableSupplyPresetSelection = SellableSupplyPreset | "all"

/** 可被单独移除的已生效条件。 */
export type SellableFilterKey =
    | "q"
    | "productKind"
    | "productCategoryId"
    | "productBrandId"
    | "productSupplierId"
    | "supplyRegion"
    | "supplyPreset"
    | "salesPrice"

/** 公司商品池：搜索 + 类型 / 分类 / 品牌 / 供应商 / 区域 / 售价。 */
export function useSellableListFilters(
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

    const productKind = PRODUCT_KIND_VALUES.find(
        (value) => value === searchParams.get("productKind"),
    )
    const productCategoryId =
        searchParams.get("productCategoryId")?.trim() || undefined
    const productBrandId =
        searchParams.get("productBrandId")?.trim() || undefined
    const productSupplierId =
        searchParams.get("productSupplierId")?.trim() || undefined
    const supplyRegion = searchParams.get("supplyRegion")?.trim() || undefined
    const supplyPreset = (["single-supplier", "nationwide"] as const).find(
        (value) => value === searchParams.get("supplyPreset"),
    )
    const productSalesPriceMin =
        searchParams.get("productSalesPriceMin")?.trim() || undefined
    const productSalesPriceMax =
        searchParams.get("productSalesPriceMax")?.trim() || undefined
    const hasAdvancedSellableFilters = Boolean(
        productCategoryId ||
        productBrandId ||
        productSupplierId ||
        supplyRegion ||
        productSalesPriceMin ||
        productSalesPriceMax,
    )
    const hasStructuredSellableFilters = Boolean(
        productKind || hasAdvancedSellableFilters,
    )

    const [sellableFilterPanelOpen, setSellableFilterPanelOpen] =
        React.useState(hasStructuredSellableFilters)
    const [productKindDraft, setProductKindDraft] = React.useState<
        ProductKind | "all"
    >(productKind ?? "all")
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

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

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
        setSellableFilterPanelOpen(false)
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

    /** 快捷视图直接应用；它不改动关键词或「更多筛选」草稿。 */
    const applySupplyPreset = React.useCallback(
        (next: SellableSupplyPresetSelection) => {
            patchUrl({
                supplyPreset: next === "all" ? null : next,
                page: null,
            })
            resetPagination()
        },
        [patchUrl, resetPagination],
    )

    /** 移除单个已生效条件；销售价按区间整体移除。 */
    const removeFilter = React.useCallback(
        (key: SellableFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "productKind") setProductKindDraft("all")
            if (key === "productCategoryId") setProductCategoryIdDraft(null)
            if (key === "productBrandId") setProductBrandIdDraft(null)
            if (key === "productSupplierId") setProductSupplierIdDraft(null)
            if (key === "supplyRegion") setSupplyRegionDraft("")
            if (key === "salesPrice") {
                setProductSalesPriceMinDraft("")
                setProductSalesPriceMaxDraft("")
                setProductSalesPriceError(null)
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

    /** 仅清除「更多筛选」；保留关键词和快捷视图，并立即刷新结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setProductKindDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setSupplyRegionDraft("")
        setProductSalesPriceError(null)
        patchUrl({
            productKind: null,
            eligibilityAsOf: null,
            productCategoryId: null,
            productBrandId: null,
            productSupplierId: null,
            supplyRegion: null,
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setProductKindDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setSupplyRegionDraft("")
        setProductSalesPriceError(null)
        setSellableFilterPanelOpen(false)
        patchUrl({
            q: null,
            productKind: null,
            eligibilityAsOf: null,
            productCategoryId: null,
            productBrandId: null,
            productSupplierId: null,
            supplyRegion: null,
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            supplyPreset: null,
            page: null,
        })
        resetPagination()
    }, [patchUrl, resetPagination, setSearchDraft])

    React.useEffect(() => {
        setProductKindDraft(productKind ?? "all")
        setProductCategoryIdDraft(productCategoryId ?? null)
        setProductBrandIdDraft(productBrandId ?? null)
        setProductSupplierIdDraft(productSupplierId ?? null)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setSupplyRegionDraft(supplyRegion ?? "")
        setProductSalesPriceError(null)
    }, [
        productBrandId,
        productCategoryId,
        productKind,
        productSalesPriceMax,
        productSalesPriceMin,
        productSupplierId,
        supplyRegion,
    ])

    return {
        q,
        supplyPreset,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        supplyRegion,
        productSalesPriceMin,
        productSalesPriceMax,
        hasAdvancedSellableFilters,
        hasStructuredSellableFilters,
        searchDraft,
        setSearchDraft,
        sellableFilterPanelOpen,
        setSellableFilterPanelOpen,
        productKindDraft,
        setProductKindDraft,
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
        changePagination,
        commitSearch,
        applySellableFilters,
        applySupplyPreset,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
