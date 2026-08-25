"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"

import { productSalesPriceRangeError } from "@/features/master-data/lib/list-filters"
import type {
    SellableFilterKey,
    SellableSupplyPresetSelection,
} from "@/features/master-data/hooks/use-sellable-list-filters"
import {
    PRODUCT_KIND_VALUES,
    type ProductKind,
    type SellableSupplyPreset,
} from "@/features/master-data/types"

const EMPTY_PAGINATION: PaginationState = { pageIndex: 0, pageSize: 20 }

/** Dialog 内的公司商品池筛选：与列表页条件一致，但不写入 URL。 */
export function useSellableSkuPickerFilters() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [searchDraft, setSearchDraft] = React.useState("")
    const [q, setQ] = React.useState("")
    const [supplyPreset, setSupplyPreset] =
        React.useState<SellableSupplyPresetSelection>("all")
    const [productKind, setProductKind] = React.useState<
        ProductKind | undefined
    >(undefined)
    const [productCategoryId, setProductCategoryId] = React.useState<
        string | undefined
    >(undefined)
    const [productBrandId, setProductBrandId] = React.useState<
        string | undefined
    >(undefined)
    const [productSupplierId, setProductSupplierId] = React.useState<
        string | undefined
    >(undefined)
    const [supplyRegion, setSupplyRegion] = React.useState<string | undefined>(
        undefined,
    )
    const [productSalesPriceMin, setProductSalesPriceMin] = React.useState<
        string | undefined
    >(undefined)
    const [productSalesPriceMax, setProductSalesPriceMax] = React.useState<
        string | undefined
    >(undefined)

    const [sellableFilterPanelOpen, setSellableFilterPanelOpen] =
        React.useState(false)
    const [productKindDraft, setProductKindDraft] = React.useState<
        ProductKind | "all"
    >("all")
    const [productCategoryIdDraft, setProductCategoryIdDraft] = React.useState<
        string | null
    >(null)
    const [productBrandIdDraft, setProductBrandIdDraft] = React.useState<
        string | null
    >(null)
    const [productSupplierIdDraft, setProductSupplierIdDraft] = React.useState<
        string | null
    >(null)
    const [productSalesPriceMinDraft, setProductSalesPriceMinDraft] =
        React.useState("")
    const [productSalesPriceMaxDraft, setProductSalesPriceMaxDraft] =
        React.useState("")
    const [supplyRegionDraft, setSupplyRegionDraft] = React.useState("")
    const [productSalesPriceError, setProductSalesPriceError] = React.useState<
        string | null
    >(null)
    const [pagination, setPagination] =
        React.useState<PaginationState>(EMPTY_PAGINATION)

    const resetPagination = React.useCallback(() => {
        setPagination((current) =>
            current.pageIndex === 0 ? current : { ...current, pageIndex: 0 },
        )
    }, [])

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

    const applySellableFilters = React.useCallback(() => {
        const minimum = productSalesPriceMinDraft.trim()
        const maximum = productSalesPriceMaxDraft.trim()
        const error = productSalesPriceRangeError(minimum, maximum)
        setProductSalesPriceError(error)
        if (error) return
        const nextKind =
            productKindDraft === "all"
                ? undefined
                : PRODUCT_KIND_VALUES.find(
                      (value) => value === productKindDraft,
                  )
        setQ(searchDraft.trim())
        setProductKind(nextKind)
        setProductCategoryId(productCategoryIdDraft ?? undefined)
        setProductBrandId(productBrandIdDraft ?? undefined)
        setProductSupplierId(productSupplierIdDraft ?? undefined)
        setSupplyRegion(supplyRegionDraft.trim() || undefined)
        setProductSalesPriceMin(minimum || undefined)
        setProductSalesPriceMax(maximum || undefined)
        setSellableFilterPanelOpen(false)
        resetPagination()
    }, [
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

    const applySupplyPreset = React.useCallback(
        (next: SellableSupplyPresetSelection) => {
            setSupplyPreset(next)
            resetPagination()
        },
        [resetPagination],
    )

    const removeFilter = React.useCallback(
        (key: SellableFilterKey) => {
            if (key === "q") {
                setSearchDraft("")
                setQ("")
            }
            if (key === "productKind") {
                setProductKindDraft("all")
                setProductKind(undefined)
            }
            if (key === "productCategoryId") {
                setProductCategoryIdDraft(null)
                setProductCategoryId(undefined)
            }
            if (key === "productBrandId") {
                setProductBrandIdDraft(null)
                setProductBrandId(undefined)
            }
            if (key === "productSupplierId") {
                setProductSupplierIdDraft(null)
                setProductSupplierId(undefined)
            }
            if (key === "supplyRegion") {
                setSupplyRegionDraft("")
                setSupplyRegion(undefined)
            }
            if (key === "supplyPreset") {
                setSupplyPreset("all")
            }
            if (key === "salesPrice") {
                setProductSalesPriceMinDraft("")
                setProductSalesPriceMaxDraft("")
                setProductSalesPriceMin(undefined)
                setProductSalesPriceMax(undefined)
                setProductSalesPriceError(null)
            }
            resetPagination()
        },
        [resetPagination],
    )

    const resetMoreFilters = React.useCallback(() => {
        setProductKindDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setSupplyRegionDraft("")
        setProductSalesPriceError(null)
        setProductKind(undefined)
        setProductCategoryId(undefined)
        setProductBrandId(undefined)
        setProductSupplierId(undefined)
        setProductSalesPriceMin(undefined)
        setProductSalesPriceMax(undefined)
        setSupplyRegion(undefined)
        resetPagination()
    }, [resetPagination])

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setQ("")
        setSupplyPreset("all")
        setSellableFilterPanelOpen(false)
        resetMoreFilters()
    }, [resetMoreFilters])

    const changePagination = React.useCallback((next: PaginationState) => {
        setPagination(next)
    }, [])

    return {
        searchInputRef,
        q,
        supplyPreset:
            supplyPreset === "all"
                ? undefined
                : (supplyPreset as SellableSupplyPreset),
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        supplyRegion,
        productSalesPriceMin,
        productSalesPriceMax,
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
        changePagination,
        applySellableFilters,
        applySupplyPreset,
        removeFilter,
        resetMoreFilters,
        clearAllFilters,
    }
}
