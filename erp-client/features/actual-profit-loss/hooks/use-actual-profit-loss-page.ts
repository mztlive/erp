"use client"

import * as React from "react"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import type { ComboboxOption } from "@/components/business/option-combobox"
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
    parseCsvValues,
    parseDimension,
    parsePreset,
    resolvePeriod,
    serializeCsvValues,
} from "@/features/actual-profit-loss/lib/url-state"
import { COVERAGE_FILTER_LABEL } from "@/features/actual-profit-loss/types"
import type {
    CostEntryDetail,
    ProfitLossCoverage,
    ProfitLossExportJob,
    ProfitLossQuery,
    ProfitLossRow,
} from "@/features/actual-profit-loss/types"

export type ProfitLossUrlPatch = Record<string, string | null | undefined>

/** 已生效条件 chip：key 可被 removeFilter 单独撤销。 */
export type ProfitLossAppliedChip = Readonly<{
    key: string
    label: string
}>

const FULFILLMENT_MODE_CHIP_PREFIX = "fulfillmentMode:"
const COST_TYPE_CHIP_PREFIX = "costType:"

/**
 * 实际经营盈亏页的状态编排：URL 参数解析、查询/变更 hook 接线、
 * 搜索与结构化筛选草稿、面板 UI 态、表头排序 ↔ URL 双向同步、成本下钻与导出流程。
 *
 * 状态模型（docs/ui-filter-design.md §5）：
 * - Applied：URL 查询参数，唯一事实源；query / 导出 / 计数 / chip / 空态只读它。
 * - Draft：搜索与「更多筛选」字段的本地受控草稿，变化不触发请求。
 * - UI：面板展开态、分页等本地 state。
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

    // 结构化明细筛选（更多筛选面板）：福利场景 / 履约方式 / 成本类型
    const benefitScenario =
        searchParams.get("benefitScenario")?.trim() || undefined
    const fulfillmentModes = React.useMemo(
        () => parseCsvValues(searchParams.get("fulfillmentMode")),
        [searchParams],
    )
    const costTypes = React.useMemo(
        () => parseCsvValues(searchParams.get("costType")),
        [searchParams],
    )
    const hasStructuredFilters = Boolean(
        benefitScenario ||
            fulfillmentModes.length > 0 ||
            costTypes.length > 0,
    )

    const [searchInput, setSearchInput] = React.useState(qParam)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const [benefitScenarioDraft, setBenefitScenarioDraft] = React.useState(
        benefitScenario ?? "",
    )
    const [fulfillmentModesDraft, setFulfillmentModesDraft] = React.useState<
        string[]
    >(() => [...fulfillmentModes])
    const [costTypesDraft, setCostTypesDraft] = React.useState<string[]>(() => [
        ...costTypes,
    ])
    // 初始深链带结构化条件时展开面板；URL 回填 effect 不触碰该 UI 态
    const [filterPanelOpen, setFilterPanelOpen] = React.useState(
        hasStructuredFilters,
    )
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
              benefitScenario,
              fulfillmentModes:
                  fulfillmentModes.length > 0 ? fulfillmentModes : undefined,
              costTypes: costTypes.length > 0 ? costTypes : undefined,
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

    const data = viewQuery.data

    // —— URL 回填（仅同步草稿；不重置面板展开态，见 docs/ui-filter-design.md §5.4）——
    // 关键词只随 q 本身回填：其它 URL 条件变化不得覆盖正在编辑的搜索草稿
    React.useEffect(() => {
        setSearchInput(qParam)
    }, [qParam])

    React.useEffect(() => {
        setBenefitScenarioDraft(benefitScenario ?? "")
        setFulfillmentModesDraft([...fulfillmentModes])
        setCostTypesDraft([...costTypes])
    }, [benefitScenario, costTypes, fulfillmentModes])

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
            // 弹层（Dialog / Sheet / Popover / Combobox）打开时不得聚焦背景搜索框
            if (
                document.querySelector(
                    '[role="dialog"], [data-slot="sheet"], [data-slot="popover-content"], [data-slot="combobox-content"]',
                )
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    function patchUrl(
        patch: ProfitLossUrlPatch,
        options?: { replace?: boolean },
    ) {
        // 筛选/搜索变更一律 replace 且不跳动滚动位置
        patchSearchParams({ router, pathname, searchParams }, patch, {
            replace: options?.replace ?? true,
            scroll: false,
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

    /** 已生效筛选：搜索、覆盖口径、来源锁定与结构化条件。 */
    const hasFilters = Boolean(
        qParam.trim() ||
            customerId ||
            salesOrderId ||
            coverage !== "covered" ||
            hasStructuredFilters,
    )

    /**
     * 唯一提交路径：收起态 Enter / 搜索框尾部箭头 / 展开态「应用全部筛选」
     * 都调用本函数，一次性写 q 与全部「更多筛选」草稿，成功后收起面板。
     */
    const applyFilters = React.useCallback(() => {
        patchUrl({
            q: searchInput.trim() || null,
            benefitScenario: benefitScenarioDraft.trim() || null,
            fulfillmentMode: serializeCsvValues(fulfillmentModesDraft) || null,
            costType: serializeCsvValues(costTypesDraft) || null,
        })
        setFilterPanelOpen(false)
        setPagination((p) => ({ ...p, pageIndex: 0 }))
    }, [
        benefitScenarioDraft,
        costTypesDraft,
        fulfillmentModesDraft,
        // eslint-disable-next-line react-hooks/exhaustive-deps
        patchUrl,
        searchInput,
    ])

    /**
     * 清空全部：草稿、面板、URL 筛选参数与分页一起重置；
     * 保留期间、维度（视图）与排序等导航上下文。
     */
    const clearAllFilters = React.useCallback(() => {
        setSearchInput("")
        setBenefitScenarioDraft("")
        setFulfillmentModesDraft([])
        setCostTypesDraft([])
        setFilterPanelOpen(false)
        setPagination((p) => ({ ...p, pageIndex: 0 }))
        patchUrl({
            q: null,
            coverage: null,
            customerId: null,
            salesOrderId: null,
            benefitScenario: null,
            fulfillmentMode: null,
            costType: null,
        })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    /** 只清「更多筛选」结构化条件；保留关键词与覆盖快捷筛选，面板保持展开。 */
    const resetMoreFilters = React.useCallback(() => {
        setBenefitScenarioDraft("")
        setFulfillmentModesDraft([])
        setCostTypesDraft([])
        setPagination((p) => ({ ...p, pageIndex: 0 }))
        patchUrl({
            benefitScenario: null,
            fulfillmentMode: null,
            costType: null,
        })
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [patchUrl])

    /** 单个 chip 只移除自己的已生效条件。 */
    const removeFilter = React.useCallback(
        (key: string) => {
            if (key === "q") {
                setSearchInput("")
                patchUrl({ q: null })
            } else if (key === "coverage") {
                patchUrl({ coverage: null })
            } else if (key === "customerId") {
                patchUrl({ customerId: null })
            } else if (key === "salesOrderId") {
                patchUrl({ salesOrderId: null })
            } else if (key === "benefitScenario") {
                setBenefitScenarioDraft("")
                patchUrl({ benefitScenario: null })
            } else if (key.startsWith(FULFILLMENT_MODE_CHIP_PREFIX)) {
                const value = key.slice(FULFILLMENT_MODE_CHIP_PREFIX.length)
                setFulfillmentModesDraft((current) =>
                    current.filter((item) => item !== value),
                )
                patchUrl({
                    fulfillmentMode:
                        serializeCsvValues(
                            fulfillmentModes.filter((item) => item !== value),
                        ) || null,
                })
            } else if (key.startsWith(COST_TYPE_CHIP_PREFIX)) {
                const value = key.slice(COST_TYPE_CHIP_PREFIX.length)
                setCostTypesDraft((current) =>
                    current.filter((item) => item !== value),
                )
                patchUrl({
                    costType:
                        serializeCsvValues(
                            costTypes.filter((item) => item !== value),
                        ) || null,
                })
            }
            setPagination((p) => ({ ...p, pageIndex: 0 }))
        },
        // eslint-disable-next-line react-hooks/exhaustive-deps
        [costTypes, fulfillmentModes, patchUrl],
    )

    const selectedCustomerLabel = React.useMemo(
        () =>
            data?.rows.items.find((row) => row.customerId === customerId)
                ?.customerLabel,
        [customerId, data?.rows.items],
    )
    const selectedSalesOrderLabel = React.useMemo(
        () =>
            data?.rows.items.find((row) => row.objectId === salesOrderId)
                ?.identityLabel,
        [data?.rows.items, salesOrderId],
    )
    const costTypeLabelMap = React.useMemo(
        () =>
            new Map(
                (data?.costComposition ?? []).map((item) => [
                    item.costType,
                    item.label,
                ]),
            ),
        [data?.costComposition],
    )

    /** 已生效条件全部显性化为可单独移除的 chip（含来源锁定参数）。 */
    const appliedChips = React.useMemo<readonly ProfitLossAppliedChip[]>(() => {
        const chips: ProfitLossAppliedChip[] = []
        const q = qParam.trim()
        if (q) chips.push({ key: "q", label: `搜索：${q}` })
        if (coverage !== "covered") {
            chips.push({
                key: "coverage",
                label: `覆盖：${COVERAGE_FILTER_LABEL[coverage]}`,
            })
        }
        if (customerId) {
            chips.push({
                key: "customerId",
                label: selectedCustomerLabel ?? "客户锁定",
            })
        }
        if (salesOrderId) {
            chips.push({
                key: "salesOrderId",
                label: selectedSalesOrderLabel ?? "销售单锁定",
            })
        }
        if (benefitScenario) {
            chips.push({
                key: "benefitScenario",
                label: `福利场景：${benefitScenario}`,
            })
        }
        for (const value of fulfillmentModes) {
            chips.push({
                key: `${FULFILLMENT_MODE_CHIP_PREFIX}${value}`,
                label: `履约方式：${value}`,
            })
        }
        for (const value of costTypes) {
            chips.push({
                key: `${COST_TYPE_CHIP_PREFIX}${value}`,
                label: `成本类型：${costTypeLabelMap.get(value) ?? value}`,
            })
        }
        return chips
    }, [
        benefitScenario,
        costTypeLabelMap,
        costTypes,
        coverage,
        customerId,
        fulfillmentModes,
        qParam,
        salesOrderId,
        selectedCustomerLabel,
        selectedSalesOrderLabel,
    ])

    // 「更多筛选」字段选项：取自当前视图数据，并固定已生效值避免无法撤销
    const benefitScenarioOptions = React.useMemo<readonly ComboboxOption[]>(
        () => {
            const seen = new Set<string>()
            const options: ComboboxOption[] = []
            for (const row of data?.rows.items ?? []) {
                for (const value of row.benefitScenarios ?? []) {
                    if (value && !seen.has(value)) {
                        seen.add(value)
                        options.push({ value, label: value })
                    }
                }
            }
            if (benefitScenario && !seen.has(benefitScenario)) {
                options.push({ value: benefitScenario, label: benefitScenario })
            }
            return options
        },
        [benefitScenario, data?.rows.items],
    )
    const fulfillmentModeOptions = React.useMemo<readonly ComboboxOption[]>(
        () => {
            const seen = new Set<string>()
            const options: ComboboxOption[] = []
            for (const row of data?.rows.items ?? []) {
                for (const value of row.fulfillmentModes ?? []) {
                    if (value && !seen.has(value)) {
                        seen.add(value)
                        options.push({ value, label: value })
                    }
                }
            }
            for (const value of fulfillmentModes) {
                if (!seen.has(value)) {
                    options.push({ value, label: value })
                }
            }
            return options
        },
        [data?.rows.items, fulfillmentModes],
    )
    const costTypeOptions = React.useMemo<readonly ComboboxOption[]>(() => {
        const seen = new Set<string>()
        const options: ComboboxOption[] = []
        for (const item of data?.costComposition ?? []) {
            if (item.costType && !seen.has(item.costType)) {
                seen.add(item.costType)
                options.push({
                    value: item.costType,
                    label: item.label || item.costType,
                })
            }
        }
        for (const value of costTypes) {
            if (!seen.has(value)) {
                options.push({
                    value,
                    label: costTypeLabelMap.get(value) ?? value,
                })
            }
        }
        return options
    }, [costTypeLabelMap, costTypes, data?.costComposition])

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
        const next = v as ProfitLossCoverage
        // 默认「成本完整」不写 URL；切回默认时删除参数
        patchUrl(next === "covered" ? { coverage: null } : { coverage: next })
        resetPagination()
    }

    function handleDimensionChange(v: string) {
        patchUrl({ dimension: v })
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
        benefitScenarioDraft,
        setBenefitScenarioDraft,
        fulfillmentModesDraft,
        setFulfillmentModesDraft,
        costTypesDraft,
        setCostTypesDraft,
        filterPanelOpen,
        setFilterPanelOpen,
        hasStructuredFilters,
        appliedChips,
        benefitScenarioOptions,
        fulfillmentModeOptions,
        costTypeOptions,
        applyFilters,
        clearAllFilters,
        resetMoreFilters,
        removeFilter,
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
        pageRows,
        columns,
        freshnessUi,
        selectedEntry,
        openCostDetail,
        patchUrl,
        resetPagination,
        handleCoverageChange,
        handleDimensionChange,
    }
}
