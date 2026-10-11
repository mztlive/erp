"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import type { PaginationState } from "@tanstack/react-table"
import { useAppForm } from "@/components/form"
import type { PriceFilterKind } from "@/lib/price-filter"
import type { ProductKind } from "@/features/master-data/types"
import type { SellableFilterKey } from "@/features/master-data/hooks/use-sellable-list-filters"
import { productSalesPriceRangeError } from "@/features/master-data/lib/list-filters"
import type { SellablePickerAppliedFilters } from "@/features/sales-orders/lib/sellable-sku-picker-query"

const EMPTY_FILTERS = { q: "", productPriceKind: "dropship" } as const

/** 复用公司商品池的筛选合同，草稿通过表单提交，已生效条件仅保存在弹窗内。 */
export function useSellableSkuPickerFilters() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [applied, setApplied] =
        React.useState<SellablePickerAppliedFilters>(EMPTY_FILTERS)
    const {
        q,
        productKind,
        productCategoryId,
        productBrandId,
        supplyRegion,
        productPriceKind = "dropship",
        productSalesPriceMin,
        productSalesPriceMax,
    } = applied
    const [pagination, changePagination] = React.useState<PaginationState>({
        pageIndex: 0,
        pageSize: 20,
    })
    const resetPagination = React.useCallback(() => {
        changePagination((current) => ({ ...current, pageIndex: 0 }))
    }, [])
    const hasStructuredSellableFilters = Boolean(
        productKind ||
        productCategoryId ||
        productBrandId ||
        supplyRegion ||
        productSalesPriceMin ||
        productSalesPriceMax,
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
            setApplied({
                q: value.searchDraft.trim(),
                productKind:
                    value.productKindDraft === "all"
                        ? undefined
                        : value.productKindDraft,
                productCategoryId: value.productCategoryIdDraft ?? undefined,
                productBrandId: value.productBrandIdDraft ?? undefined,
                supplyRegion: value.supplyRegionDraft.trim() || undefined,
                productPriceKind:
                    minimum || maximum
                        ? value.productPriceKindDraft
                        : "dropship",
                productSalesPriceMin: minimum || undefined,
                productSalesPriceMax: maximum || undefined,
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

    const applySellableFilters = React.useCallback(() => {
        void form.handleSubmit()
    }, [form])

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
            setApplied((current) => {
                if (key === "salesPrice")
                    return {
                        ...current,
                        productPriceKind: "dropship",
                        productSalesPriceMin: undefined,
                        productSalesPriceMax: undefined,
                    }
                if (key === "q") return { ...current, q: "" }
                return { ...current, [key]: undefined }
            })
            resetPagination()
        },
        [
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
        setApplied(EMPTY_FILTERS)
        resetPagination()
    }, [
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

    return {
        q,
        searchInputRef,
        productKind,
        productCategoryId,
        productBrandId,
        supplyRegion,
        productPriceKind,
        productSalesPriceMin,
        productSalesPriceMax,
        hasPendingChanges,
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
        changePagination,
        applySellableFilters,
        removeFilter,
        resetMoreFilters,
        cancelMoreFilters,
        clearAllFilters,
    }
}
