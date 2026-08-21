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
 * 解析结果按 searchParams 身份 memo 化，保证数组型参数（factType 等）
 * 在 URL 未变化时保持稳定引用，供筛选草稿回填 effect 使用。
 */
export function useConsumptionOrdersUrlState() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const parsed = React.useMemo(() => {
        const qParam = searchParams.get("q") ?? ""
        const mallId = searchParams.get("mall") ?? "all"
        const fulfillmentChain = searchParams.get("fulfillmentChain") ?? "all"
        const attributionStatus = searchParams.get("attributionStatus") ?? "all"
        const paymentSource = searchParams.get("paymentSource") ?? "all"
        const costBasis = searchParams.get("costBasis") ?? "all"
        const occurredFrom = searchParams.get("occurredFrom") ?? ""
        const occurredTo = searchParams.get("occurredTo") ?? ""
        const factTypes = parseMultiValue(
            searchParams.get("factType"),
            FACT_TYPES,
        )
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
            pageFromUrl,
            sizeFromUrl,
        }
    }, [searchParams])

    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: parsed.pageFromUrl - 1,
        pageSize: parsed.sizeFromUrl,
    })

    React.useEffect(() => {
        setPagination((p) =>
            p.pageIndex === parsed.pageFromUrl - 1
                ? p
                : { ...p, pageIndex: parsed.pageFromUrl - 1 },
        )
    }, [parsed.pageFromUrl])

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
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
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
            router.replace(qs ? `${pathname}?${qs}` : pathname, {
                scroll: false,
            })
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

    const toggleMetric = React.useCallback(
        (key: MallConsumptionOrderMetricKey) => {
            replaceParams({ metric: parsed.metric === key ? undefined : key })
        },
        [parsed.metric, replaceParams],
    )

    const listQueryInput: MallConsumptionOrderListQuery = React.useMemo(
        () => ({
            q: parsed.qParam || undefined,
            mallIds: parsed.mallId === "all" ? undefined : [parsed.mallId],
            occurredFrom: parsed.occurredFrom || undefined,
            occurredTo: parsed.occurredTo || undefined,
            factTypes: parsed.factTypes.length ? parsed.factTypes : undefined,
            fulfillmentChains:
                parsed.fulfillmentChain === "all"
                    ? undefined
                    : [parsed.fulfillmentChain as FulfillmentChain],
            attributionStatuses:
                parsed.attributionStatus === "all"
                    ? undefined
                    : [parsed.attributionStatus as AttributionStatus],
            paymentSources:
                parsed.paymentSource === "all"
                    ? undefined
                    : [parsed.paymentSource as PaymentSourceFilter],
            supplierStatuses: parsed.supplierStatuses.length
                ? parsed.supplierStatuses
                : undefined,
            costBases:
                parsed.costBasis === "all"
                    ? undefined
                    : [parsed.costBasis as CostBasis],
            dataSources: parsed.dataSources.length
                ? parsed.dataSources
                : undefined,
            metric: parsed.metric === "all" ? undefined : parsed.metric,
            page: pagination.pageIndex + 1,
            pageSize: pagination.pageSize,
            sort: "occurredAt.desc",
        }),
        [
            parsed.attributionStatus,
            parsed.costBasis,
            parsed.dataSources,
            parsed.factTypes,
            parsed.fulfillmentChain,
            parsed.mallId,
            parsed.metric,
            parsed.occurredFrom,
            parsed.occurredTo,
            parsed.paymentSource,
            parsed.qParam,
            parsed.supplierStatuses,
            pagination.pageIndex,
            pagination.pageSize,
        ],
    )

    const hasActiveFilters = Boolean(
        parsed.qParam ||
            parsed.mallId !== "all" ||
            parsed.occurredFrom ||
            parsed.occurredTo ||
            parsed.factTypes.length > 0 ||
            parsed.fulfillmentChain !== "all" ||
            parsed.attributionStatus !== "all" ||
            parsed.supplierStatuses.length > 0 ||
            parsed.paymentSource !== "all" ||
            parsed.costBasis !== "all" ||
            parsed.dataSources.length > 0 ||
            parsed.metric !== "all",
    )

    const listReturnHref = React.useMemo(() => {
        const qs = searchParams.toString()
        return qs ? `${pathname}?${qs}` : pathname
    }, [pathname, searchParams])

    return {
        ...parsed,
        pagination,
        listQueryInput,
        hasActiveFilters,
        listReturnHref,
        replaceParams,
        handlePaginationChange,
        openPreview,
        closePreview,
        toggleMetric,
    }
}
