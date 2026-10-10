"use client"

import * as React from "react"
import { parsePriceFilterKind, type PriceFilterKind } from "@/lib/price-filter"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"

import { useListUrl } from "@/features/master-data/hooks/use-list-url"
import { productSalesPriceRangeError } from "@/features/master-data/lib/list-filters"
import {
    parseSellableListLayout,
    type SellableListLayout,
} from "@/features/master-data/lib/sellable-list-layout"
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

/** 公司商品池：搜索 + 类型 / 分类 / 品牌 / 区域 / 售价。 */
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

    const layout = parseSellableListLayout(searchParams.get("layout"))
    const productKind = PRODUCT_KIND_VALUES.find(
        (value) => value === searchParams.get("productKind"),
    )
    const productCategoryId =
        searchParams.get("productCategoryId")?.trim() || undefined
    const productBrandId =
        searchParams.get("productBrandId")?.trim() || undefined
    const supplyRegion = searchParams.get("supplyRegion")?.trim() || undefined
    const productPriceKind = parsePriceFilterKind(
        searchParams.get("productPriceKind"),
    )
    const productSalesPriceMin =
        searchParams.get("productSalesPriceMin")?.trim() || undefined
    const productSalesPriceMax =
        searchParams.get("productSalesPriceMax")?.trim() || undefined
    const hasAdvancedSellableFilters = Boolean(
        productCategoryId ||
        productBrandId ||
        supplyRegion ||
        productSalesPriceMin ||
        productSalesPriceMax,
    )
    const hasStructuredSellableFilters = Boolean(
        productKind || hasAdvancedSellableFilters,
    )

    const [sellableFilterPanelOpen, setSellableFilterPanelOpen] =
        React.useState(false)
    const [productSalesPriceError, setProductSalesPriceError] = React.useState<
        string | null
    >(null)
    const form = useAppForm({
        defaultValues: {
            searchDraft: q,
            productKindDraft: (productKind ?? "all") as ProductKind | "all",
            productCategoryIdDraft: productCategoryId ?? null,
            productBrandIdDraft: productBrandId ?? null,
            productPriceKindDraft: productPriceKind,
            productSalesPriceMinDraft: productSalesPriceMin ?? "",
            productSalesPriceMaxDraft: productSalesPriceMax ?? "",
            supplyRegionDraft: supplyRegion ?? "",
        },
        onSubmit: ({ value }) => {
            const minimum = value.productSalesPriceMinDraft.trim()
            const maximum = value.productSalesPriceMaxDraft.trim()
            const error = productSalesPriceRangeError(minimum, maximum)
            setProductSalesPriceError(error)
            if (error) {
                setSellableFilterPanelOpen(true)
                return
            }
            patchUrl({
                q: value.searchDraft.trim() || null,
                productKind:
                    value.productKindDraft === "all"
                        ? null
                        : value.productKindDraft,
                productCategoryId: value.productCategoryIdDraft,
                productBrandId: value.productBrandIdDraft,
                productSupplierId: null,
                supplyPreset: null,
                supplyRegion: value.supplyRegionDraft.trim() || null,
                productPriceKind:
                    minimum || maximum ? value.productPriceKindDraft : null,
                productSalesPriceMin: minimum || null,
                productSalesPriceMax: maximum || null,
                eligibilityAsOf: null,
                page: null,
            })
            resetPagination()
            setSellableFilterPanelOpen(false)
        },
    })
    const {
        searchDraft,
        productKindDraft,
        productCategoryIdDraft,
        productBrandIdDraft,
        productPriceKindDraft,
        productSalesPriceMinDraft,
        productSalesPriceMaxDraft,
        supplyRegionDraft,
    } = useStore(form.store, (state) => state.values)
    const setSearchDraft = React.useCallback(
        (value: string) => form.setFieldValue("searchDraft", value),
        [form],
    )
    const setProductKindDraft = React.useCallback(
        (value: ProductKind | "all") =>
            form.setFieldValue("productKindDraft", value),
        [form],
    )
    const setProductCategoryIdDraft = React.useCallback(
        (value: string | null) =>
            form.setFieldValue("productCategoryIdDraft", value),
        [form],
    )
    const setProductBrandIdDraft = React.useCallback(
        (value: string | null) =>
            form.setFieldValue("productBrandIdDraft", value),
        [form],
    )
    const setProductPriceKindDraft = React.useCallback(
        (value: PriceFilterKind) =>
            form.setFieldValue("productPriceKindDraft", value),
        [form],
    )
    const setProductSalesPriceMinDraft = React.useCallback(
        (value: string) =>
            form.setFieldValue("productSalesPriceMinDraft", value),
        [form],
    )
    const setProductSalesPriceMaxDraft = React.useCallback(
        (value: string) =>
            form.setFieldValue("productSalesPriceMaxDraft", value),
        [form],
    )
    const setSupplyRegionDraft = React.useCallback(
        (value: string) => form.setFieldValue("supplyRegionDraft", value),
        [form],
    )

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === q.trim()) return
        patchUrl({ q: next || null, page: null })
        resetPagination()
    }, [patchUrl, q, resetPagination, searchDraft])

    const applySellableFilters = React.useCallback(() => {
        void form.handleSubmit()
    }, [form])

    const setLayout = React.useCallback(
        (next: SellableListLayout) => {
            patchUrl({ layout: next === "table" ? null : next })
        },
        [patchUrl],
    )

    /** 移除单个已生效条件；价格类型与区间整体移除。 */
    const removeFilter = React.useCallback(
        (key: SellableFilterKey) => {
            if (key === "q") setSearchDraft("")
            if (key === "productKind") setProductKindDraft("all")
            if (key === "productCategoryId") setProductCategoryIdDraft(null)
            if (key === "productBrandId") setProductBrandIdDraft(null)
            if (key === "supplyRegion") setSupplyRegionDraft("")
            if (key === "salesPrice") {
                setProductPriceKindDraft("dropship")
                setProductSalesPriceMinDraft("")
                setProductSalesPriceMaxDraft("")
                setProductSalesPriceError(null)
            }
            patchUrl(
                key === "salesPrice"
                    ? {
                          productPriceKind: null,
                          productSalesPriceMin: null,
                          productSalesPriceMax: null,
                          page: null,
                      }
                    : { [key]: null, page: null },
            )
            resetPagination()
        },
        [
            patchUrl,
            resetPagination,
            setSearchDraft,
            setProductKindDraft,
            setProductCategoryIdDraft,
            setProductBrandIdDraft,
            setProductPriceKindDraft,
            setProductSalesPriceMinDraft,
            setProductSalesPriceMaxDraft,
            setSupplyRegionDraft,
        ],
    )

    /** 仅重置更多条件的草稿；保留关键词、商品类型、分类及当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setProductBrandIdDraft(null)
        setProductPriceKindDraft("dropship")
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setSupplyRegionDraft("")
        setProductSalesPriceError(null)
    }, [
        setProductBrandIdDraft,
        setProductPriceKindDraft,
        setProductSalesPriceMinDraft,
        setProductSalesPriceMaxDraft,
        setSupplyRegionDraft,
    ])

    /** 取消、外点和 Esc 只恢复低频草稿，保留搜索、商品类型和分类。 */
    const cancelMoreFilters = React.useCallback(() => {
        setProductBrandIdDraft(productBrandId ?? null)
        setSupplyRegionDraft(supplyRegion ?? "")
        setProductPriceKindDraft(productPriceKind)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
        setSellableFilterPanelOpen(false)
    }, [
        productBrandId,
        productSalesPriceMax,
        productPriceKind,
        productSalesPriceMin,
        setProductBrandIdDraft,
        setProductSalesPriceMaxDraft,
        setProductPriceKindDraft,
        setProductSalesPriceMinDraft,
        setSupplyRegionDraft,
        supplyRegion,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        productKindDraft !== (productKind ?? "all") ||
        productCategoryIdDraft !== (productCategoryId ?? null) ||
        productBrandIdDraft !== (productBrandId ?? null) ||
        supplyRegionDraft.trim() !== (supplyRegion ?? "") ||
        (Boolean(
            productSalesPriceMinDraft.trim() ||
            productSalesPriceMaxDraft.trim(),
        ) &&
            productPriceKindDraft !== productPriceKind) ||
        productSalesPriceMinDraft.trim() !== (productSalesPriceMin ?? "") ||
        productSalesPriceMaxDraft.trim() !== (productSalesPriceMax ?? "")

    const clearAllFilters = React.useCallback(() => {
        setSearchDraft("")
        setProductKindDraft("all")
        setProductCategoryIdDraft(null)
        setProductBrandIdDraft(null)
        setProductPriceKindDraft("dropship")
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
            productPriceKind: null,
            productSalesPriceMin: null,
            productSalesPriceMax: null,
            supplyPreset: null,
            page: null,
        })
        resetPagination()
    }, [
        patchUrl,
        resetPagination,
        setSearchDraft,
        setProductKindDraft,
        setProductCategoryIdDraft,
        setProductBrandIdDraft,
        setProductPriceKindDraft,
        setProductSalesPriceMinDraft,
        setProductSalesPriceMaxDraft,
        setSupplyRegionDraft,
    ])

    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) setSearchDraft(q)
    }, [q, searchInputRef, setSearchDraft])

    React.useEffect(() => {
        setProductKindDraft(productKind ?? "all")
    }, [productKind, setProductKindDraft])

    React.useEffect(() => {
        setProductCategoryIdDraft(productCategoryId ?? null)
    }, [productCategoryId, setProductCategoryIdDraft])

    React.useEffect(() => {
        setProductBrandIdDraft(productBrandId ?? null)
    }, [productBrandId, setProductBrandIdDraft])

    React.useEffect(() => {
        setSupplyRegionDraft(supplyRegion ?? "")
    }, [supplyRegion, setSupplyRegionDraft])

    React.useEffect(() => {
        setProductPriceKindDraft(productPriceKind)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
    }, [
        productPriceKind,
        productSalesPriceMin,
        productSalesPriceMax,
        setProductPriceKindDraft,
        setProductSalesPriceMinDraft,
        setProductSalesPriceMaxDraft,
    ])

    return {
        q,
        layout,
        setLayout,
        productKind,
        productCategoryId,
        productBrandId,
        supplyRegion,
        productPriceKind,
        productSalesPriceMin,
        productSalesPriceMax,
        hasPendingChanges,
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
        productPriceKindDraft,
        productSalesPriceMinDraft,
        setProductPriceKindDraft,
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
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
    }
}
