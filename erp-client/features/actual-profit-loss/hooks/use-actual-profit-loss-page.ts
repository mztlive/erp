"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { buildProfitLossColumns } from "@/features/actual-profit-loss/hooks/columns"
import {
    useCostEntriesForRowQuery,
    usePeriodBasisConfigQuery,
    useProfitLossViewQuery,
    useStartProfitLossExportMutation,
} from "@/features/actual-profit-loss/hooks/queries"
import { buildProfitLossCsv } from "@/features/actual-profit-loss/lib/csv"
import {
    mapFreshnessState,
    parseCoverage,
    parseDimension,
    parsePreset,
    resolvePeriod,
} from "@/features/actual-profit-loss/lib/url-state"
import type {
    CostEntryDetail,
    ProfitLossCoverage,
    ProfitLossExportJob,
    ProfitLossQuery,
    ProfitLossRow,
} from "@/features/actual-profit-loss/types"

export type ProfitLossUrlPatch = Record<string, string | null | undefined>

/**
 * 实际经营盈亏页的状态编排：URL 参数解析、查询/变更 hook 接线、
 * 搜索防抖、表头排序 ↔ URL 双向同步、成本下钻与导出流程。
 * 页面只消费返回值做渲染，不持有业务状态。
 */
export function useActualProfitLossPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const periodPreset = parsePreset(searchParams.get("periodPreset"))
    const periodFromUrl = searchParams.get("from")
    const periodToUrl = searchParams.get("to")
    const resolvedDefault = resolvePeriod(periodPreset)
    const from = periodFromUrl ?? resolvedDefault.from
    const to = periodToUrl ?? resolvedDefault.to

    const periodBasisUrl = searchParams.get("periodBasis") ?? ""
    const coverage = parseCoverage(searchParams.get("coverage"))
    const dimension = parseDimension(searchParams.get("dimension"))
    const customerId = searchParams.get("customerId") ?? undefined
    const salesOrderId = searchParams.get("salesOrderId") ?? undefined
    const qParam = searchParams.get("q") ?? ""
    const sort = searchParams.get("sort") ?? "actualProfitLossNet:asc"

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [pagination, setPagination] = React.useState<PaginationState>({
        pageIndex: 0,
        pageSize: 20,
    })
    const [costDetailRow, setCostDetailRow] =
        React.useState<ProfitLossRow | null>(null)
    const [selectedCostEntryId, setSelectedCostEntryId] = React.useState<
        string | null
    >(null)
    const [exportJob, setExportJob] =
        React.useState<ProfitLossExportJob | null>(null)
    const [refreshFailed, setRefreshFailed] = React.useState<string | null>(
        null,
    )
    const [refreshing, setRefreshing] = React.useState(false)
    const rowFocusRef = React.useRef<Map<string, HTMLElement | null>>(new Map())
    const restoreFocusIdRef = React.useRef<string | null>(null)

    const basisQuery = usePeriodBasisConfigQuery()
    const basisConfig = basisQuery.data

    // 服务端已配置时，若 URL 无 periodBasis，写入配置值（非静默猜口径）
    React.useEffect(() => {
        if (!basisConfig) return
        if (periodBasisUrl) return
        if (basisConfig.configuredPeriodBasis) {
            const next = new URLSearchParams(searchParams.toString())
            next.set("periodBasis", basisConfig.configuredPeriodBasis)
            if (!next.get("from")) next.set("from", from)
            if (!next.get("to")) next.set("to", to)
            router.replace(`${pathname}?${next.toString()}`)
        }
    }, [basisConfig, periodBasisUrl, searchParams, pathname, router, from, to])

    const allowedCodes = React.useMemo(
        () => new Set(basisConfig?.allowedPeriodBases.map((b) => b.code) ?? []),
        [basisConfig],
    )

    const periodBasisValid =
        Boolean(periodBasisUrl) && allowedCodes.has(periodBasisUrl)

    const analysisBlocked =
        basisQuery.isSuccess &&
        !basisConfig?.configuredPeriodBasis &&
        !periodBasisValid

    const analysisReady = periodBasisValid && !analysisBlocked

    const plQuery: ProfitLossQuery | null = analysisReady
        ? {
              from,
              to,
              periodBasis: periodBasisUrl,
              scopeId: "org-hq-finance",
              coverage,
              customerId,
              salesOrderId,
              dimension,
              q: qParam || undefined,
              sort,
              pageSize: 20,
          }
        : null

    const viewQuery = useProfitLossViewQuery(plQuery, analysisReady)
    const exportMutation = useStartProfitLossExportMutation()

    const costIds = costDetailRow?.costEntryIds ?? []
    const costEntriesQuery = useCostEntriesForRowQuery(costIds)

    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchInput === qParam) return
            patchUrl({ q: searchInput.trim() || null })
            setPagination((p) => ({ ...p, pageIndex: 0 }))
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchInput])

    function patchUrl(
        patch: ProfitLossUrlPatch,
        options?: { replace?: boolean },
    ) {
        // P2：筛选/分页/搜索变更默认 replace；需要 push 时显式传 options
        patchSearchParams({ router, pathname, searchParams }, patch, {
            replace: options?.replace ?? true,
        })
    }

    // 表头排序 ↔ URL sort 双向接线：排序作用于服务端全量行（按维度分组后），不只当前页
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
                    : "actualProfitLossNet:asc",
            })
            setPagination((p) => ({ ...p, pageIndex: 0 }))
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [sort],
    )

    const [exportFailed, setExportFailed] = React.useState<string | null>(null)

    // 关闭 detail 后恢复焦点
    React.useEffect(() => {
        if (costDetailRow) return
        const id = restoreFocusIdRef.current
        if (!id) return
        const el = rowFocusRef.current.get(id)
        if (el) {
            el.focus()
            restoreFocusIdRef.current = null
        }
    }, [costDetailRow])

    const data = viewQuery.data

    /** P4 清除范围：清 q/customerId/salesOrderId/coverage；保留期间/维度/sort。 */
    const hasFilters = Boolean(
        qParam.trim() || customerId || salesOrderId || coverage !== "covered",
    )

    const clearFilters = React.useCallback(() => {
        setSearchInput("")
        setPagination((p) => ({ ...p, pageIndex: 0 }))
        patchUrl({
            q: null,
            customerId: null,
            salesOrderId: null,
            coverage: null,
        })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [searchParams, pathname])

    const pageRows = React.useMemo(() => {
        if (!data) return []
        const start = pagination.pageIndex * pagination.pageSize
        return data.rows.items.slice(start, start + pagination.pageSize)
    }, [data, pagination.pageIndex, pagination.pageSize])

    const openCostDetail = React.useCallback((row: ProfitLossRow) => {
        if (
            !row.allowedDrilldowns.includes("cost_entry") ||
            row.costEntryIds.length === 0
        ) {
            return
        }
        restoreFocusIdRef.current = row.rowId
        setCostDetailRow(row)
        setSelectedCostEntryId(row.costEntryIds[0] ?? null)
    }, [])

    const columns = buildProfitLossColumns({ openCostDetail, rowFocusRef })

    const freshnessUi = data
        ? mapFreshnessState(data.freshness.state, {
              refreshFailed: Boolean(refreshFailed),
              refreshing,
          })
        : { uiState: "unknown" as const, statusLabel: "等待查询" }

    const selectedEntry: CostEntryDetail | null = React.useMemo(() => {
        if (!costEntriesQuery.data || !selectedCostEntryId) return null
        return (
            costEntriesQuery.data.find(
                (e) => e.costEntryId === selectedCostEntryId,
            ) ?? null
        )
    }, [costEntriesQuery.data, selectedCostEntryId])

    async function handleRefresh() {
        setRefreshing(true)
        setRefreshFailed(null)
        try {
            await viewQuery.refetch()
            await basisQuery.refetch()
        } catch (error) {
            setRefreshFailed(
                getErrorMessage(error, "刷新失败，已保留上次成功数据。"),
            )
        } finally {
            setRefreshing(false)
        }
    }

    async function handleExport() {
        if (!data || !plQuery || !analysisReady) return
        if (!data.fieldPermissions.canExport) return
        setExportFailed(null)
        try {
            const job = await exportMutation.mutateAsync({
                query: plQuery,
                view: data,
                coverage,
            })
            setExportJob(job)

            const csv = buildProfitLossCsv(data, job.watermark, coverage)
            const url = URL.createObjectURL(
                new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
            )
            const anchor = document.createElement("a")
            anchor.href = url
            anchor.download = `实际盈亏-非卡券不含税-${job.watermark.periodFrom}_${job.watermark.periodTo}.csv`
            anchor.click()
            URL.revokeObjectURL(url)
        } catch (error) {
            setExportFailed(
                getErrorMessage(error, "未能生成导出文件，请稍后重试。"),
            )
        }
    }

    function resetPagination() {
        setPagination((p) => ({ ...p, pageIndex: 0 }))
    }

    function handleCoverageChange(v: string) {
        patchUrl({ coverage: (v ?? coverage) as ProfitLossCoverage })
        resetPagination()
    }

    function handleSearchCommit() {
        patchUrl({ q: searchInput.trim() || null })
        resetPagination()
    }

    function handleDimensionChange(v: string) {
        patchUrl({ dimension: v })
        resetPagination()
    }

    function handleClearCustomer() {
        patchUrl({ customerId: null })
        resetPagination()
    }

    function handleClearSalesOrder() {
        patchUrl({ salesOrderId: null })
        resetPagination()
    }

    return {
        basisQuery,
        basisConfig,
        viewQuery,
        exportMutation,
        costEntriesQuery,
        data,
        periodPresetRaw: searchParams.get("periodPreset") ?? "",
        from,
        to,
        periodBasisUrl,
        periodBasisValid,
        coverage,
        dimension,
        customerId,
        salesOrderId,
        analysisBlocked,
        analysisReady,
        searchInput,
        setSearchInput,
        searchInputRef,
        pagination,
        setPagination,
        costDetailRow,
        setCostDetailRow,
        selectedCostEntryId,
        setSelectedCostEntryId,
        exportJob,
        setExportJob,
        exportFailed,
        refreshFailed,
        tableSorting,
        handleTableSortingChange,
        handleRefresh,
        handleExport,
        hasFilters,
        clearFilters,
        pageRows,
        columns,
        freshnessUi,
        selectedEntry,
        openCostDetail,
        patchUrl,
        resetPagination,
        handleCoverageChange,
        handleSearchCommit,
        handleDimensionChange,
        handleClearCustomer,
        handleClearSalesOrder,
    }
}
