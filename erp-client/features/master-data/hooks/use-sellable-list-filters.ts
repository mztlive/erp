"use client"

import * as React from "react"
import { parsePriceFilterKind, type PriceFilterKind } from "@/lib/price-filter"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"

import { useListUrl } from "@/features/master-data/hooks/use-list-url"
import { productSalesPriceRangeError } from "@/features/master-data/lib/list-filters"
import {
    parseSellableDeliveryExtent,
    type SellableDeliveryExtent,
} from "@/features/master-data/lib/sellable-delivery-filter"
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
    | "deliveryRegion"
    | "supplyPreset"
    | "salesPrice"

/** 公司商品池：搜索 + 类型 / 分类 / 品牌 / 配送地区 / 售价。 */
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
    const deliveryRegion =
        searchParams.get("deliveryRegion")?.trim() || undefined
    const deliveryExtent = parseSellableDeliveryExtent(
        searchParams.get("deliveryExtent"),
    )
    const includeFreightExtra = searchParams.get("freightExtra") === "1"
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
        deliveryRegion ||
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
            deliveryRegionDraft: deliveryRegion ?? "",
            deliveryExtentDraft: deliveryExtent as SellableDeliveryExtent,
            freightExtraDraft: includeFreightExtra,
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
                ...deliveryUrlPatch(
                    value.deliveryRegionDraft,
                    value.deliveryExtentDraft,
                    value.freightExtraDraft,
                ),
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
        deliveryRegionDraft,
        deliveryExtentDraft,
        freightExtraDraft,
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
    const setDeliveryRegionDraft = React.useCallback(
        (value: string) => form.setFieldValue("deliveryRegionDraft", value),
        [form],
    )
    const setDeliveryExtentDraft = React.useCallback(
        (value: SellableDeliveryExtent) =>
            form.setFieldValue("deliveryExtentDraft", value),
        [form],
    )
    const setFreightExtraDraft = React.useCallback(
        (value: boolean) => form.setFieldValue("freightExtraDraft", value),
        [form],
    )
    /** 配送地区三项一起设置、一起清除。 */
    const setDeliveryDrafts = React.useCallback(
        (region: string, extent: SellableDeliveryExtent, freight: boolean) => {
            setDeliveryRegionDraft(region)
            setDeliveryExtentDraft(extent)
            setFreightExtraDraft(freight)
        },
        [setDeliveryRegionDraft, setDeliveryExtentDraft, setFreightExtraDraft],
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
            if (key === "deliveryRegion") setDeliveryDrafts("", "FULL", false)
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
                    : key === "deliveryRegion"
                      ? { ...deliveryUrlPatch("", "FULL", false), page: null }
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
            setDeliveryDrafts,
        ],
    )

    /** 仅重置更多条件的草稿；保留关键词、商品类型、分类及当前结果。 */
    const resetMoreFilters = React.useCallback(() => {
        setProductBrandIdDraft(null)
        setProductPriceKindDraft("dropship")
        setProductSalesPriceMinDraft("")
        setProductSalesPriceMaxDraft("")
        setDeliveryDrafts("", "FULL", false)
        setProductSalesPriceError(null)
    }, [
        setProductBrandIdDraft,
        setProductPriceKindDraft,
        setProductSalesPriceMinDraft,
        setProductSalesPriceMaxDraft,
        setDeliveryDrafts,
    ])

    /** 取消、外点和 Esc 只恢复低频草稿，保留搜索、商品类型和分类。 */
    const cancelMoreFilters = React.useCallback(() => {
        setProductBrandIdDraft(productBrandId ?? null)
        setDeliveryDrafts(
            deliveryRegion ?? "",
            deliveryExtent,
            includeFreightExtra,
        )
        setProductPriceKindDraft(productPriceKind)
        setProductSalesPriceMinDraft(productSalesPriceMin ?? "")
        setProductSalesPriceMaxDraft(productSalesPriceMax ?? "")
        setProductSalesPriceError(null)
        setSellableFilterPanelOpen(false)
    }, [
        deliveryExtent,
        deliveryRegion,
        includeFreightExtra,
        setDeliveryDrafts,
        productBrandId,
        productSalesPriceMax,
        productPriceKind,
        productSalesPriceMin,
        setProductBrandIdDraft,
        setProductSalesPriceMaxDraft,
        setProductPriceKindDraft,
        setProductSalesPriceMinDraft,
    ])

    const hasPendingChanges =
        searchDraft.trim() !== q.trim() ||
        productKindDraft !== (productKind ?? "all") ||
        productCategoryIdDraft !== (productCategoryId ?? null) ||
        productBrandIdDraft !== (productBrandId ?? null) ||
        deliveryRegionDraft !== (deliveryRegion ?? "") ||
        (Boolean(deliveryRegionDraft) &&
            (deliveryExtentDraft !== deliveryExtent ||
                freightExtraDraft !== includeFreightExtra)) ||
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
        setDeliveryDrafts("", "FULL", false)
        setProductSalesPriceError(null)
        setSellableFilterPanelOpen(false)
        patchUrl({
            ...deliveryUrlPatch("", "FULL", false),
            q: null,
            productKind: null,
            eligibilityAsOf: null,
            productCategoryId: null,
            productBrandId: null,
            productSupplierId: null,
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
        setDeliveryDrafts,
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
        setDeliveryDrafts(
            deliveryRegion ?? "",
            deliveryExtent,
            includeFreightExtra,
        )
    }, [deliveryRegion, deliveryExtent, includeFreightExtra, setDeliveryDrafts])

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
        deliveryRegion,
        deliveryExtent,
        includeFreightExtra,
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
        deliveryRegionDraft,
        setDeliveryRegionDraft,
        deliveryExtentDraft,
        setDeliveryExtentDraft,
        freightExtraDraft,
        setFreightExtraDraft,
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

/** 未选地区时范围与运费另计不进入地址栏；默认值也不写入。 */
function deliveryUrlPatch(
    region: string,
    extent: SellableDeliveryExtent,
    includeFreightExtra: boolean,
) {
    return {
        deliveryRegion: region || null,
        deliveryExtent: region && extent === "PARTIAL" ? "PARTIAL" : null,
        freightExtra: region && includeFreightExtra ? "1" : null,
    }
}
