"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
    DownloadIcon,
    ExternalLinkIcon,
    RefreshCwIcon,
    SearchIcon,
} from "lucide-react"
import type { PaginationState, SortingState } from "@tanstack/react-table"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    DataFreshness,
    DataTable,
    FilterChip,
    ListToolbar,
    MetricItem,
    MetricStrip,
    OptionCombobox,
    PageActions,
    PageHeader,
    PageScaffold,
    QuickPreviewSheet,
    surfacePanelClassName,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage } from "@/lib/api/errors"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import { Progress } from "@/components/ui/progress"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    useCostEntriesForRowQuery,
    usePeriodBasisConfigQuery,
    useProfitLossViewQuery,
    useStartProfitLossExportMutation,
} from "@/features/actual-profit-loss/hooks/queries"
import {
    COVERAGE_FILTER_LABEL,
    COVERAGE_STATE_UI,
    DIMENSION_LABEL,
    type CostEntryDetail,
    type ProfitLossCoverage,
    type ProfitLossDimension,
    type ProfitLossExportJob,
    type ProfitLossQuery,
    type ProfitLossRow,
} from "@/features/actual-profit-loss/types"
import { CostEntryDetailBody } from "@/features/actual-profit-loss/components/cost-entry-detail-body"
import { buildProfitLossColumns } from "@/features/actual-profit-loss/hooks/columns"
import {
    basisLabel,
    coveragePercentNumber,
    mapFreshnessState,
    parseCoverage,
    parseDimension,
    parsePreset,
    resolvePeriod,
    W16_FORMULA_HINT,
} from "@/features/actual-profit-loss/lib/url-state"
import { buildProfitLossCsv } from "@/features/actual-profit-loss/lib/csv"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { ProfitLossChartsAndStageReference } from "@/features/actual-profit-loss/components/profit-loss-charts-and-stage-reference"
import {
    formatMoneyDisplay,
    PROFIT_LOSS_SCOPE_LABEL as SCOPE_LABEL,
} from "@/features/actual-profit-loss/lib/presentation"

export function ActualProfitLossPage() {
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
        patch: Record<string, string | null | undefined>,
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

    // —— 初载 / 配置加载 ——
    if (basisQuery.isPending) {
        return (
            <PageScaffold>
                <PageHeader
                    title={`实际经营盈亏（${SCOPE_LABEL}）`}
                    description="读取期间归属口径配置…"
                    breadcrumbs={[
                        {
                            id: "an",
                            label: "分析",
                            href: "/analytics/profit-loss",
                        },
                        { id: "pl", label: "实际经营盈亏", current: true },
                    ]}
                />
                <Skeleton className="h-16 w-full rounded-lg" />
                <Skeleton className="h-24 w-full rounded-lg" />
                <div className="grid gap-2">
                    {Array.from({ length: 5 }).map((_, i) => (
                        <Skeleton key={i} className="h-10 w-full rounded-lg" />
                    ))}
                </div>
            </PageScaffold>
        )
    }

    if (basisQuery.isError || !basisConfig) {
        return (
            <PageScaffold>
                <PageHeader
                    title={`实际经营盈亏（${SCOPE_LABEL}）`}
                    breadcrumbs={[
                        {
                            id: "an",
                            label: "分析",
                            href: "/analytics/profit-loss",
                        },
                        { id: "pl", label: "实际经营盈亏", current: true },
                    ]}
                />
                <BusinessFailureState
                    error={basisQuery.error}
                    title="期间归属口径配置读取失败"
                    action={
                        <Button
                            type="button"
                            onClick={() => void basisQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold>
            <PageHeader
                title={`实际经营盈亏（${SCOPE_LABEL}）`}
                breadcrumbs={[
                    { id: "an", label: "分析", href: "/analytics/profit-loss" },
                    { id: "pl", label: "实际经营盈亏", current: true },
                ]}
                metadata={
                    data ? (
                        <div className="flex flex-col gap-1">
                            <DataFreshness
                                updatedAt={formatDateTime(
                                    data.freshness.projectedAt,
                                    "full",
                                )}
                                dateTime={data.freshness.projectedAt}
                                state={freshnessUi.uiState}
                                statusLabel={freshnessUi.statusLabel}
                                label="经营汇总"
                            />
                        </div>
                    ) : (
                        <DataFreshness
                            updatedAt="—"
                            state="unknown"
                            label="经营汇总"
                            statusLabel="待选择口径"
                        />
                    )
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "refresh",
                                label: "刷新",
                                icon: RefreshCwIcon,
                                variant: "ghost",
                                className:
                                    "text-muted-foreground hover:text-foreground",
                                disabled: !analysisReady,
                                onClick: () => {
                                    void handleRefresh()
                                },
                            },
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled:
                                    !analysisReady ||
                                    !data ||
                                    !data.fieldPermissions.canExport ||
                                    data.rows.total === 0 ||
                                    exportMutation.isPending,
                                onClick: () => {
                                    void handleExport()
                                },
                            },
                        ]}
                    />
                }
            />

            {/* 期间 / 归属口径 */}
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>统计期间与归属口径</CardTitle>
                    <CardDescription>
                        查询与导出仅按此处明确的期间与归属口径执行。
                    </CardDescription>
                </CardHeader>
                <CardContent className="flex flex-col gap-3 pt-4 sm:flex-row sm:flex-wrap sm:items-end">
                    <div className="space-y-1.5">
                        <Label htmlFor="period-preset">期间快捷</Label>
                        <OptionCombobox
                            id="period-preset"
                            value={searchParams.get("periodPreset") ?? ""}
                            onValueChange={(v) => {
                                if (!v) return
                                const preset = parsePreset(v)
                                const range = resolvePeriod(preset)
                                patchUrl({
                                    periodPreset: preset,
                                    from: range.from,
                                    to: range.to,
                                })
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                            options={[
                                { value: "", label: "自定义" },
                                { value: "month-to-date", label: "本月迄今" },
                                { value: "last-month", label: "上月" },
                                { value: "quarter-to-date", label: "本季迄今" },
                            ]}
                            className="w-[10rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="期间快捷"
                            placeholder="期间快捷"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="period-from">从</Label>
                        <DatePicker
                            className="w-[10.5rem]"
                            value={from || undefined}
                            onValueChange={(next) => {
                                patchUrl({
                                    from: next || null,
                                    periodPreset: null,
                                })
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="period-to">至</Label>
                        <DatePicker
                            className="w-[10.5rem]"
                            value={to || undefined}
                            onValueChange={(next) => {
                                patchUrl({
                                    to: next || null,
                                    periodPreset: null,
                                })
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label>已解析期间</Label>
                        <p className="num text-sm font-medium">
                            {from} ~ {to}
                        </p>
                    </div>
                    <div className="min-w-[16rem] flex-1 space-y-1.5">
                        <Label htmlFor="period-basis">期间归属口径</Label>
                        <OptionCombobox
                            id="period-basis"
                            value={periodBasisUrl || ""}
                            onValueChange={(v) => {
                                patchUrl({ periodBasis: v || null })
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                            }}
                            options={[
                                {
                                    value: "",
                                    label: basisConfig.configuredPeriodBasis
                                        ? "请确认归属口径"
                                        : "请显式选择归属口径（未配置）",
                                },
                                ...basisConfig.allowedPeriodBases.map(
                                    (opt) => ({
                                        value: opt.code,
                                        label:
                                            opt.label +
                                            (basisConfig.configuredPeriodBasis ===
                                            opt.code
                                                ? "（默认口径）"
                                                : ""),
                                    }),
                                ),
                            ]}
                            className="w-full min-w-[16rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="期间归属口径"
                            placeholder="请选择归属口径"
                        />
                    </div>
                    {periodBasisValid ? (
                        <Badge variant="secondary">
                            口径已明确 · {basisConfig.configurationVersion}
                        </Badge>
                    ) : (
                        <Badge variant="destructive">分析与导出已阻断</Badge>
                    )}
                </CardContent>
            </Card>

            {/* 阻断：口径未配置且用户未显式选择 */}
            {analysisBlocked ? (
                <Alert variant="destructive">
                    <AlertTitle>期间归属口径尚未配置</AlertTitle>
                    <AlertDescription className="space-y-3">
                        <p>
                            尚未设置默认归属口径，请选择上方任一口径后开始分析。
                        </p>
                        <p className="text-xs text-muted-foreground">
                            公式说明：{W16_FORMULA_HINT}
                        </p>
                        <ul className="list-disc space-y-1 pl-5 text-sm">
                            {basisConfig.allowedPeriodBases.map((opt) => (
                                <li key={opt.code}>
                                    <button
                                        type="button"
                                        className="font-medium text-primary underline-offset-2 hover:underline"
                                        onClick={() =>
                                            patchUrl(
                                                { periodBasis: opt.code },
                                                { replace: true },
                                            )
                                        }
                                    >
                                        {opt.label}
                                    </button>
                                    <span className="text-muted-foreground">
                                        {" "}
                                        — {opt.explanation}
                                    </span>
                                </li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}

            {analysisBlocked ? (
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>公式与边界（查询阻断中）</CardTitle>
                        <CardDescription>选定口径后加载数据。</CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-2 text-sm text-muted-foreground">
                        <p>{W16_FORMULA_HINT}</p>
                        <p>
                            公式仅统计实际发生成本（冲减计入）；计划与已确认金额仅作对照。卡券与消费成本不在本页。
                        </p>
                    </CardContent>
                </Card>
            ) : null}

            {/* 分析主体：仅在口径就绪后 */}
            {analysisReady ? (
                <>
                    {viewQuery.isPending && !data ? (
                        <>
                            <Skeleton className="h-20 w-full" />
                            <div className="grid gap-4 xl:grid-cols-5">
                                {Array.from({ length: 5 }).map((_, i) => (
                                    <Skeleton key={i} className="h-24 w-full" />
                                ))}
                            </div>
                            <Skeleton className="h-64 w-full" />
                        </>
                    ) : null}

                    {viewQuery.isError && !data ? (
                        <BusinessFailureState
                            error={viewQuery.error}
                            title="盈亏数据加载失败"
                            action={
                                <Button
                                    type="button"
                                    onClick={() => void viewQuery.refetch()}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : null}

                    {data ? (
                        <>
                            {data.freshness.state === "stale" ||
                            refreshFailed ? (
                                <Alert variant="warning">
                                    <AlertTitle>
                                        {refreshFailed
                                            ? "刷新失败"
                                            : "数据陈旧"}
                                    </AlertTitle>
                                    <AlertDescription>
                                        {refreshFailed
                                            ? refreshFailed
                                            : `数据更新于 ${formatDateTime(data.freshness.projectedAt, "full")}，来源已于 ${formatDateTime(data.freshness.sourceWatermark, "full")} 更新。`}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {data.freshness.state === "rebuilding" ? (
                                <Alert>
                                    <AlertTitle>数据更新中</AlertTitle>
                                    <AlertDescription>
                                        更新中，已保留上次成功结果；导出将标注旧数据时间。
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {data.freshness.state === "failed" ? (
                                <Alert variant="destructive">
                                    <AlertTitle>数据更新失败</AlertTitle>
                                    <AlertDescription>
                                        经营记录未被修改；展示上次成功数据并标记失败。请联系管理员排查后台任务。
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {viewQuery.isError ? (
                                <Alert variant="destructive">
                                    <AlertTitle>数据更新失败</AlertTitle>
                                    <AlertDescription>
                                        {getErrorMessage(
                                            viewQuery.error,
                                            "已保留上次成功结果，未覆盖业务数据。请重试或调整筛选。",
                                        )}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {exportFailed ? (
                                <Alert variant="destructive">
                                    <AlertTitle>导出失败</AlertTitle>
                                    <AlertDescription>
                                        {exportFailed}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {data.correctionPendingNotice ? (
                                <Alert>
                                    <AlertTitle>来源纠错已登记</AlertTitle>
                                    <AlertDescription>
                                        {data.correctionPendingNotice}
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {exportJob ? (
                                <BackgroundJobProgress
                                    mode="all-or-nothing"
                                    status={
                                        exportJob.status === "queued"
                                            ? "queued"
                                            : exportJob.status === "running"
                                              ? "running"
                                              : exportJob.status === "succeeded"
                                                ? "succeeded"
                                                : "failed"
                                    }
                                    total={exportJob.total}
                                    completed={exportJob.completed}
                                    succeeded={
                                        exportJob.status === "succeeded"
                                            ? exportJob.total
                                            : undefined
                                    }
                                    label="实际经营盈亏导出"
                                    description={
                                        <>
                                            期间{" "}
                                            {exportJob.watermark.periodFrom}~
                                            {exportJob.watermark.periodTo} ·
                                            归属口径{" "}
                                            {basisLabel(
                                                exportJob.watermark.periodBasis,
                                            )}{" "}
                                            · 数据更新于{" "}
                                            {exportJob.watermark.projectedAt}
                                            {exportJob.downloadLabel ? (
                                                <span className="mt-1 block font-medium">
                                                    可下载：
                                                    {exportJob.downloadLabel}
                                                </span>
                                            ) : null}
                                        </>
                                    }
                                    action={
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="ghost"
                                            onClick={() => setExportJob(null)}
                                        >
                                            关闭
                                        </Button>
                                    }
                                />
                            ) : null}

                            <Alert>
                                <AlertTitle>
                                    成本覆盖 · {SCOPE_LABEL} · 可靠性{" "}
                                    {data.coverage.reliability === "reliable"
                                        ? "可靠"
                                        : data.coverage.reliability ===
                                            "partial"
                                          ? "部分可靠"
                                          : "不可用"}
                                </AlertTitle>
                                <AlertDescription className="space-y-3">
                                    <Progress
                                        value={coveragePercentNumber(
                                            data.coverage.coverageRate,
                                        )}
                                    >
                                        <span className="text-xs">
                                            成本覆盖率
                                        </span>
                                        <span className="num ml-auto text-sm">
                                            {data.coverage.coverageRate}
                                        </span>
                                    </Progress>
                                    <div className="grid gap-2 text-sm sm:grid-cols-3">
                                        <div>
                                            <span className="text-muted-foreground">
                                                覆盖收入{" "}
                                            </span>
                                            <span className="num font-medium">
                                                {formatMoneyDisplay(
                                                    data.coverage
                                                        .coveredNetRevenue,
                                                )}
                                            </span>
                                        </div>
                                        <div>
                                            <span className="text-muted-foreground">
                                                未覆盖收入{" "}
                                            </span>
                                            <span className="num font-medium">
                                                {formatMoneyDisplay(
                                                    data.coverage
                                                        .uncoveredNetRevenue,
                                                )}
                                            </span>
                                        </div>
                                        <div>
                                            <span className="text-muted-foreground">
                                                利润可靠性{" "}
                                            </span>
                                            <BusinessStatusBadge
                                                context="detail"
                                                label={
                                                    data.coverage
                                                        .coverageState ===
                                                    "complete"
                                                        ? "完整"
                                                        : data.coverage
                                                                .coverageState ===
                                                            "partial"
                                                          ? "部分覆盖"
                                                          : "完全未覆盖"
                                                }
                                                tone={
                                                    data.coverage
                                                        .coverageState ===
                                                    "complete"
                                                        ? "success"
                                                        : data.coverage
                                                                .coverageState ===
                                                            "partial"
                                                          ? "warning"
                                                          : "destructive"
                                                }
                                            />
                                        </div>
                                    </div>
                                    <p className="text-xs text-muted-foreground">
                                        缺失成本显示为未覆盖并注明原因，不会按零成本计利润。
                                    </p>
                                </AlertDescription>
                            </Alert>

                            <MetricStrip
                                columns={5}
                                aria-label="实际经营盈亏核心指标（非卡券·不含税）"
                            >
                                <MetricItem
                                    label="不含税销售收入"
                                    value={formatMoneyDisplay(
                                        data.totals.netSalesRevenue,
                                    )}
                                />
                                <MetricItem
                                    label="实际采购成本"
                                    value={
                                        data.totals.actualProcurementCostNet !=
                                        null
                                            ? formatMoneyDisplay(
                                                  data.totals
                                                      .actualProcurementCostNet,
                                              )
                                            : "无权限"
                                    }
                                    detail="实际发生+冲减 · 不含税"
                                />
                                <MetricItem
                                    label="实际履约费用"
                                    value={
                                        data.totals.actualFulfillmentCostNet !=
                                        null
                                            ? formatMoneyDisplay(
                                                  data.totals
                                                      .actualFulfillmentCostNet,
                                              )
                                            : "无权限"
                                    }
                                    detail="印刷/仓储/配送等 · 不含税"
                                />
                                <MetricItem
                                    label="实际经营盈亏"
                                    value={
                                        data.totals.actualProfitLossNet != null
                                            ? formatMoneyDisplay(
                                                  data.totals
                                                      .actualProfitLossNet,
                                              )
                                            : (data.totals
                                                  .marginUnavailableReason ??
                                              "不可用")
                                    }
                                    detail={
                                        data.coverage.reliability === "partial"
                                            ? "部分覆盖 · 仅可靠子集"
                                            : SCOPE_LABEL
                                    }
                                    status={
                                        data.totals.actualProfitLossNet !=
                                            null &&
                                        Number(
                                            data.totals.actualProfitLossNet,
                                        ) < 0
                                            ? {
                                                  label: "亏损",
                                                  tone: "destructive",
                                              }
                                            : data.coverage.reliability ===
                                                "partial"
                                              ? {
                                                    label: "部分可靠",
                                                    tone: "warning",
                                                }
                                              : undefined
                                    }
                                />
                                <MetricItem
                                    label="实际利润率"
                                    value={
                                        data.totals.marginRate ??
                                        data.totals.marginUnavailableReason ??
                                        "不适用"
                                    }
                                    detail="盈亏 / 适用不含税收入"
                                />
                            </MetricStrip>

                            <Alert>
                                <AlertTitle>口径说明</AlertTitle>
                                <AlertDescription className="text-xs leading-relaxed">
                                    {data.formulaText}
                                    <span className="mt-1 block">
                                        {data.excludedNote}
                                    </span>
                                    <span className="mt-1 block">
                                        当前范围：{data.filterSummary}
                                    </span>
                                </AlertDescription>
                            </Alert>

                            <ProfitLossChartsAndStageReference data={data} />
                            <BusinessTableFrame
                                title={`明细 · ${DIMENSION_LABEL[dimension]}（${SCOPE_LABEL}）`}
                                description={`共 ${data.rows.total} 行 · 明细与指标/汇总同一数据范围（趋势与构成图为固定口径序列）· 点击盈亏下钻销售单 · 点击成本金额打开成本记录详情`}
                                toolbar={
                                    <div className="space-y-2">
                                        <ListToolbar
                                            aria-label="盈亏明细筛选"
                                            search={
                                                <InputGroup className="max-w-sm">
                                                    <InputGroupAddon>
                                                        <SearchIcon className="size-4" />
                                                    </InputGroupAddon>
                                                    <InputGroupInput
                                                        ref={searchInputRef}
                                                        placeholder="搜索销售单号、客户（/）"
                                                        value={searchInput}
                                                        onChange={(e) =>
                                                            setSearchInput(
                                                                e.target.value,
                                                            )
                                                        }
                                                        onKeyDown={(e) => {
                                                            if (
                                                                e.key ===
                                                                "Enter"
                                                            ) {
                                                                patchUrl({
                                                                    q:
                                                                        searchInput.trim() ||
                                                                        null,
                                                                })
                                                                setPagination(
                                                                    (p) => ({
                                                                        ...p,
                                                                        pageIndex: 0,
                                                                    }),
                                                                )
                                                            }
                                                        }}
                                                        aria-label="搜索销售单或客户"
                                                    />
                                                </InputGroup>
                                            }
                                            filters={
                                                <>
                                                    <Label
                                                        htmlFor="coverage-filter"
                                                        className="sr-only"
                                                    >
                                                        成本覆盖
                                                    </Label>
                                                    <OptionCombobox
                                                        id="coverage-filter"
                                                        value={coverage}
                                                        onValueChange={(v) => {
                                                            patchUrl({
                                                                coverage: (v ??
                                                                    coverage) as ProfitLossCoverage,
                                                            })
                                                            setPagination(
                                                                (p) => ({
                                                                    ...p,
                                                                    pageIndex: 0,
                                                                }),
                                                            )
                                                        }}
                                                        options={(
                                                            Object.keys(
                                                                COVERAGE_FILTER_LABEL,
                                                            ) as ProfitLossCoverage[]
                                                        ).map((key) => ({
                                                            value: key,
                                                            label: COVERAGE_FILTER_LABEL[
                                                                key
                                                            ],
                                                        }))}
                                                        className="w-[10rem]"
                                                        size="sm"
                                                        allowClear={false}
                                                        aria-label="成本覆盖"
                                                        placeholder="成本覆盖"
                                                    />
                                                </>
                                            }
                                            secondary={
                                                customerId || salesOrderId ? (
                                                    <>
                                                        {customerId ? (
                                                            <FilterChip
                                                                label="客户锁定"
                                                                onClear={() => {
                                                                    patchUrl({
                                                                        customerId:
                                                                            null,
                                                                    })
                                                                    setPagination(
                                                                        (
                                                                            p,
                                                                        ) => ({
                                                                            ...p,
                                                                            pageIndex: 0,
                                                                        }),
                                                                    )
                                                                }}
                                                                clearLabel="移除客户锁定"
                                                            />
                                                        ) : null}
                                                        {salesOrderId ? (
                                                            <FilterChip
                                                                label="销售单锁定"
                                                                onClear={() => {
                                                                    patchUrl({
                                                                        salesOrderId:
                                                                            null,
                                                                    })
                                                                    setPagination(
                                                                        (
                                                                            p,
                                                                        ) => ({
                                                                            ...p,
                                                                            pageIndex: 0,
                                                                        }),
                                                                    )
                                                                }}
                                                                clearLabel="移除销售单锁定"
                                                            />
                                                        ) : null}
                                                    </>
                                                ) : undefined
                                            }
                                            actions={
                                                <div className="flex items-center gap-2 text-xs text-muted-foreground">
                                                    <span aria-live="polite">
                                                        共{" "}
                                                        {data.rows.total.toLocaleString(
                                                            "zh-CN",
                                                        )}{" "}
                                                        条
                                                    </span>
                                                    {hasFilters ? (
                                                        <Button
                                                            type="button"
                                                            size="xs"
                                                            variant="ghost"
                                                            onClick={
                                                                clearFilters
                                                            }
                                                        >
                                                            清除筛选
                                                        </Button>
                                                    ) : null}
                                                </div>
                                            }
                                        />
                                        <Tabs
                                            value={dimension}
                                            onValueChange={(v) => {
                                                patchUrl({ dimension: v })
                                                setPagination((p) => ({
                                                    ...p,
                                                    pageIndex: 0,
                                                }))
                                            }}
                                        >
                                            <TabsList>
                                                {(
                                                    Object.keys(
                                                        DIMENSION_LABEL,
                                                    ) as ProfitLossDimension[]
                                                ).map((key) => (
                                                    <TabsTrigger
                                                        key={key}
                                                        value={key}
                                                    >
                                                        {DIMENSION_LABEL[key]}
                                                    </TabsTrigger>
                                                ))}
                                            </TabsList>
                                        </Tabs>
                                    </div>
                                }
                                table={
                                    data.rows.total === 0 ? (
                                        <BusinessEmptyState
                                            kind="filter"
                                            title="当前筛选无非卡券经营结果"
                                            description={`范围：${data.filterSummary}。可调整期间、覆盖口径或清除搜索。`}
                                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                                            action={
                                                <Button
                                                    type="button"
                                                    variant="secondary"
                                                    className="rounded-lg shadow-none"
                                                    onClick={clearFilters}
                                                >
                                                    清除筛选
                                                </Button>
                                            }
                                        />
                                    ) : (
                                        <DataTable
                                            data={pageRows}
                                            columns={columns}
                                            getRowId={(row) => row.rowId}
                                            rowCount={data.rows.total}
                                            pagination={pagination}
                                            onPaginationChange={setPagination}
                                            sorting={tableSorting}
                                            onSortingChange={
                                                handleTableSortingChange
                                            }
                                            loading={
                                                viewQuery.isFetching &&
                                                !viewQuery.isPending
                                            }
                                            layout="flush"
                                            density="compact"
                                        />
                                    )
                                }
                            />
                        </>
                    ) : null}
                </>
            ) : null}

            <QuickPreviewSheet
                open={costDetailRow != null}
                onOpenChange={(open) => {
                    if (!open) {
                        setCostDetailRow(null)
                        setSelectedCostEntryId(null)
                    }
                }}
                size="detail"
                title="成本记录"
                description="只读 · 不含税；含税仅作税额展示。"
                identity={
                    costDetailRow ? (
                        <span>
                            销售单 {costDetailRow.identityLabel} ·{" "}
                            {costDetailRow.customerLabel}
                        </span>
                    ) : null
                }
                summary={
                    costDetailRow ? (
                        <BusinessStatusBadge
                            context="preview"
                            label={
                                COVERAGE_STATE_UI[costDetailRow.coverageState]
                                    .label
                            }
                            tone={
                                COVERAGE_STATE_UI[costDetailRow.coverageState]
                                    .tone
                            }
                            description={costDetailRow.coverageBlockers
                                .map((b) => b.message)
                                .join("；")}
                        />
                    ) : null
                }
                footer={
                    <div className="flex w-full flex-wrap items-center justify-between gap-2">
                        <p className="text-xs text-muted-foreground">
                            不可删除成本或直接改金额；更正请走原业务对象变更/冲减。
                        </p>
                        {costDetailRow?.objectId ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/sales/orders/${encodeURIComponent(costDetailRow.objectId)}`}
                                        target="_blank"
                                    />
                                }
                            >
                                {openWorkspaceLabel("W05")}
                                <ExternalLinkIcon className="ml-1 size-3.5" />
                            </Button>
                        ) : null}
                    </div>
                }
            >
                <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-auto p-4">
                    {costDetailRow &&
                    costDetailRow.coverageBlockers.length > 0 ? (
                        <Alert variant="warning">
                            <AlertTitle>成本缺口原因</AlertTitle>
                            <AlertDescription>
                                <ul className="list-disc pl-4">
                                    {costDetailRow.coverageBlockers.map((b) => (
                                        <li key={b.code}>{b.message}</li>
                                    ))}
                                </ul>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {costEntriesQuery.isPending ? (
                        <Skeleton className="h-40 w-full" />
                    ) : costEntriesQuery.isError ? (
                        <Alert variant="destructive">
                            <AlertTitle>成本记录加载失败</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(
                                    costEntriesQuery.error,
                                    "未能读取本条销售单的成本记录。请重试；不影响已展示金额。",
                                )}
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    className="ml-2"
                                    onClick={() =>
                                        void costEntriesQuery.refetch()
                                    }
                                >
                                    重试
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : costEntriesQuery.data &&
                      costEntriesQuery.data.length > 0 ? (
                        <>
                            <div className="flex flex-wrap gap-2">
                                {costEntriesQuery.data.map((entry) => (
                                    <Button
                                        key={entry.costEntryId}
                                        type="button"
                                        size="sm"
                                        variant={
                                            selectedCostEntryId ===
                                            entry.costEntryId
                                                ? "default"
                                                : "outline"
                                        }
                                        onClick={() =>
                                            setSelectedCostEntryId(
                                                entry.costEntryId,
                                            )
                                        }
                                    >
                                        {entry.costTypeLabel} ·{" "}
                                        {entry.stageLabel}
                                    </Button>
                                ))}
                            </div>
                            <Separator />
                            {selectedEntry ? (
                                <CostEntryDetailBody entry={selectedEntry} />
                            ) : null}
                        </>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            当前行无可查看的成本记录（无权限或完全未覆盖）。
                        </p>
                    )}
                </div>
            </QuickPreviewSheet>
        </PageScaffold>
    )
}
