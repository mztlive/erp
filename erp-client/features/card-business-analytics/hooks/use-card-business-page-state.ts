"use client"

import * as React from "react"
import type { PaginationState, SortingState } from "@tanstack/react-table"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"

import {
    parseCostBasis,
    parseCoverage,
    parseDateBasis,
    parseDimension,
    parseExpiry,
    parsePreset,
    resolvePeriod,
} from "../lib/url-state"
import type {
    CardBusinessAnalyticsQuery,
    DateBasis,
    DateBasisConfig,
    PeriodPreset,
} from "../types"

/**
 * W28 页面状态聚合：URL 参数解析、分页/排序接线、显式期间选择、
 * 日期口径默认值回填与查询对象派生。原为页面组件内的状态簇。
 */
export function useCardBusinessPageState(
    basisConfig: DateBasisConfig | undefined,
    basisIsSuccess: boolean,
) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const periodPreset = parsePreset(searchParams.get("periodPreset"))
    const periodPresetValue = searchParams.get("periodPreset") ?? ""
    const periodFromUrl = searchParams.get("from")
    const periodToUrl = searchParams.get("to")
    const resolvedDefault = resolvePeriod(periodPreset)
    const from = periodFromUrl ?? ""
    const to = periodToUrl ?? ""

    const dateBasisUrl = parseDateBasis(searchParams.get("dateBasis"))
    const dimension = parseDimension(searchParams.get("dimension"))
    const customerId = searchParams.get("customerId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const costBasis = parseCostBasis(searchParams.get("costBasis"))
    const expiryState = parseExpiry(searchParams.get("expiryState"))
    const coverage = parseCoverage(searchParams.get("coverage"))
    const sort = searchParams.get("sort") ?? "consumption:desc"

    const [explicitFrom, setExplicitFrom] = React.useState(
        from || resolvedDefault.from,
    )
    const [explicitTo, setExplicitTo] = React.useState(to || resolvedDefault.to)
    const [explicitDateBasis, setExplicitDateBasis] = React.useState<DateBasis>(
        dateBasisUrl || "consumption",
    )
    const pageFromUrl = Math.max(
        1,
        Number(searchParams.get("page") ?? "1") || 1,
    )
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: pageFromUrl - 1,
        pageSize: 50,
    })

    // 服务端已配置默认 dateBasis 且 URL 缺省时写入（非静默猜口径）
    React.useEffect(() => {
        if (!basisConfig) return
        if (dateBasisUrl && from && to) return
        if (
            basisConfig.configuredDateBasis &&
            (!dateBasisUrl || !from || !to)
        ) {
            const next = new URLSearchParams(searchParams.toString())
            if (!dateBasisUrl) {
                next.set("dateBasis", basisConfig.configuredDateBasis)
            }
            if (!from) next.set("from", resolvedDefault.from)
            if (!to) next.set("to", resolvedDefault.to)
            router.replace(`${pathname}?${next.toString()}`)
        }
    }, [
        basisConfig,
        dateBasisUrl,
        from,
        to,
        searchParams,
        pathname,
        router,
        resolvedDefault.from,
        resolvedDefault.to,
    ])

    const allowedCodes = React.useMemo(
        () => new Set(basisConfig?.allowedDateBases.map((b) => b.code) ?? []),
        [basisConfig],
    )

    const dateBasisValid =
        Boolean(dateBasisUrl) && allowedCodes.has(dateBasisUrl as DateBasis)

    const periodComplete = Boolean(from && to && dateBasisValid)

    /** Q2：默认未配置且用户尚未显式选择完整 from/to/dateBasis */
    const analysisBlocked =
        basisIsSuccess &&
        !basisConfig?.configuredDateBasis &&
        !periodComplete

    const analysisReady = periodComplete && !analysisBlocked

    const analysisQuery: CardBusinessAnalyticsQuery | null = analysisReady
        ? {
              from,
              to,
              dateBasis: dateBasisUrl as DateBasis,
              dimension,
              customerId,
              salesOrderId,
              costBasis,
              expiryState,
              coverage,
              sort,
              page: pagination.pageIndex + 1,
              pageSize: pagination.pageSize,
          }
        : null

    function patchUrl(
        patch: Record<string, string | null | undefined>,
        options?: { replace?: boolean },
    ) {
        // P1/P2/P6：筛选变更恒 replace；page 入 URL，任何筛选变更回第 1 页（删除 page 即省略）
        patchSearchParams(
            { router, pathname, searchParams },
            { ...patch, page: null },
            { replace: true, ...options },
        )
        setPagination((p) => (p.pageIndex === 0 ? p : { ...p, pageIndex: 0 }))
    }

    // P6：分页写 URL（page），URL 回读同步本地分页
    React.useEffect(() => {
        setPagination((p) =>
            p.pageIndex === pageFromUrl - 1
                ? p
                : { ...p, pageIndex: pageFromUrl - 1 },
        )
    }, [pageFromUrl])

    const handlePaginationChange = React.useCallback(
        (next: PaginationState) => {
            setPagination(next)
            const sp = new URLSearchParams(searchParams.toString())
            if (next.pageIndex <= 0) sp.delete("page")
            else sp.set("page", String(next.pageIndex + 1))
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    // 表头排序 ↔ URL sort 双向接线：排序作用于服务端全量分组行，不只当前页
    const tableSorting = React.useMemo<SortingState>(() => {
        const [id, dir] = sort.split(":")
        return [{ id, desc: dir === "desc" }]
    }, [sort])

    const handleTableSortingChange = React.useCallback(
        (next: SortingState) => {
            const nextSort = next[0]
            patchUrl({
                sort: nextSort
                    ? `${nextSort.id}:${nextSort.desc ? "desc" : "asc"}`
                    : "consumption:desc",
            })
            setPagination((p) => ({ ...p, pageIndex: 0 }))
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [sort],
    )

    function applyExplicitPeriod() {
        if (!explicitFrom || !explicitTo || !explicitDateBasis) return
        if (explicitFrom > explicitTo) return
        patchUrl({
            from: explicitFrom,
            to: explicitTo,
            dateBasis: explicitDateBasis,
            periodPreset: null,
        })
    }

    function applyPreset(preset: PeriodPreset) {
        const p = resolvePeriod(preset)
        const basis =
            dateBasisUrl ||
            basisConfig?.configuredDateBasis ||
            explicitDateBasis ||
            "consumption"
        patchUrl({
            from: p.from,
            to: p.to,
            periodPreset: preset,
            dateBasis: basis,
        })
    }

    return {
        periodPreset,
        periodPresetValue,
        from,
        to,
        dateBasis: dateBasisUrl,
        dimension,
        customerId,
        salesOrderId,
        costBasis,
        expiryState,
        coverage,
        sort,
        explicitFrom,
        explicitTo,
        explicitDateBasis,
        pagination,
        tableSorting,
        analysisBlocked,
        analysisReady,
        analysisQuery,
        setExplicitFrom,
        setExplicitTo,
        setExplicitDateBasis,
        patchUrl,
        handlePaginationChange,
        handleTableSortingChange,
        applyExplicitPeriod,
        applyPreset,
    }
}

export type CardBusinessPageState = ReturnType<
    typeof useCardBusinessPageState
>
