"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState } from "@tanstack/react-table"

import {
    DATA_SOURCES,
    FACT_TYPES,
    SUPPLIER_STATUSES,
    parseMetric,
    parseMultiValue,
} from "@/features/mall-consumption-orders/lib/url-state"
import type {
    AttributionStatus,
    CostBasis,
    FulfillmentChain,
    MallConsumptionOrderListQuery,
    MallConsumptionOrderMetricKey,
    PaymentSourceFilter,
} from "@/features/mall-consumption-orders/types"

export type ReplaceParamsPatch = Record<string, string | undefined>

/**
 * 列表页 URL 状态：解析全部筛选参数、持有分页状态并把修改写回 URL。
 * URL 是唯一事实来源；预览（导航上下文）不随清筛选重置。
 */
export function useConsumptionOrdersUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const qParam = searchParams.get("q") ?? ""
    const mallId = searchParams.get("mall") ?? "all"
    const fulfillmentChain = searchParams.get("fulfillmentChain") ?? "all"
    const attributionStatus = searchParams.get("attributionStatus") ?? "all"
    const paymentSource = searchParams.get("paymentSource") ?? "all"
    const costBasis = searchParams.get("costBasis") ?? "all"
    const occurredFrom = searchParams.get("occurredFrom") ?? ""
    const occurredTo = searchParams.get("occurredTo") ?? ""
    const factTypes = parseMultiValue(searchParams.get("factType"), FACT_TYPES)
    const supplierStatuses = parseMultiValue(
        searchParams.get("supplierStatus"),
        SUPPLIER_STATUSES,
    )
    const dataSources = parseMultiValue(
        searchParams.get("dataSource"),
        DATA_SOURCES,
    )
    const periodSelected = Boolean(occurredFrom && occurredTo)
    const metric = parseMetric(searchParams.get("metric"))
    const previewId = searchParams.get("preview")
    const pageFromUrl = Math.max(
        1,
        Number(searchParams.get("page") ?? "1") || 1,
    )
    const sizeFromUrl = Math.max(
        1,
        Math.min(50, Number(searchParams.get("size") ?? "8") || 8),
    )
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageFromUrl - 1,
        pageSize: sizeFromUrl,
    })

    React.useEffect(() => {
        setPagination((p) =>
            p.pageIndex === pageFromUrl - 1
                ? p
                : { ...p, pageIndex: pageFromUrl - 1 },
        )
    }, [pageFromUrl])

    const replaceParams = React.useCallback(
        (patch: ReplaceParamsPatch, resetPage = true) => {
            const sp = new URLSearchParams(searchParams.toString())
            for (const [k, v] of Object.entries(patch)) {
                if (!v || v === "all") sp.delete(k)
                else sp.set(k, v)
            }
            if (resetPage) {
                sp.delete("page")
                setPagination((p) => ({ ...p, pageIndex: 0 }))
            }
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPagination(next)
            const sp = new URLSearchParams(searchParams.toString())
            if (next.pageIndex <= 0) sp.delete("page")
            else sp.set("page", String(next.pageIndex + 1))
            if (next.pageSize === 8) sp.delete("size")
            else sp.set("size", String(next.pageSize))
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    const openPreview = React.useCallback(
        (mallOrderId: string) => {
            replaceParams({ preview: mallOrderId }, false)
        },
        [replaceParams],
    )

    const closePreview = React.useCallback(() => {
        replaceParams({ preview: undefined }, false)
    }, [replaceParams])

    // P4：清全部筛选参数 + 分页回 1；预览（导航上下文）与视图参数保留
    const clearFilters = () => {
        replaceParams({
            q: undefined,
            mall: undefined,
            occurredFrom: undefined,
            occurredTo: undefined,
            factType: undefined,
            fulfillmentChain: undefined,
            attributionStatus: undefined,
            supplierStatus: undefined,
            paymentSource: undefined,
            costBasis: undefined,
            dataSource: undefined,
            metric: undefined,
        })
    }

    const toggleMetric = (key: MallConsumptionOrderMetricKey) => {
        replaceParams({ metric: metric === key ? undefined : key })
    }

    const listQueryInput: MallConsumptionOrderListQuery = React.useMemo(
        () => ({
            q: qParam || undefined,
            mallIds: mallId === "all" ? undefined : [mallId],
            occurredFrom: occurredFrom || undefined,
            occurredTo: occurredTo || undefined,
            factTypes: factTypes.length ? factTypes : undefined,
            fulfillmentChains:
                fulfillmentChain === "all"
                    ? undefined
                    : [fulfillmentChain as FulfillmentChain],
            attributionStatuses:
                attributionStatus === "all"
                    ? undefined
                    : [attributionStatus as AttributionStatus],
            paymentSources:
                paymentSource === "all"
                    ? undefined
                    : [paymentSource as PaymentSourceFilter],
            supplierStatuses: supplierStatuses.length
                ? supplierStatuses
                : undefined,
            costBases:
                costBasis === "all" ? undefined : [costBasis as CostBasis],
            dataSources: dataSources.length ? dataSources : undefined,
            metric: metric === "all" ? undefined : metric,
            page: pagination.pageIndex + 1,
            pageSize: pagination.pageSize,
            sort: "occurredAt.desc",
        }),
        [
            attributionStatus,
            costBasis,
            dataSources,
            factTypes,
            fulfillmentChain,
            mallId,
            metric,
            occurredFrom,
            occurredTo,
            pagination.pageIndex,
            pagination.pageSize,
            paymentSource,
            qParam,
            supplierStatuses,
        ],
    )

    const hasActiveFilters = Boolean(
        qParam ||
            mallId !== "all" ||
            occurredFrom ||
            occurredTo ||
            factTypes.length > 0 ||
            fulfillmentChain !== "all" ||
            attributionStatus !== "all" ||
            supplierStatuses.length > 0 ||
            paymentSource !== "all" ||
            costBasis !== "all" ||
            dataSources.length > 0 ||
            metric !== "all",
    )

    const listReturnHref = React.useMemo(() => {
        const qs = searchParams.toString()
        return qs ? `${pathname}?${qs}` : pathname
    }, [pathname, searchParams])

    return {
        qParam,
        mallId,
        fulfillmentChain,
        attributionStatus,
        paymentSource,
        costBasis,
        occurredFrom,
        occurredTo,
        factTypes,
        supplierStatuses,
        dataSources,
        periodSelected,
        metric,
        previewId,
        pagination,
        listQueryInput,
        hasActiveFilters,
        listReturnHref,
        replaceParams,
        handlePaginationChange,
        openPreview,
        closePreview,
        clearFilters,
        toggleMetric,
    }
}
