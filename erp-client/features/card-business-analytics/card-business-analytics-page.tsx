"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type {
    ColumnDef,
    PaginationState,
    SortingState,
} from "@tanstack/react-table"
import {
    CalendarRangeIcon,
    DownloadIcon,
    ExternalLinkIcon,
    InfoIcon,
    RefreshCwIcon,
} from "lucide-react"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    BusinessTableFrame,
    CostCoverageNotice,
    DataFreshness,
    DataTable,
    MetricItem,
    MetricStrip,
    MoneyValue,
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
import {
    CustomerSearchCombobox,
    SalesOrderSearchCombobox,
} from "@/features/entity-selectors"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import type { DataFreshnessState } from "@/components/business/page"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { DatePicker } from "@/components/ui/date-picker"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import {
    useCardBusinessAnalyticsQuery,
    useDateBasisConfigQuery,
    useStartCardBusinessExportMutation,
} from "@/features/card-business-analytics/queries"
import type {
    CardBusinessAnalyticsQuery,
    CardBusinessAnalyticsView,
    CardBusinessDimension,
    CardBusinessExportJob,
    CardBusinessRow,
    CostBasisCode,
    CoverageFilter,
    DateBasis,
    ExpiryState,
    PeriodPreset,
    ProjectionFreshnessState,
    TaxBasis,
} from "@/features/card-business-analytics/types"
import {
    COVERAGE_FILTER_LABEL,
    COVERAGE_STATUS_UI,
    COST_BASIS_LABEL,
    COST_BASIS_ROW_UI,
    DATE_BASIS_LABEL,
    DIMENSION_LABEL,
} from "@/features/card-business-analytics/types"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { CardBusinessCharts } from "./components/card-business-charts"
import { formatMoneyDisplay } from "./presentation"

function taxBadge(basis: TaxBasis): string {
    return basis === "GROSS" ? "含税" : "不含税"
}

/** 导出成功条的真实下载入口：按当前视图行生成 CSV（含口径/筛选/时间水印）。 */
function downloadCardBusinessCsv(
    data: CardBusinessAnalyticsView,
    job: CardBusinessExportJob,
) {
    const wm = job.watermark
    const quote = (v: string) => `"${v.replaceAll('"', '""')}"`
    const metaLines = [
        "# 业务口径=卡券经营（销售/面值/消费/余额为含税；成本/毛差/经营贡献为不含税）",
        `# 期间=${wm.periodFrom} ~ ${wm.periodTo}`,
        `# 日期口径=${DATE_BASIS_LABEL[wm.dateBasis]}`,
        `# 筛选=${wm.filterSummary}`,
        `# 覆盖率=${wm.coverageRate ?? "—"}`,
        `# 数据更新时间=${wm.projectionUpdatedAt}`,
        `# 同步时间=${wm.consumedOutboxWatermark}`,
        `# 余额快照时间=${wm.balanceSnapshotAt ?? "—"}`,
        `# 延迟=${wm.lagSeconds} 秒`,
        `# 行数=${wm.rowCount}`,
        `# 微信排除=${wm.wechatExcludedNote}`,
    ]
    const header =
        "客户,销售单,卡券类目,卡实例引用,消费(含税),退款(含税),成本口径,成本(不含税),覆盖,未履约余额(含税)"
    const body = data.rows.items.map((r) =>
        [
            r.customerLabel,
            r.salesOrderNo ?? "",
            r.voucherCategoryLabel,
            r.cardInstanceRef ?? "",
            r.consumptionGross,
            r.refundGross,
            COST_BASIS_LABEL[r.costBasis],
            r.costNet ?? "",
            r.coverageStatus === "covered"
                ? "已覆盖"
                : r.coverageStatus === "partial"
                  ? "部分"
                  : "未覆盖",
            r.unfulfilledBalanceGross,
        ]
            .map((c) => quote(String(c)))
            .join(","),
    )
    const csv = [...metaLines, header, ...body].join("\n")
    const url = URL.createObjectURL(
        new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" }),
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download =
        wm.periodFrom && wm.periodTo
            ? `卡券经营分析_${wm.periodFrom}_${wm.periodTo}.csv`
            : "卡券经营分析.csv"
    anchor.click()
    URL.revokeObjectURL(url)
}

function parseDateBasis(raw: string | null): DateBasis | "" {
    if (raw === "consumption" || raw === "sales" || raw === "expiry") return raw
    return ""
}

function parseDimension(raw: string | null): CardBusinessDimension {
    if (
        raw === "customer" ||
        raw === "sales_order" ||
        raw === "voucher_category" ||
        raw === "card_instance"
    ) {
        return raw
    }
    return "customer"
}

function parsePreset(raw: string | null): PeriodPreset {
    if (
        raw === "last-month" ||
        raw === "quarter-to-date" ||
        raw === "month-to-date"
    ) {
        return raw
    }
    return "month-to-date"
}

function parseExpiry(raw: string | null): ExpiryState {
    if (raw === "active" || raw === "expired" || raw === "all") return raw
    return "all"
}

function parseCoverage(raw: string | null): CoverageFilter {
    if (raw === "below_threshold" || raw === "none" || raw === "all") return raw
    return "all"
}

function parseCostBasis(raw: string | null): CostBasisCode[] | undefined {
    if (!raw) return undefined
    const parts = raw
        .split(",")
        .map((s) => s.trim())
        .filter(
            (s): s is CostBasisCode =>
                s === "ACTUAL" || s === "STANDARD" || s === "NONE",
        )
    return parts.length > 0 ? parts : undefined
}

/** periodPreset → 相对当前日期计算 from/to。 */
function resolvePeriod(preset: PeriodPreset): { from: string; to: string } {
    const today = new Date()
    const iso = (d: Date): string => {
        const y = d.getFullYear()
        const m = `${d.getMonth() + 1}`.padStart(2, "0")
        const day = `${d.getDate()}`.padStart(2, "0")
        return `${y}-${m}-${day}`
    }
    if (preset === "last-month") {
        const last = new Date(today.getFullYear(), today.getMonth(), 0)
        const first = new Date(last.getFullYear(), last.getMonth(), 1)
        return { from: iso(first), to: iso(last) }
    }
    if (preset === "quarter-to-date") {
        const qStart = new Date(
            today.getFullYear(),
            Math.floor(today.getMonth() / 3) * 3,
            1,
        )
        return { from: iso(qStart), to: iso(today) }
    }
    return { from: iso(today), to: iso(today) }
}

function mapFreshnessUi(
    state: ProjectionFreshnessState,
    options?: {
        refreshFailed?: boolean
        refreshing?: boolean
        breached?: boolean
    },
): { uiState: DataFreshnessState; statusLabel: string } {
    if (options?.refreshing) {
        return { uiState: "syncing", statusLabel: "正在刷新数据" }
    }
    if (options?.refreshFailed) {
        return { uiState: "failed", statusLabel: "刷新失败 · 保留旧数据" }
    }
    if (options?.breached || state === "stale") {
        return {
            uiState: "stale",
            statusLabel: "SLA 超时 · 数据陈旧 · 非实时",
        }
    }
    switch (state) {
        case "rebuilding":
            return { uiState: "syncing", statusLabel: "数据更新中" }
        case "failed":
            return { uiState: "failed", statusLabel: "数据更新失败" }
        default:
            return { uiState: "fresh", statusLabel: "数据已更新" }
    }
}

function metricValue(
    value: string | null,
    taxBasis: TaxBasis,
    valueState: string,
    reasonCode?: string,
): React.ReactNode {
    // 指标 label 已附加（含税）/（不含税），MoneyValue 不再重复渲染口径徽章
    if (valueState === "masked") {
        return <MoneyValue value={null} unavailableReason="无字段权限" />
    }
    if (valueState === "unavailable" || value == null) {
        return (
            <MoneyValue
                value={null}
                unavailableReason={reasonCode ?? "不可计算"}
            />
        )
    }
    // 比率类直接展示（label 已带口径说明）
    if (value.includes("%")) {
        return <span className="num">{value}</span>
    }
    return <MoneyValue value={value} />
}

export function CardBusinessAnalyticsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const accountProfile = useAccountProfileQuery()
    const canReadAllCustomers = hasPermission(
        accountProfile.data?.permissions,
        "customer_scope:detail",
    )

    const periodPreset = parsePreset(searchParams.get("periodPreset"))
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
    const [exportJob, setExportJob] =
        React.useState<CardBusinessExportJob | null>(null)
    const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)
    const [basisSheetOpen, setBasisSheetOpen] = React.useState(false)
    const [refreshFailed, setRefreshFailed] = React.useState<string | null>(
        null,
    )
    const [refreshing, setRefreshing] = React.useState(false)

    const basisQuery = useDateBasisConfigQuery()
    const basisConfig = basisQuery.data

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
        basisQuery.isSuccess &&
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

    const viewQuery = useCardBusinessAnalyticsQuery(
        analysisQuery,
        analysisReady,
    )
    const exportMutation = useStartCardBusinessExportMutation()

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

    async function handleRefresh() {
        setRefreshing(true)
        setRefreshFailed(null)
        try {
            await viewQuery.refetch()
        } catch (error) {
            setRefreshFailed(
                getErrorMessage(error, "刷新失败，已保留上次成功数据。"),
            )
        } finally {
            setRefreshing(false)
        }
    }

    async function handleExportConfirm() {
        if (!data || !analysisQuery) return
        setExportPreviewOpen(false)
        const job = await exportMutation.mutateAsync({
            query: analysisQuery,
            view: {
                period: data.period,
                scope: data.scope,
                freshness: data.freshness,
                coverage: data.coverage,
                filterSummary: data.filterSummary,
                wechatExcludedNote: data.wechatExcludedNote,
                fieldPermissions: data.fieldPermissions,
                rows: data.rows,
            },
        })
        setExportJob(job)
    }

    const data = viewQuery.data

    const columns = React.useMemo<ColumnDef<CardBusinessRow>[]>(
        () => [
            {
                id: "customer",
                accessorFn: (r) => r.customerLabel,
                header: "客户",
                meta: { label: "客户" },
                cell: ({ row }) =>
                    row.original.customerId ? (
                        <Link
                            href={`/sales/customers/${row.original.customerId}`}
                            className="text-sm underline-offset-2 hover:underline"
                        >
                            {row.original.customerLabel}
                        </Link>
                    ) : (
                        <span className="text-sm">
                            {row.original.customerLabel}
                        </span>
                    ),
            },
            {
                id: "salesOrder",
                accessorFn: (r) => r.salesOrderNo ?? "",
                header: "销售单",
                meta: { label: "销售单" },
                cell: ({ row }) =>
                    row.original.salesOrderId ? (
                        <Link
                            href={`/sales/orders/${row.original.salesOrderId}`}
                            className="text-sm underline-offset-2 hover:underline"
                        >
                            {row.original.salesOrderNo}
                        </Link>
                    ) : (
                        <span className="text-sm">
                            {row.original.salesOrderNo ?? "—"}
                        </span>
                    ),
            },
            {
                id: "category",
                accessorFn: (r) => r.voucherCategoryLabel,
                header: "卡券类目",
                meta: { label: "卡券类目" },
                cell: ({ row }) => (
                    <span className="text-sm">
                        {row.original.voucherCategoryLabel}
                    </span>
                ),
            },
            {
                id: "cardRef",
                accessorFn: (r) => r.cardInstanceRef ?? "",
                header: "卡实例引用",
                meta: { label: "稳定卡实例引用摘要" },
                cell: ({ row }) =>
                    row.original.cardInstanceRef ? (
                        <span
                            className="num text-sm"
                            title="不可逆稳定引用，不可反推卡号/卡密"
                        >
                            {row.original.cardInstanceRef}
                        </span>
                    ) : (
                        <span className="text-sm text-muted-foreground">—</span>
                    ),
            },
            {
                id: "consumption",
                accessorFn: (r) => r.consumptionGross,
                header: "消费(含税)",
                meta: {
                    label: "消费金额含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.consumptionGross} />
                ),
            },
            {
                id: "refund",
                accessorFn: (r) => r.refundGross,
                header: "退款(含税)",
                meta: {
                    label: "退款含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.refundGross} />
                ),
            },
            {
                id: "costBasis",
                accessorFn: (r) => r.costBasis,
                header: "成本口径",
                meta: { label: "成本口径" },
                cell: ({ row }) => {
                    const ui = COST_BASIS_ROW_UI[row.original.costBasis]
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={ui.label}
                            tone={ui.tone}
                        />
                    )
                },
            },
            {
                id: "cost",
                accessorFn: (r) => r.costNet ?? "",
                header: "成本(不含税)",
                meta: {
                    label: "成本不含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => {
                    if (row.original.costBasis === "NONE") {
                        return (
                            <MoneyValue
                                value={null}
                                unavailableReason="无可用成本 · 不显示金额"
                            />
                        )
                    }
                    return <MoneyValue value={row.original.costNet} />
                },
            },
            {
                id: "coverage",
                accessorFn: (r) => r.coverageStatus,
                header: "覆盖",
                meta: { label: "覆盖状态" },
                cell: ({ row }) => {
                    const s = row.original.coverageStatus
                    return (
                        <BusinessStatusBadge
                            context="list"
                            label={
                                s === "covered"
                                    ? "已覆盖"
                                    : s === "partial"
                                      ? "部分"
                                      : "未覆盖"
                            }
                            tone={
                                s === "covered"
                                    ? "success"
                                    : s === "partial"
                                      ? "warning"
                                      : "destructive"
                            }
                        />
                    )
                },
            },
            {
                id: "balance",
                accessorFn: (r) => r.unfulfilledBalanceGross,
                header: "未履约余额(含税)",
                meta: {
                    label: "未履约余额含税",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue value={row.original.unfulfilledBalanceGross} />
                ),
            },
            {
                id: "actions",
                header: "下钻",
                meta: { label: "下钻" },
                cell: ({ row }) => (
                    <div className="flex flex-wrap gap-1">
                        {row.original.consumptionOrderHref ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                render={
                                    <Link
                                        href={row.original.consumptionOrderHref}
                                    />
                                }
                            >
                                {openWorkspaceLabel("W25")}
                                <ExternalLinkIcon
                                    className="size-3"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                        {row.original.supplierOrderHref ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                render={
                                    <Link
                                        href={row.original.supplierOrderHref}
                                    />
                                }
                            >
                                {openWorkspaceLabel("W26")}
                                <ExternalLinkIcon
                                    className="size-3"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                        {row.original.costBasis === "NONE" && data ? (
                            <Button
                                type="button"
                                size="xs"
                                variant="ghost"
                                render={
                                    <Link
                                        href={
                                            data.governanceLinks
                                                .noneCoverageHref
                                        }
                                    />
                                }
                            >
                                {openWorkspaceLabel("W29")}
                                <ExternalLinkIcon
                                    className="size-3"
                                    aria-hidden
                                />
                            </Button>
                        ) : null}
                    </div>
                ),
            },
        ],
        [data],
    )

    // —— Loading shells ——
    if (basisQuery.isPending) {
        return (
            <PageScaffold>
                <Skeleton className="h-10 w-64 rounded-lg" />
                <Skeleton className="h-24 w-full rounded-lg" />
                <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                    {Array.from({ length: 8 }).map((_, i) => (
                        <Skeleton key={i} className="h-20 rounded-lg" />
                    ))}
                </div>
                <Skeleton className="h-64 w-full rounded-lg" />
            </PageScaffold>
        )
    }

    if (basisQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    error={basisQuery.error}
                    title="日期口径配置加载失败"
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

    // —— Q2 期间与日期口径选择态 ——
    if (analysisBlocked) {
        return (
            <PageScaffold>
                <PageHeader
                    title="卡券消费台账与经营分析"
                    description="系统尚未配置默认日期口径。请显式选择期间与日期口径后再开始分析。"
                    breadcrumbs={[
                        {
                            id: "an",
                            label: "分析",
                            href: "/analytics/card-business",
                        },
                        { id: "cb", label: "卡券经营分析", current: true },
                    ]}
                />
                <Alert variant="warning">
                    <CalendarRangeIcon aria-hidden="true" />
                    <AlertTitle>请选择期间与日期口径</AlertTitle>
                    <AlertDescription>
                        选择完整的期间与日期口径后才会发起查询；该选择将作用于全部指标与图表。
                    </AlertDescription>
                </Alert>
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-border/30">
                        <CardTitle>显式期间与日期口径</CardTitle>
                        <CardDescription>
                            选择完整的期间与日期口径后才会发起查询；该选择将作用于全部指标与图表。
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="flex flex-col gap-4 pt-4">
                        <div className="grid gap-3 sm:grid-cols-3">
                            <div className="space-y-1.5">
                                <Label htmlFor="w28-from">开始日期</Label>
                                <DatePicker
                                    value={explicitFrom || undefined}
                                    onValueChange={(next) =>
                                        setExplicitFrom(next ?? "")
                                    }
                                />
                            </div>
                            <div className="space-y-1.5">
                                <Label htmlFor="w28-to">结束日期</Label>
                                <DatePicker
                                    value={explicitTo || undefined}
                                    onValueChange={(next) =>
                                        setExplicitTo(next ?? "")
                                    }
                                />
                            </div>
                            <div className="space-y-1.5">
                                <Label htmlFor="w28-dateBasis">日期口径</Label>
                                <OptionCombobox
                                    id="w28-dateBasis"
                                    value={explicitDateBasis}
                                    onValueChange={(v) =>
                                        setExplicitDateBasis(
                                            (v ??
                                                explicitDateBasis) as DateBasis,
                                        )
                                    }
                                    options={(
                                        basisConfig?.allowedDateBases ?? []
                                    ).map((b) => ({
                                        value: b.code,
                                        label: b.label,
                                    }))}
                                    className="w-full"
                                    size="sm"
                                    allowClear={false}
                                    aria-label="日期口径"
                                    placeholder="日期口径"
                                />
                            </div>
                        </div>
                        <div className="flex flex-col gap-2">
                            <Button
                                type="button"
                                disabled={
                                    !explicitFrom ||
                                    !explicitTo ||
                                    !explicitDateBasis ||
                                    (explicitFrom > explicitTo &&
                                        explicitFrom !== explicitTo)
                                }
                                onClick={applyExplicitPeriod}
                            >
                                开始分析
                            </Button>
                            {explicitFrom &&
                            explicitTo &&
                            explicitFrom > explicitTo ? (
                                <p className="text-xs text-destructive">
                                    开始日期不能晚于结束日期，请调整后提交。
                                </p>
                            ) : null}
                        </div>
                    </CardContent>
                </Card>
            </PageScaffold>
        )
    }

    const freshnessUi = data
        ? mapFreshnessUi(data.freshness.state, {
              refreshFailed: Boolean(refreshFailed),
              refreshing,
              breached: data.freshness.slaState === "BREACHED",
          })
        : { uiState: "unknown" as const, statusLabel: "—" }

    return (
        <PageScaffold>
            <PageHeader
                title="卡券消费台账与经营分析"
                breadcrumbs={[
                    {
                        id: "an",
                        label: "分析",
                        href: "/analytics/card-business",
                    },
                    { id: "cb", label: "卡券经营分析", current: true },
                ]}
                metadata={
                    data ? (
                        <div className="flex flex-col gap-1">
                            <DataFreshness
                                updatedAt={formatDateTime(
                                    data.freshness.projectionUpdatedAt,
                                    "full",
                                )}
                                dateTime={data.freshness.projectionUpdatedAt}
                                state={freshnessUi.uiState}
                                statusLabel={freshnessUi.statusLabel}
                                label="分析汇总"
                            />
                            <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
                                <span>
                                    同步{" "}
                                    <time
                                        className="num"
                                        dateTime={
                                            data.freshness
                                                .consumedOutboxWatermark
                                        }
                                    >
                                        {formatDateTime(
                                            data.freshness
                                                .consumedOutboxWatermark,
                                            "full",
                                        )}
                                    </time>
                                </span>
                                <span aria-hidden>·</span>
                                <span>
                                    业务记录{" "}
                                    <time
                                        className="num"
                                        dateTime={
                                            data.freshness.sourceFactWatermark
                                        }
                                    >
                                        {formatDateTime(
                                            data.freshness.sourceFactWatermark,
                                            "full",
                                        )}
                                    </time>
                                </span>
                                {data.freshness.balanceSnapshotAt ? (
                                    <>
                                        <span aria-hidden>·</span>
                                        <span>
                                            余额快照{" "}
                                            <time
                                                className="num"
                                                dateTime={
                                                    data.freshness
                                                        .balanceSnapshotAt
                                                }
                                            >
                                                {formatDateTime(
                                                    data.freshness
                                                        .balanceSnapshotAt,
                                                    "full",
                                                )}
                                            </time>
                                            <span className="ml-1">
                                                （独立）
                                            </span>
                                        </span>
                                    </>
                                ) : null}
                                <span aria-hidden>·</span>
                                <span
                                    className={
                                        data.freshness.lagSeconds >
                                        data.freshness.maxLagSeconds
                                            ? "font-medium text-destructive"
                                            : "num"
                                    }
                                >
                                    延迟 {data.freshness.lagSeconds} 秒（上限{" "}
                                    {data.freshness.maxLagSeconds} 秒）
                                </span>
                            </div>
                        </div>
                    ) : null
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "basis",
                                label: "口径说明",
                                icon: InfoIcon,
                                variant: "outline",
                                onClick: () => setBasisSheetOpen(true),
                            },
                            {
                                actionKey: "refresh",
                                label: refreshing ? "刷新中" : "刷新",
                                icon: RefreshCwIcon,
                                variant: "ghost",
                                className:
                                    "text-muted-foreground hover:text-foreground",
                                disabled: !data || refreshing,
                                onClick: () => {
                                    void handleRefresh()
                                },
                            },
                            {
                                actionKey: "export",
                                label: "导出",
                                icon: DownloadIcon,
                                disabled: !data?.fieldPermissions.canExport,
                                onClick: () => setExportPreviewOpen(true),
                            },
                        ]}
                    />
                }
            />

            {/* Filter bar */}
            <Card size="sm" className={surfacePanelClassName}>
                <CardContent className="flex flex-col gap-3 pt-4 sm:flex-row sm:flex-wrap sm:items-end">
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-preset">期间快捷</Label>
                        <OptionCombobox
                            id="w28-preset"
                            value={searchParams.get("periodPreset") ?? ""}
                            onValueChange={(v) => {
                                if (!v) return
                                applyPreset(v as PeriodPreset)
                            }}
                            options={[
                                { value: "", label: "自定义" },
                                { value: "month-to-date", label: "本月至今" },
                                { value: "last-month", label: "上月" },
                                { value: "quarter-to-date", label: "本季至今" },
                            ]}
                            className="w-[10rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="期间快捷"
                            placeholder="期间快捷"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-filter-from">从</Label>
                        <DatePicker
                            className="w-[10.5rem]"
                            value={from || undefined}
                            onValueChange={(next) =>
                                patchUrl({
                                    from: next ?? "",
                                    periodPreset: null,
                                })
                            }
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-filter-to">至</Label>
                        <DatePicker
                            className="w-[10.5rem]"
                            value={to || undefined}
                            onValueChange={(next) =>
                                patchUrl({ to: next ?? "", periodPreset: null })
                            }
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-filter-basis">日期口径</Label>
                        <OptionCombobox
                            id="w28-filter-basis"
                            value={dateBasisUrl}
                            onValueChange={(v) =>
                                patchUrl({ dateBasis: v ?? dateBasisUrl })
                            }
                            options={(basisConfig?.allowedDateBases ?? []).map(
                                (b) => ({
                                    value: b.code,
                                    label: b.label,
                                }),
                            )}
                            className="w-[12rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="日期口径"
                            placeholder="日期口径"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-customer">客户</Label>
                        <CustomerSearchCombobox
                            value={customerId}
                            onValueChange={(id) =>
                                patchUrl({ customerId: id || null })
                            }
                            purpose="filter"
                            scope={
                                canReadAllCustomers
                                    ? "all_authorized"
                                    : "assigned"
                            }
                            className="min-w-48"
                            placeholder="全部客户"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-so">销售单</Label>
                        <SalesOrderSearchCombobox
                            value={salesOrderId}
                            onValueChange={(id) =>
                                patchUrl({ salesOrderId: id || null })
                            }
                            purpose="filter"
                            className="min-w-48"
                            placeholder="全部销售单"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-costBasis">成本口径</Label>
                        <OptionCombobox
                            id="w28-costBasis"
                            value={costBasis?.join(",") ?? ""}
                            onValueChange={(v) =>
                                patchUrl({ costBasis: v || null })
                            }
                            options={[
                                { value: "", label: "全部" },
                                { value: "ACTUAL", label: "实际成本" },
                                { value: "STANDARD", label: "标准成本" },
                                { value: "NONE", label: "无可用成本" },
                                {
                                    value: "ACTUAL,STANDARD",
                                    label: "实际 + 标准成本",
                                },
                            ]}
                            className="w-[11rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="成本口径"
                            placeholder="成本口径"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-expiry">履约期限</Label>
                        <OptionCombobox
                            id="w28-expiry"
                            value={expiryState}
                            onValueChange={(v) =>
                                patchUrl({ expiryState: (v ?? "all") || null })
                            }
                            options={[
                                { value: "all", label: "全部" },
                                { value: "active", label: "未到期" },
                                { value: "expired", label: "已到期" },
                            ]}
                            className="w-[8rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="履约期限"
                            placeholder="履约期限"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-coverage">覆盖口径</Label>
                        <OptionCombobox
                            id="w28-coverage"
                            value={coverage}
                            onValueChange={(v) =>
                                patchUrl({
                                    coverage: v && v !== "all" ? v : null,
                                })
                            }
                            options={[
                                { value: "all", label: "全部覆盖状态" },
                                {
                                    value: "below_threshold",
                                    label: COVERAGE_FILTER_LABEL.below_threshold,
                                },
                                {
                                    value: "none",
                                    label: COVERAGE_FILTER_LABEL.none,
                                },
                            ]}
                            className="w-[10rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="覆盖口径"
                            placeholder="全部覆盖状态"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="w28-dimension">分析视角</Label>
                        <OptionCombobox
                            id="w28-dimension"
                            value={dimension}
                            onValueChange={(v) =>
                                patchUrl({ dimension: v ?? dimension })
                            }
                            options={(
                                Object.keys(
                                    DIMENSION_LABEL,
                                ) as CardBusinessDimension[]
                            ).map((k) => ({
                                value: k,
                                label: DIMENSION_LABEL[k],
                            }))}
                            className="w-[10rem]"
                            size="sm"
                            allowClear={false}
                            aria-label="分析视角"
                            placeholder="分析视角"
                        />
                    </div>
                    {(customerId ||
                        salesOrderId ||
                        (costBasis && costBasis.length > 0) ||
                        expiryState !== "all" ||
                        coverage !== "all") && (
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            onClick={() =>
                                patchUrl({
                                    customerId: null,
                                    salesOrderId: null,
                                    costBasis: null,
                                    expiryState: null,
                                    coverage: null,
                                })
                            }
                        >
                            清除筛选
                        </Button>
                    )}
                </CardContent>
            </Card>

            {viewQuery.isPending && !data ? (
                <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
                    {Array.from({ length: 8 }).map((_, i) => (
                        <Skeleton key={i} className="h-20 rounded-lg" />
                    ))}
                </div>
            ) : null}

            {viewQuery.isError && !data ? (
                <BusinessFailureState
                    error={viewQuery.error}
                    title="卡券经营数据加载失败"
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
                    {data.freshness.slaState === "BREACHED" ||
                    data.freshness.state === "stale" ||
                    refreshFailed ? (
                        <Alert variant="warning">
                            <AlertTitle>
                                {refreshFailed
                                    ? "刷新失败"
                                    : "数据陈旧 · 更新超时 · 非实时"}
                            </AlertTitle>
                            <AlertDescription>
                                {refreshFailed
                                    ? refreshFailed
                                    : `更新延迟 ${data.freshness.lagSeconds}s 超过固定上限 ${data.freshness.maxLagSeconds}s（${
                                          data.freshness.slaState === "BREACHED"
                                              ? "已超时"
                                              : data.freshness.slaState ===
                                                  "REBUILDING"
                                                ? "重建中"
                                                : "异常"
                                      }）。数据 ${formatDateTime(data.freshness.projectionUpdatedAt, "full")}，同步 ${formatDateTime(data.freshness.consumedOutboxWatermark, "full")}。余额记录独立显示，不合并为「实时」。`}
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {data.freshness.state === "rebuilding" ? (
                        <Alert>
                            <AlertTitle>数据更新中</AlertTitle>
                            <AlertDescription>
                                保留最近成功结果只读查看；导出将标注旧数据时间。更新只修复查询数据，不修改业务记录。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {data.freshness.state === "failed" ? (
                        <Alert variant="destructive">
                            <AlertTitle>数据更新失败</AlertTitle>
                            <AlertDescription>
                                经营记录未被修改；展示上次成功数据。可前往接口错误中心查看数据异常。
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="ml-2"
                                    render={
                                        <Link
                                            href={
                                                data.governanceLinks
                                                    .integrationErrorsHref
                                            }
                                        />
                                    }
                                >
                                    {openWorkspaceLabel("W29")}
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {viewQuery.isError ? (
                        <Alert variant="destructive">
                            <AlertTitle>数据更新失败</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(
                                    viewQuery.error,
                                    "已保留上次成功结果供只读查阅，未覆盖任何金额。请重试或调整筛选。",
                                )}
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
                            label="卡券经营分析导出"
                            description={
                                <>
                                    口径/筛选：
                                    {exportJob.watermark.filterSummary}
                                    <span className="mt-1 block">
                                        覆盖率{" "}
                                        {exportJob.watermark.coverageRate ??
                                            "—"}{" "}
                                        · 数据{" "}
                                        {formatDateTime(
                                            exportJob.watermark
                                                .projectionUpdatedAt,
                                            "full",
                                        )}{" "}
                                        · 同步{" "}
                                        {formatDateTime(
                                            exportJob.watermark
                                                .consumedOutboxWatermark,
                                            "full",
                                        )}{" "}
                                        · 延迟 {exportJob.watermark.lagSeconds}s
                                    </span>
                                    <span className="mt-1 block text-xs">
                                        {exportJob.watermark.taxDisclaimer}
                                    </span>
                                    {exportJob.downloadLabel ? (
                                        <span className="mt-1 block font-medium">
                                            导出文件：{exportJob.downloadLabel}
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                className="ml-2"
                                                onClick={() =>
                                                    downloadCardBusinessCsv(
                                                        data,
                                                        exportJob,
                                                    )
                                                }
                                            >
                                                下载 CSV
                                            </Button>
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

                    {/* Shared filter summary */}
                    <Alert>
                        <AlertTitle>当前筛选与数据时间</AlertTitle>
                        <AlertDescription className="text-xs leading-relaxed">
                            {data.filterSummary}
                            <span className="mt-1 block">
                                指标条与图表为全量口径（不随客户/销售单/成本口径/履约/覆盖筛选变化）；下钻明细表已按筛选过滤并按分析视角聚合。
                            </span>
                            <span className="mt-1 block text-muted-foreground">
                                {data.wechatExcludedNote}
                            </span>
                        </AlertDescription>
                    </Alert>

                    {/* CostCoverageNotice — 强制在利润指标前 */}
                    <CostCoverageNotice
                        basis={data.coverage.dominantBasis}
                        coveragePercent={data.coverage.ratePercent}
                        coverageLabel={data.coverage.rate ?? "—"}
                        coverageState={
                            COVERAGE_STATUS_UI[data.coverage.status].noticeState
                        }
                        breakdown={{
                            ACTUAL: (
                                <span>
                                    消费{" "}
                                    {formatMoneyDisplay(
                                        data.coverage.byBasis.find(
                                            (b) => b.basis === "ACTUAL",
                                        )?.consumptionGross,
                                    )}{" "}
                                    ·{" "}
                                    {data.coverage.byBasis.find(
                                        (b) => b.basis === "ACTUAL",
                                    )?.shareLabel ?? "—"}
                                    {data.fieldPermissions.canViewCost ? (
                                        <>
                                            {" "}
                                            · 成本{" "}
                                            {formatMoneyDisplay(
                                                data.coverage.byBasis.find(
                                                    (b) => b.basis === "ACTUAL",
                                                )?.costNet,
                                            )}
                                        </>
                                    ) : null}
                                </span>
                            ),
                            STANDARD: (
                                <span>
                                    消费{" "}
                                    {formatMoneyDisplay(
                                        data.coverage.byBasis.find(
                                            (b) => b.basis === "STANDARD",
                                        )?.consumptionGross,
                                    )}{" "}
                                    ·{" "}
                                    {data.coverage.byBasis.find(
                                        (b) => b.basis === "STANDARD",
                                    )?.shareLabel ?? "—"}
                                    <span className="block text-xs text-muted-foreground">
                                        按历史有效供给价估算，非实际
                                    </span>
                                </span>
                            ),
                            NONE: (
                                <span>
                                    消费{" "}
                                    {formatMoneyDisplay(
                                        data.coverage.byBasis.find(
                                            (b) => b.basis === "NONE",
                                        )?.consumptionGross,
                                    )}{" "}
                                    ·{" "}
                                    {data.coverage.byBasis.find(
                                        (b) => b.basis === "NONE",
                                    )?.shareLabel ?? "—"}
                                    <span className="block text-xs text-muted-foreground">
                                        无可用成本，不显示金额，不计入利润
                                    </span>
                                </span>
                            ),
                        }}
                        profitBasis="不含税；当前经营贡献不等于最终利润，须结合未履约余额查看。"
                        notice={
                            <>
                                {data.coverage.notice}
                                {data.coverage.profitReferenceOnly ? (
                                    <span className="mt-1 block font-medium">
                                        成本不完整，结果仅供参考。
                                    </span>
                                ) : null}
                                <span className="mt-2 flex flex-wrap gap-2">
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        render={
                                            <Link
                                                href={
                                                    data.governanceLinks
                                                        .noneCoverageHref
                                                }
                                            />
                                        }
                                    >
                                        查看未归集（接口错误中心）
                                    </Button>
                                    <Button
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        render={
                                            <Link
                                                href={
                                                    data.governanceLinks
                                                        .backfillHref
                                                }
                                            />
                                        }
                                    >
                                        历史消费回填
                                    </Button>
                                </span>
                            </>
                        }
                    />

                    {/* Metric strip — 含税/不含税逐项标注 */}
                    <MetricStrip
                        columns={4}
                        aria-label="卡券经营核心指标（销售消费余额含税 · 成本贡献不含税）"
                    >
                        {data.metrics.map((m) => (
                            <MetricItem
                                key={m.key}
                                label={`${m.label}（${taxBadge(m.taxBasis)}）`}
                                value={metricValue(
                                    m.value,
                                    m.taxBasis,
                                    m.valueState,
                                    m.reasonCode,
                                )}
                                detail={m.detail}
                                status={
                                    m.key === "currentContributionNet" &&
                                    data.coverage.profitReferenceOnly
                                        ? { label: "仅供参考", tone: "warning" }
                                        : m.key === "consumptionMarginNet" &&
                                            data.coverage.profitReferenceOnly
                                          ? {
                                                label: "仅供参考",
                                                tone: "warning",
                                            }
                                          : undefined
                                }
                            />
                        ))}
                    </MetricStrip>

                    {!data.scopeFullyExpired ? (
                        <Alert>
                            <AlertTitle>最终利润未展示</AlertTitle>
                            <AlertDescription className="flex flex-wrap items-center gap-2">
                                <span>
                                    {data.finalProfitUnavailableReason}
                                    当前同屏展示「当前经营贡献」与「未履约余额」。若需最终盈亏视角，可将日期口径切换为履约到期日并筛选已到期范围。
                                </span>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="outline"
                                    onClick={() =>
                                        patchUrl({
                                            dateBasis: "expiry",
                                            expiryState: "expired",
                                        })
                                    }
                                >
                                    切换为履约到期日 + 已到期
                                </Button>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    <CardBusinessCharts data={data} />
                    {/* Drill-down table */}
                    {data.rows.total === 0 ? (
                        <BusinessEmptyState
                            kind="filter"
                            title="当前筛选无卡券经营记录"
                            description="请调整期间、客户、销售单、成本口径或覆盖筛选。"
                            className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                            action={
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() =>
                                        patchUrl({
                                            customerId: null,
                                            salesOrderId: null,
                                            costBasis: null,
                                            expiryState: null,
                                            coverage: null,
                                        })
                                    }
                                >
                                    清除筛选
                                </Button>
                            }
                        />
                    ) : (
                        <BusinessTableFrame
                            title="下钻明细"
                            description={`客户 / 销售单 / 卡券类目 / 卡实例引用 / 消费(含税) / 退款(含税) / 成本口径 / 成本(不含税) / 覆盖 / 未履约余额(含税) / 下钻。不包含卡号、卡密与绑定手机号。共 ${data.rows.total} 行 · ${data.filterSummary}`}
                            table={
                                <DataTable
                                    columns={columns}
                                    data={[...data.rows.items]}
                                    getRowId={(row) => row.rowId}
                                    rowCount={data.rows.total}
                                    pagination={pagination}
                                    onPaginationChange={handlePaginationChange}
                                    sorting={tableSorting}
                                    onSortingChange={handleTableSortingChange}
                                    manualPagination
                                    layout="flush"
                                    density="compact"
                                />
                            }
                        />
                    )}
                </>
            ) : null}

            {/* 口径说明 Sheet */}
            <QuickPreviewSheet
                open={basisSheetOpen}
                onOpenChange={setBasisSheetOpen}
                title="卡券经营口径说明"
                description="税额、成本覆盖与完成条件"
            >
                <div className="space-y-4 text-sm">
                    <DescriptionList columns="one">
                        <DescriptionItem>
                            <DescriptionTerm>含税指标</DescriptionTerm>
                            <DescriptionDetails>
                                卡券销售金额、可消费总额度、累计消费、未消费余额、未履约余额均为含税。
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>不含税指标</DescriptionTerm>
                            <DescriptionDetails>
                                实际消费成本、消费毛差、当前经营贡献、最终经营盈亏均为不含税。进项税率不被销项税率替代。
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>成本三分法</DescriptionTerm>
                            <DescriptionDetails>
                                实际成本计入利润；标准成本按消费时点的有效供给价估算；无成本仅计入消费额与覆盖率，不显示为零成本，也不计入利润。
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>
                                当前贡献 vs 最终利润
                            </DescriptionTerm>
                            <DescriptionDetails>
                                当前经营贡献不是最终利润；须同屏展示未履约余额。履约期限未到期范围不展示最终利润。
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>微信支付边界</DescriptionTerm>
                            <DescriptionDetails>
                                微信支付消费与成本不进入企业卡券指标；仍走供应商结算。
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>日期口径</DescriptionTerm>
                            <DescriptionDetails>
                                {Object.values(DATE_BASIS_LABEL).join("；")}
                                。未配置默认口径时须显式选择，不会自动采用本月或消费发生日。
                            </DescriptionDetails>
                        </DescriptionItem>
                    </DescriptionList>
                </div>
            </QuickPreviewSheet>

            {/* 导出预览：口径/筛选/水位/覆盖率 disclaimer */}
            <QuickPreviewSheet
                open={exportPreviewOpen}
                onOpenChange={setExportPreviewOpen}
                title="导出预览"
                description="导出为当前查询数据记录，非台账副本；下载时重新鉴权。"
            >
                {data ? (
                    <div className="space-y-4 text-sm">
                        <Alert variant="warning">
                            <AlertTitle>导出免责声明</AlertTitle>
                            <AlertDescription className="space-y-2 text-xs">
                                <p>
                                    <strong>口径：</strong>
                                    销售/面值/消费/余额为含税；成本/毛差/经营贡献为不含税。无成本数据不计入利润。
                                </p>
                                <p>
                                    <strong>筛选：</strong>
                                    {data.filterSummary}
                                </p>
                                <p>
                                    <strong>数据时间：</strong>
                                    数据{" "}
                                    {formatDateTime(
                                        data.freshness.projectionUpdatedAt,
                                        "full",
                                    )}{" "}
                                    · 同步{" "}
                                    {formatDateTime(
                                        data.freshness.consumedOutboxWatermark,
                                        "full",
                                    )}{" "}
                                    · 余额记录{" "}
                                    {formatDateTime(
                                        data.freshness.balanceSnapshotAt,
                                        "full",
                                    )}{" "}
                                    · 延迟 {data.freshness.lagSeconds}s / 上限{" "}
                                    {data.freshness.maxLagSeconds}s
                                </p>
                                <p>
                                    <strong>覆盖率：</strong>
                                    {data.coverage.rate ?? "—"}（阈值{" "}
                                    {data.coverage.threshold}）
                                    {data.coverage.profitReferenceOnly
                                        ? " · 成本不完整，结果仅供参考"
                                        : ""}
                                </p>
                                <p>
                                    <strong>微信排除：</strong>
                                    {data.wechatExcludedNote}
                                </p>
                                <p>
                                    <strong>数据范围：</strong>
                                    行数 {data.rows.total}
                                    ；明细表已按当前筛选过滤并按分析视角聚合，指标与图表为全量口径。
                                </p>
                            </AlertDescription>
                        </Alert>
                        <div className="flex justify-end gap-2">
                            <Button
                                type="button"
                                variant="outline"
                                onClick={() => setExportPreviewOpen(false)}
                            >
                                取消
                            </Button>
                            <Button
                                type="button"
                                disabled={exportMutation.isPending}
                                onClick={() => void handleExportConfirm()}
                            >
                                确认导出
                            </Button>
                        </div>
                    </div>
                ) : null}
            </QuickPreviewSheet>
        </PageScaffold>
    )
}
