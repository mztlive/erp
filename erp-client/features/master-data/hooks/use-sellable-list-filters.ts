"use client"

import * as React from "react"
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

    const layout = parseSellableListLayout(searchParams.get("layout"))
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
            productSupplierIdDraft: productSupplierId ?? null,
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
                productSupplierId: value.productSupplierIdDraft,
                supplyRegion: value.supplyRegionDraft.trim() || null,
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
        productSupplierIdDraft,
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
    const setProductSupplierIdDraft = React.useCallback(
        (value: string | null) =>
            form.setFieldValue("productSupplierIdDraft", value),
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
        [
            patchUrl,
            resetPagination,
            setSearchDraft,
            setProductKindDraft,
            setProductCategoryIdDraft,
            setProductBrandIdDraft,
            setProductSupplierIdDraft,
            setProductSalesPriceMinDraft,
            setProductSalesPriceMaxDraft,
            setSupplyRegionDraft,
        ],
    )

    /** 仅重置更多条件的草稿；保留关键词、商品类型、分类及当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setProductBrandIdDraft(null)
        setProductSupplierIdDraft(null)
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setSupplyRegionDraft("")
        setProductSalesPriceError(null)
    }, [
        setProductBrandIdDraft,
        setProductSupplierIdDraft,
        setProductSalesPriceMinDraft,
        setProductSalesPriceMaxDraft,
        setSupplyRegionDraft,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        productKindDraft !== (productKind ?? "all") ||
        productCategoryIdDraft !== (productCategoryId ?? null) ||
        productBrandIdDraft !== (productBrandId ?? null) ||
        productSupplierIdDraft !== (productSupplierId ?? null) ||
        supplyRegionDraft.trim() !== (supplyRegion ?? "") ||
        productSalesPriceMinDraft.trim() !== (productSalesPriceMin ?? "") ||
        productSalesPriceMaxDraft.trim() !== (productSalesPriceMax ?? "")

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
    }, [
        patchUrl,
        resetPagination,
        setSearchDraft,
        setProductKindDraft,
        setProductCategoryIdDraft,
        setProductBrandIdDraft,
        setProductSupplierIdDraft,
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
        setProductSupplierIdDraft(productSupplierId ?? null)
    }, [productSupplierId, setProductSupplierIdDraft])

    React.useEffect(() => {
        setSupplyRegionDraft(supplyRegion ?? "")
    }, [supplyRegion, setSupplyRegionDraft])

    React.useEffect(() => {
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
    }, [
        productSalesPriceMin,
        productSalesPriceMax,
        setProductSalesPriceMinDraft,
        setProductSalesPriceMaxDraft,
    ])

    return {
        q,
        layout,
        setLayout,
        supplyPreset,
        productKind,
        productCategoryId,
        productBrandId,
        productSupplierId,
        supplyRegion,
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
