"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Legend,
  Line,
  LineChart,
  XAxis,
  YAxis,
} from "recharts"
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
  QuickPreviewSheet,
} from "@/components/business"
import type { DataFreshnessState } from "@/components/business/page"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
  type ChartConfig,
} from "@/components/ui/chart"
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
  useCardBusinessExportJobQuery,
  useDateBasisConfigQuery,
  useStartCardBusinessExportMutation,
} from "@/features/card-business-analytics/queries"
import type {
  CardBusinessAnalyticsQuery,
  CardBusinessDimension,
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
  COVERAGE_STATUS_UI,
  COST_BASIS_LABEL,
  COST_BASIS_ROW_UI,
  DATE_BASIS_LABEL,
  DIMENSION_LABEL,
} from "@/features/card-business-analytics/types"
import { openWorkspaceLabel } from "@/lib/ui-text"

const consumptionChartConfig = {
  sales: { label: "销售(含税)", color: "var(--chart-1)" },
  consumption: { label: "消费(含税)", color: "var(--chart-2)" },
  refund: { label: "退款(含税)", color: "var(--chart-3)" },
} satisfies ChartConfig

const basisChartConfig = {
  ACTUAL: { label: "ACTUAL", color: "var(--chart-1)" },
  STANDARD: { label: "STANDARD", color: "var(--chart-2)" },
  NONE: { label: "NONE", color: "var(--chart-3)" },
} satisfies ChartConfig

const contributionChartConfig = {
  contribution: { label: "经营贡献(不含税)", color: "var(--chart-1)" },
  margin: { label: "消费毛差(不含税)", color: "var(--chart-2)" },
  coverage: { label: "覆盖率%", color: "var(--chart-4)" },
} satisfies ChartConfig

const BASIS_COLORS: Record<CostBasisCode, string> = {
  ACTUAL: "var(--chart-1)",
  STANDARD: "var(--chart-2)",
  NONE: "var(--chart-3)",
}

function formatMoneyDisplay(value: string | undefined | null): string {
  if (value == null || value === "" || value === "—") return "—"
  const n = Number(value)
  if (!Number.isFinite(n)) return value
  return new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
  }).format(n)
}

function formatDateTime(iso: string | undefined): string {
  if (!iso) return "—"
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      hour12: false,
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return iso
  }
}

function taxBadge(basis: TaxBasis): string {
  return basis === "GROSS" ? "含税" : "不含税"
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
        s === "ACTUAL" || s === "STANDARD" || s === "NONE"
    )
  return parts.length > 0 ? parts : undefined
}

/** periodPreset → 明确 from/to（演示固定锚定 2026-08-01） */
function resolvePeriod(preset: PeriodPreset): { from: string; to: string } {
  if (preset === "last-month") {
    return { from: "2026-07-01", to: "2026-07-31" }
  }
  if (preset === "quarter-to-date") {
    return { from: "2026-07-01", to: "2026-08-01" }
  }
  return { from: "2026-08-01", to: "2026-08-01" }
}

function mapFreshnessUi(
  state: ProjectionFreshnessState,
  options?: { refreshFailed?: boolean; refreshing?: boolean; breached?: boolean }
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
  reasonCode?: string
): React.ReactNode {
  if (valueState === "masked") {
    return (
      <MoneyValue
        value={null}
        taxBasis={taxBasis === "GROSS" ? "gross" : "net"}
        unavailableReason="无字段权限"
      />
    )
  }
  if (valueState === "unavailable" || value == null) {
    return (
      <MoneyValue
        value={null}
        taxBasis={taxBasis === "GROSS" ? "gross" : "net"}
        unavailableReason={reasonCode ?? "不可计算"}
      />
    )
  }
  // 比率类直接展示
  if (value.includes("%")) {
    return (
      <span className="inline-flex flex-wrap items-baseline gap-2">
        <span className="num">{value}</span>
        <Badge variant="neutral">{taxBadge(taxBasis)}</Badge>
      </span>
    )
  }
  return (
    <MoneyValue
      value={value}
      taxBasis={taxBasis === "GROSS" ? "gross" : "net"}
    />
  )
}

export function CardBusinessAnalyticsPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

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
  const sort = searchParams.get("sort") ?? "consumptionGross:desc"
  const basisConfigScenario =
    searchParams.get("basisConfig") === "missing" ? "missing" : "default"
  const freshnessDemo = (searchParams.get("freshness") as
    | ProjectionFreshnessState
    | null) ?? undefined
  const fieldHideRaw = searchParams.get("fieldHide")
  const fieldHide =
    fieldHideRaw === "cost" || fieldHideRaw === "profit"
      ? fieldHideRaw
      : "none"

  const [explicitFrom, setExplicitFrom] = React.useState(
    from || resolvedDefault.from
  )
  const [explicitTo, setExplicitTo] = React.useState(to || resolvedDefault.to)
  const [explicitDateBasis, setExplicitDateBasis] = React.useState<DateBasis>(
    dateBasisUrl || "consumption"
  )
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 50,
  })
  const [exportJobId, setExportJobId] = React.useState<string | null>(null)
  const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)
  const [basisSheetOpen, setBasisSheetOpen] = React.useState(false)
  const [refreshFailed, setRefreshFailed] = React.useState(false)
  const [refreshing, setRefreshing] = React.useState(false)

  const basisQuery = useDateBasisConfigQuery({
    scenario: basisConfigScenario,
  })
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
    [basisConfig]
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
        freshnessDemo:
          freshnessDemo === "stale" ||
          freshnessDemo === "rebuilding" ||
          freshnessDemo === "failed" ||
          freshnessDemo === "fresh"
            ? freshnessDemo
            : undefined,
        fieldHide,
      }
    : null

  const viewQuery = useCardBusinessAnalyticsQuery(analysisQuery, analysisReady)
  const exportMutation = useStartCardBusinessExportMutation()
  const exportJobQuery = useCardBusinessExportJobQuery(exportJobId)

  function patchUrl(
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean }
  ) {
    const next = new URLSearchParams(searchParams.toString())
    for (const [key, value] of Object.entries(patch)) {
      if (value == null || value === "") next.delete(key)
      else next.set(key, value)
    }
    const qs = next.toString()
    const href = qs ? `${pathname}?${qs}` : pathname
    if (options?.replace) router.replace(href)
    else router.push(href)
  }

  function applyExplicitPeriod() {
    if (!explicitFrom || !explicitTo || !explicitDateBasis) return
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
    setRefreshFailed(false)
    try {
      await viewQuery.refetch()
    } catch {
      setRefreshFailed(true)
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
    setExportJobId(job.jobId)
  }

  const data = viewQuery.data

  const columns = React.useMemo<ColumnDef<CardBusinessRow>[]>(
    () => [
      {
        id: "customer",
        accessorFn: (r) => r.customerLabel,
        header: "客户",
        meta: { label: "客户" },
        cell: ({ row }) => (
          <Link
            href={`/sales/customers/${row.original.customerId}`}
            className="text-sm underline-offset-2 hover:underline"
          >
            {row.original.customerLabel}
          </Link>
        ),
      },
      {
        id: "salesOrder",
        accessorFn: (r) => r.salesOrderNo,
        header: "销售单",
        meta: { label: "销售单" },
        cell: ({ row }) => (
          <Link
            href={`/sales/orders/${row.original.salesOrderId}`}
            className="text-sm underline-offset-2 hover:underline"
          >
            {row.original.salesOrderNo}
          </Link>
        ),
      },
      {
        id: "category",
        accessorFn: (r) => r.voucherCategoryLabel,
        header: "卡券类目",
        meta: { label: "卡券类目" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.voucherCategoryLabel}</span>
        ),
      },
      {
        id: "cardRef",
        accessorFn: (r) => r.cardInstanceRef,
        header: "卡实例引用",
        meta: { label: "稳定卡实例引用摘要" },
        cell: ({ row }) => (
          <span
            className="num text-sm"
            title="不可逆稳定引用，不可反推卡号/卡密"
          >
            {row.original.cardInstanceRef}
          </span>
        ),
      },
      {
        id: "consumption",
        accessorFn: (r) => r.consumptionGross,
        header: "消费(含税)",
        meta: { label: "消费金额含税", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <MoneyValue value={row.original.consumptionGross} taxBasis="gross" />
        ),
      },
      {
        id: "refund",
        accessorFn: (r) => r.refundGross,
        header: "退款(含税)",
        meta: { label: "退款含税", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => (
          <MoneyValue value={row.original.refundGross} taxBasis="gross" />
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
        meta: { label: "成本不含税", width: "amount", align: "end", numeric: true },
        cell: ({ row }) => {
          if (row.original.costBasis === "NONE") {
            return (
              <MoneyValue
                value={null}
                taxBasis="net"
                unavailableReason="NONE · 非零成本"
              />
            )
          }
          return (
            <MoneyValue value={row.original.costNet} taxBasis="net" />
          )
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
                s === "covered" ? "已覆盖" : s === "partial" ? "部分" : "未覆盖"
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
          <MoneyValue
            value={row.original.unfulfilledBalanceGross}
            taxBasis="gross"
          />
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
                render={<Link href={row.original.consumptionOrderHref} />}
              >
                W25 消费单
                <ExternalLinkIcon className="size-3" aria-hidden />
              </Button>
            ) : null}
            {row.original.supplierOrderHref ? (
              <Button
                type="button"
                size="xs"
                variant="ghost"
                render={<Link href={row.original.supplierOrderHref} />}
              >
                W26 供应单
                <ExternalLinkIcon className="size-3" aria-hidden />
              </Button>
            ) : null}
            {row.original.costBasis === "NONE" && data ? (
              <Button
                type="button"
                size="xs"
                variant="ghost"
                render={<Link href={data.governanceLinks.noneCoverageHref} />}
              >
                W29 未归集
                <ExternalLinkIcon className="size-3" aria-hidden />
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    [data]
  )

  const consumptionChartData = React.useMemo(() => {
    if (!data) return []
    return data.trends.consumption.map((p) => ({
      period: p.period,
      sales: Number(p.salesGross) / 10000,
      consumption: Number(p.consumptionGross) / 10000,
      refund: Number(p.refundGross) / 10000,
      salesLabel: formatMoneyDisplay(p.salesGross),
      consumptionLabel: formatMoneyDisplay(p.consumptionGross),
      refundLabel: formatMoneyDisplay(p.refundGross),
      balanceLabel: formatMoneyDisplay(p.balanceGross),
    }))
  }, [data])

  const basisChartData = React.useMemo(() => {
    if (!data) return []
    return data.coverage.byBasis.map((s) => ({
      basis: s.basis,
      label: COST_BASIS_LABEL[s.basis],
      amount: Number(s.consumptionGross) / 10000,
      amountLabel: formatMoneyDisplay(s.consumptionGross),
      share: s.shareLabel,
      costLabel:
        s.basis === "NONE"
          ? "不计入成本"
          : formatMoneyDisplay(s.costNet),
    }))
  }, [data])

  const contributionChartData = React.useMemo(() => {
    if (!data) return []
    return data.trends.contribution.map((p) => ({
      period: p.period,
      contribution: Number(p.contributionNet) / 10000,
      margin: Number(p.marginNet) / 10000,
      coverage: p.coveragePercent,
      contributionLabel: formatMoneyDisplay(p.contributionNet),
      marginLabel: formatMoneyDisplay(p.marginNet),
      coverageLabel: p.coverageRate,
    }))
  }, [data])

  // —— Loading shells ——
  if (basisQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <Skeleton className="h-10 w-64" />
        <Skeleton className="h-24 w-full" />
        <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <Skeleton key={i} className="h-20 rounded-lg" />
          ))}
        </div>
        <Skeleton className="h-64 w-full" />
      </div>
    )
  }

  if (basisQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="system"
          title="日期口径配置加载失败"
          description="无法取得服务端默认日期口径。请重试；不会静默采用「本月/消费发生日」。"
          action={
            <Button
              type="button"
              onClick={() => void basisQuery.refetch()}
            >
              重试
            </Button>
          }
        />
      </div>
    )
  }

  // —— Q2 期间与日期口径选择态 ——
  if (analysisBlocked) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="卡券消费台账与经营分析"
          description="服务端未配置默认日期口径（DATE_BASIS_DEFAULT_UNCONFIGURED）。请显式选择期间与日期口径后开始分析；不会自动采用本月或消费发生日，也不会显示虚假 0 指标。"
          breadcrumbs={[
            { id: "an", label: "分析", href: "/analytics/card-business" },
            { id: "cb", label: "卡券经营分析", current: true },
          ]}
        />
        <Alert variant="warning">
          <CalendarRangeIcon aria-hidden="true" />
          <AlertTitle>请选择期间与日期口径</AlertTitle>
          <AlertDescription>
            指标、图表与明细在完整 from / to / dateBasis 写入 URL
            前不会发起查询。
          </AlertDescription>
        </Alert>
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>显式期间与日期口径</CardTitle>
            <CardDescription>
              选定后写入 URL 的 from/to/dateBasis，并作为全部查询的唯一上下文。
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 pt-4">
            <div className="grid gap-3 sm:grid-cols-3">
              <div className="space-y-1.5">
                <Label htmlFor="w28-from">开始日期</Label>
                <DatePicker
                  value={explicitFrom || undefined}
                  onValueChange={(next) => setExplicitFrom(next ?? "")}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="w28-to">结束日期</Label>
                <DatePicker
                  value={explicitTo || undefined}
                  onValueChange={(next) => setExplicitTo(next ?? "")}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="w28-dateBasis">日期口径</Label>
                <OptionCombobox
                  id="w28-dateBasis"
                  value={explicitDateBasis}
                  onValueChange={(v) =>
                    setExplicitDateBasis((v ?? explicitDateBasis) as DateBasis)
                  }
                  options={(basisConfig?.allowedDateBases ?? []).map((b) => ({
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
            <Button
              type="button"
              disabled={!explicitFrom || !explicitTo || !explicitDateBasis}
              onClick={applyExplicitPeriod}
            >
              开始分析
            </Button>
          </CardContent>
        </Card>
      </div>
    )
  }

  const freshnessUi = data
    ? mapFreshnessUi(data.freshness.state, {
        refreshFailed,
        refreshing,
        breached: data.freshness.slaState === "BREACHED",
      })
    : { uiState: "unknown" as const, statusLabel: "—" }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="卡券消费台账与经营分析"
        breadcrumbs={[
          { id: "an", label: "分析", href: "/analytics/card-business" },
          { id: "cb", label: "卡券经营分析", current: true },
        ]}
        metadata={
          data ? (
            <div className="flex flex-col gap-1">
              <DataFreshness
                updatedAt={formatDateTime(data.freshness.projectionUpdatedAt)}
                dateTime={data.freshness.projectionUpdatedAt}
                state={freshnessUi.uiState}
                statusLabel={freshnessUi.statusLabel}
                label="分析汇总"
              />
              <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
                <span>
                  Outbox 同步{" "}
                  <time
                    className="num"
                    dateTime={data.freshness.consumedOutboxWatermark}
                  >
                    {formatDateTime(data.freshness.consumedOutboxWatermark)}
                  </time>
                </span>
                <span aria-hidden>·</span>
                <span>
                  业务记录{" "}
                  <time
                    className="num"
                    dateTime={data.freshness.sourceFactWatermark}
                  >
                    {formatDateTime(data.freshness.sourceFactWatermark)}
                  </time>
                </span>
                {data.freshness.balanceSnapshotAt ? (
                  <>
                    <span aria-hidden>·</span>
                    <span>
                      余额记录{" "}
                      <time
                        className="num"
                        dateTime={data.freshness.balanceSnapshotAt}
                      >
                        {formatDateTime(data.freshness.balanceSnapshotAt)}
                      </time>
                      <span className="ml-1 text-muted-foreground">
                        （独立，不参与更新时效）
                      </span>
                    </span>
                  </>
                ) : null}
                <span aria-hidden>·</span>
                <span
                  className={
                    data.freshness.lagSeconds > data.freshness.maxLagSeconds
                      ? "font-medium text-destructive"
                      : "num"
                  }
                >
                  lag {data.freshness.lagSeconds}s / SLA{" "}
                  {data.freshness.maxLagSeconds}s · {data.freshness.slaState}
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
                variant: "outline",
                disabled: !data || refreshing,
                onClick: () => {
                  void handleRefresh()
                },
              },
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                mobileVisibility: "hide",
                disabled: !data?.fieldPermissions.canExport,
                onClick: () => setExportPreviewOpen(true),
              },
            ]}
          />
        }
      />

      {/* Filter bar */}
      <Card size="sm">
        <CardContent className="flex flex-col gap-3 pt-4 sm:flex-row sm:flex-wrap sm:items-end">
          <div className="space-y-1.5">
            <Label htmlFor="w28-preset">期间快捷</Label>
            <OptionCombobox
              id="w28-preset"
              value={periodFromUrl ? periodPreset : periodPreset}
              onValueChange={(v) =>
                applyPreset((v ?? periodPreset) as PeriodPreset)
              }
              options={[
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
                patchUrl({ from: next ?? "", periodPreset: null })
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
              options={(basisConfig?.allowedDateBases ?? []).map((b) => ({
                value: b.code,
                label: b.label,
              }))}
              className="w-[12rem]"
              size="sm"
              allowClear={false}
              aria-label="日期口径"
              placeholder="日期口径"
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="w28-customer">客户 ID</Label>
            <input
              id="w28-customer"
              className="flex h-9 w-full min-w-36 rounded-lg border border-input bg-transparent px-3 text-sm"
              placeholder="全部"
              value={customerId ?? ""}
              onChange={(e) =>
                patchUrl({ customerId: e.target.value.trim() || null })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="w28-so">销售单</Label>
            <input
              id="w28-so"
              className="flex h-9 w-full min-w-36 rounded-lg border border-input bg-transparent px-3 text-sm"
              placeholder="全部"
              value={salesOrderId ?? ""}
              onChange={(e) =>
                patchUrl({ salesOrderId: e.target.value.trim() || null })
              }
            />
          </div>
          <div className="space-y-1.5">
            <Label htmlFor="w28-costBasis">成本口径</Label>
            <OptionCombobox
              id="w28-costBasis"
              value={costBasis?.join(",") ?? ""}
              onValueChange={(v) => patchUrl({ costBasis: v || null })}
              options={[
                { value: "", label: "全部" },
                { value: "ACTUAL", label: "ACTUAL" },
                { value: "STANDARD", label: "STANDARD" },
                { value: "NONE", label: "NONE" },
                { value: "ACTUAL,STANDARD", label: "ACTUAL+STANDARD" },
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
            <Label htmlFor="w28-dimension">分析视角</Label>
            <OptionCombobox
              id="w28-dimension"
              value={dimension}
              onValueChange={(v) =>
                patchUrl({ dimension: v ?? dimension })
              }
              options={(
                Object.keys(DIMENSION_LABEL) as CardBusinessDimension[]
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
          kind="system"
          title="卡券经营数据加载失败"
          description="当前无缓存结果可展示。请重试或调整筛选。"
          action={
            <Button type="button" onClick={() => void viewQuery.refetch()}>
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
                  ? "保留上次成功数据供只读查阅。不会用本地估算覆盖金额。"
                  : `更新延迟 ${data.freshness.lagSeconds}s 超过固定上限 ${data.freshness.maxLagSeconds}s（${data.freshness.slaState}）。数据 ${formatDateTime(data.freshness.projectionUpdatedAt)}，同步 ${formatDateTime(data.freshness.consumedOutboxWatermark)}。余额记录独立显示，不合并为「实时」。`}
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
                    <Link href={data.governanceLinks.integrationErrorsHref} />
                  }
                >
                  {openWorkspaceLabel("W29")}
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {exportJobId && exportJobQuery.data ? (
            <BackgroundJobProgress
              mode="all-or-nothing"
              status={
                exportJobQuery.data.status === "queued"
                  ? "queued"
                  : exportJobQuery.data.status === "running"
                    ? "running"
                    : exportJobQuery.data.status === "succeeded"
                      ? "succeeded"
                      : "failed"
              }
              total={exportJobQuery.data.total}
              completed={exportJobQuery.data.completed}
              succeeded={
                exportJobQuery.data.status === "succeeded"
                  ? exportJobQuery.data.total
                  : undefined
              }
              label={`导出任务 ${exportJobQuery.data.jobId}`}
              description={
                <>
                  口径/筛选：{exportJobQuery.data.watermark.filterSummary}
                  <span className="mt-1 block">
                  覆盖率 {exportJobQuery.data.watermark.coverageRate ?? "—"} ·
                  数据{" "}
                  {formatDateTime(
                    exportJobQuery.data.watermark.projectionUpdatedAt
                  )}{" "}
                  · 同步{" "}
                  {formatDateTime(
                    exportJobQuery.data.watermark.consumedOutboxWatermark
                  )}{" "}
                  · 延迟 {exportJobQuery.data.watermark.lagSeconds}s
                  </span>
                  <span className="mt-1 block text-xs">
                    {exportJobQuery.data.watermark.taxDisclaimer}
                  </span>
                  {exportJobQuery.data.downloadLabel ? (
                    <span className="mt-1 block font-medium">
                      可下载：{exportJobQuery.data.downloadLabel}
                    </span>
                  ) : null}
                </>
              }
              action={
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  onClick={() => setExportJobId(null)}
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
                权限版本 {data.scope.permissionVersion} · 时区{" "}
                {data.scope.timezone} · 筛选摘要 digest{" "}
                <span className="num">{data.scope.filterDigest.slice(0, 48)}…</span>
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
                  消费 {formatMoneyDisplay(
                    data.coverage.byBasis.find((b) => b.basis === "ACTUAL")
                      ?.consumptionGross
                  )}{" "}
                  ·{" "}
                  {data.coverage.byBasis.find((b) => b.basis === "ACTUAL")
                    ?.shareLabel ?? "—"}
                  {data.fieldPermissions.canViewCost ? (
                    <>
                      {" "}
                      · 成本{" "}
                      {formatMoneyDisplay(
                        data.coverage.byBasis.find((b) => b.basis === "ACTUAL")
                          ?.costNet
                      )}
                    </>
                  ) : null}
                </span>
              ),
              STANDARD: (
                <span>
                  消费 {formatMoneyDisplay(
                    data.coverage.byBasis.find((b) => b.basis === "STANDARD")
                      ?.consumptionGross
                  )}{" "}
                  ·{" "}
                  {data.coverage.byBasis.find((b) => b.basis === "STANDARD")
                    ?.shareLabel ?? "—"}
                  <span className="block text-xs text-muted-foreground">
                    按历史有效供给价估算，非实际
                  </span>
                </span>
              ),
              NONE: (
                <span>
                  消费 {formatMoneyDisplay(
                    data.coverage.byBasis.find((b) => b.basis === "NONE")
                      ?.consumptionGross
                  )}{" "}
                  ·{" "}
                  {data.coverage.byBasis.find((b) => b.basis === "NONE")
                    ?.shareLabel ?? "—"}
                  <span className="block text-xs text-muted-foreground">
                    无可用成本 · 不显示 ¥0 · 不进利润
                  </span>
                </span>
              ),
            }}
            profitBasis="不含税 · 当前经营贡献 ≠ 最终利润；须同屏未履约余额"
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
                      <Link href={data.governanceLinks.noneCoverageHref} />
                    }
                  >
                    查看未归集（接口错误中心）
                  </Button>
                  <Button
                    type="button"
                    size="xs"
                    variant="outline"
                    render={<Link href={data.governanceLinks.backfillHref} />}
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
                  m.reasonCode
                )}
                detail={m.detail}
                status={
                  m.key === "currentContributionNet" &&
                  data.coverage.profitReferenceOnly
                    ? { label: "仅供参考", tone: "warning" }
                    : m.key === "consumptionMarginNet" &&
                        data.coverage.profitReferenceOnly
                      ? { label: "仅供参考", tone: "warning" }
                      : undefined
                }
              />
            ))}
          </MetricStrip>

          {!data.scopeFullyExpired ? (
            <Alert>
              <AlertTitle>最终利润未展示</AlertTitle>
              <AlertDescription>
                {data.finalProfitUnavailableReason}
                当前同屏展示「当前经营贡献」与「未履约余额」。若需最终盈亏视角，请将日期口径切换为履约到期日并筛选已到期范围。
              </AlertDescription>
            </Alert>
          ) : null}

          {/* Charts 2×2 */}
          <div className="grid min-w-0 gap-4 xl:grid-cols-2">
            <Card size="sm">
              <CardHeader className="border-b">
                <CardTitle>消费与余额趋势</CardTitle>
                <CardDescription>
                  销售 / 消费 / 退款（含税，万元展示）。筛选：{data.filterSummary}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4 pt-4">
                <ChartContainer
                  config={consumptionChartConfig}
                  className="aspect-[16/9] w-full"
                >
                  <BarChart data={consumptionChartData} accessibilityLayer>
                    <CartesianGrid vertical={false} />
                    <XAxis dataKey="period" tickLine={false} axisLine={false} />
                    <YAxis tickLine={false} axisLine={false} width={40} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Legend />
                    <Bar
                      dataKey="sales"
                      fill="var(--color-sales)"
                      radius={4}
                      name="销售(含税)"
                    />
                    <Bar
                      dataKey="consumption"
                      fill="var(--color-consumption)"
                      radius={4}
                      name="消费(含税)"
                    />
                    <Bar
                      dataKey="refund"
                      fill="var(--color-refund)"
                      radius={4}
                      name="退款(含税)"
                    />
                  </BarChart>
                </ChartContainer>
                {/* 键盘/读屏等价数据表 */}
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-xs">
                    <caption className="mb-2 text-left text-muted-foreground">
                      消费与余额趋势数据表（与图等价）
                    </caption>
                    <thead>
                      <tr className="border-b text-muted-foreground">
                        <th scope="col" className="py-1 pr-2">
                          周
                        </th>
                        <th scope="col" className="py-1 pr-2">
                          销售(含税)
                        </th>
                        <th scope="col" className="py-1 pr-2">
                          消费(含税)
                        </th>
                        <th scope="col" className="py-1 pr-2">
                          退款(含税)
                        </th>
                        <th scope="col" className="py-1">
                          余额(含税)
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {consumptionChartData.map((r) => (
                        <tr key={r.period} className="border-b border-border/60">
                          <th scope="row" className="py-1 pr-2 font-medium">
                            {r.period}
                          </th>
                          <td className="num py-1 pr-2">{r.salesLabel}</td>
                          <td className="num py-1 pr-2">{r.consumptionLabel}</td>
                          <td className="num py-1 pr-2">{r.refundLabel}</td>
                          <td className="num py-1">{r.balanceLabel}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader className="border-b">
                <CardTitle>成本口径构成</CardTitle>
                <CardDescription>
                  ACTUAL / STANDARD / NONE 消费金额占比。三者合计须等于累计卡券消费{" "}
                  {formatMoneyDisplay(data.coverage.totalConsumptionGross)}。
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4 pt-4">
                <ChartContainer
                  config={basisChartConfig}
                  className="aspect-[16/9] w-full"
                >
                  <BarChart data={basisChartData} accessibilityLayer>
                    <CartesianGrid vertical={false} />
                    <XAxis dataKey="label" tickLine={false} axisLine={false} />
                    <YAxis tickLine={false} axisLine={false} width={40} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Bar dataKey="amount" radius={4} name="消费额(万元)">
                      {basisChartData.map((entry) => (
                        <Cell
                          key={entry.basis}
                          fill={BASIS_COLORS[entry.basis as CostBasisCode]}
                        />
                      ))}
                    </Bar>
                  </BarChart>
                </ChartContainer>
                <div className="overflow-x-auto">
                  <table className="w-full text-left text-xs">
                    <caption className="mb-2 text-left text-muted-foreground">
                      成本口径构成数据表（名称 · 金额 · 占比 · 成本，非仅颜色）
                    </caption>
                    <thead>
                      <tr className="border-b text-muted-foreground">
                        <th scope="col" className="py-1 pr-2">
                          口径
                        </th>
                        <th scope="col" className="py-1 pr-2">
                          消费(含税)
                        </th>
                        <th scope="col" className="py-1 pr-2">
                          占比
                        </th>
                        <th scope="col" className="py-1">
                          成本(不含税)
                        </th>
                      </tr>
                    </thead>
                    <tbody>
                      {basisChartData.map((r) => (
                        <tr key={r.basis} className="border-b border-border/60">
                          <th scope="row" className="py-1 pr-2 font-medium">
                            {r.basis} · {r.label}
                          </th>
                          <td className="num py-1 pr-2">{r.amountLabel}</td>
                          <td className="num py-1 pr-2">{r.share}</td>
                          <td className="num py-1">{r.costLabel}</td>
                        </tr>
                      ))}
                      <tr className="font-medium">
                        <th scope="row" className="py-1 pr-2">
                          合计
                        </th>
                        <td className="num py-1 pr-2" colSpan={3}>
                          {formatMoneyDisplay(
                            data.coverage.totalConsumptionGross
                          )}{" "}
                          = 累计卡券消费
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader className="border-b">
                <CardTitle>经营贡献与覆盖率</CardTitle>
                <CardDescription>
                  利润金额不含税；覆盖率同屏辅助。筛选与指标一致。
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4 pt-4">
                {data.fieldPermissions.canViewProfit ? (
                  <>
                    <ChartContainer
                      config={contributionChartConfig}
                      className="aspect-[16/9] w-full"
                    >
                      <LineChart
                        data={contributionChartData}
                        accessibilityLayer
                      >
                        <CartesianGrid vertical={false} />
                        <XAxis
                          dataKey="period"
                          tickLine={false}
                          axisLine={false}
                        />
                        <YAxis
                          yAxisId="left"
                          tickLine={false}
                          axisLine={false}
                          width={40}
                        />
                        <YAxis
                          yAxisId="right"
                          orientation="right"
                          tickLine={false}
                          axisLine={false}
                          width={40}
                          domain={[0, 100]}
                        />
                        <ChartTooltip content={<ChartTooltipContent />} />
                        <Legend />
                        <Line
                          yAxisId="left"
                          type="monotone"
                          dataKey="contribution"
                          stroke="var(--color-contribution)"
                          name="经营贡献(万元)"
                          strokeWidth={2}
                          dot={false}
                        />
                        <Line
                          yAxisId="left"
                          type="monotone"
                          dataKey="margin"
                          stroke="var(--color-margin)"
                          name="消费毛差(万元)"
                          strokeWidth={2}
                          dot={false}
                        />
                        <Line
                          yAxisId="right"
                          type="monotone"
                          dataKey="coverage"
                          stroke="var(--color-coverage)"
                          name="覆盖率%"
                          strokeWidth={2}
                          strokeDasharray="4 4"
                          dot={false}
                        />
                      </LineChart>
                    </ChartContainer>
                    <div className="overflow-x-auto">
                      <table className="w-full text-left text-xs">
                        <caption className="mb-2 text-left text-muted-foreground">
                          经营贡献趋势数据表（与图等价）
                        </caption>
                        <thead>
                          <tr className="border-b text-muted-foreground">
                            <th scope="col" className="py-1 pr-2">
                              周
                            </th>
                            <th scope="col" className="py-1 pr-2">
                              经营贡献(不含税)
                            </th>
                            <th scope="col" className="py-1 pr-2">
                              消费毛差(不含税)
                            </th>
                            <th scope="col" className="py-1">
                              覆盖率
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {contributionChartData.map((r) => (
                            <tr
                              key={r.period}
                              className="border-b border-border/60"
                            >
                              <th scope="row" className="py-1 pr-2 font-medium">
                                {r.period}
                              </th>
                              <td className="num py-1 pr-2">
                                {r.contributionLabel}
                              </td>
                              <td className="num py-1 pr-2">{r.marginLabel}</td>
                              <td className="num py-1">{r.coverageLabel}</td>
                            </tr>
                          ))}
                        </tbody>
                      </table>
                    </div>
                  </>
                ) : (
                  <BusinessEmptyState
                    kind="no-scope"
                    title="无利润字段权限"
                    description="经营贡献趋势已隐藏；覆盖率与风险等级仍按授权可见。"
                  />
                )}
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader className="border-b">
                <CardTitle>类目 / 客户构成</CardTitle>
                <CardDescription>
                  排名不越过数据范围。筛选：{data.filterSummary}
                </CardDescription>
              </CardHeader>
              <CardContent className="grid gap-4 pt-4 sm:grid-cols-2">
                <div>
                  <h3 className="mb-2 text-sm font-medium">按类目</h3>
                  <ul className="space-y-2 text-sm">
                    {data.breakdowns.byCategory.map((item) => (
                      <li
                        key={item.id}
                        className="flex items-center justify-between gap-2"
                      >
                        <span>{item.label}</span>
                        <span className="num text-muted-foreground">
                          {formatMoneyDisplay(item.consumptionGross)} ·{" "}
                          {item.share}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
                <div>
                  <h3 className="mb-2 text-sm font-medium">按客户</h3>
                  <ul className="space-y-2 text-sm">
                    {data.breakdowns.byCustomer.map((item) => (
                      <li
                        key={item.id}
                        className="flex items-center justify-between gap-2"
                      >
                        <Link
                          href={`/sales/customers/${item.id}`}
                          className="underline-offset-2 hover:underline"
                        >
                          {item.label}
                        </Link>
                        <span className="num text-muted-foreground">
                          {formatMoneyDisplay(item.consumptionGross)} ·{" "}
                          {item.share}
                        </span>
                      </li>
                    ))}
                  </ul>
                </div>
              </CardContent>
            </Card>
          </div>

          {/* Drill-down table */}
          {data.rows.total === 0 ? (
            <BusinessEmptyState
              kind="filter"
              title="当前筛选无卡券经营记录"
              description="请调整期间、客户或成本口径。"
            />
          ) : (
            <BusinessTableFrame
              title="下钻明细"
              description={`客户 / 销售单 / 稳定卡实例引用 / 消费 / 成本口径 / 覆盖。无卡号、卡密、绑定手机号字段。共 ${data.rows.total} 行 · ${data.filterSummary}`}
              table={
                <DataTable
                  columns={columns}
                  data={[...data.rows.items]}
                  getRowId={(row) => row.rowId}
                  rowCount={data.rows.total}
                  pagination={pagination}
                  onPaginationChange={setPagination}
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
                卡券销售金额、可消费总额度、累计消费、未消费余额、未履约余额均为含税（GROSS）。
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>不含税指标</DescriptionTerm>
              <DescriptionDetails>
                实际消费成本、消费毛差、当前经营贡献、最终经营盈亏均为不含税（NET）。进项税率不被销项税率替代。
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>成本三分法</DescriptionTerm>
              <DescriptionDetails>
                ACTUAL 进入利润；STANDARD
                按消费时点有效供给价估算（不得写「实际」）；NONE
                只计入消费额与覆盖率分母，不显示成本 0，不进入利润。
              </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
              <DescriptionTerm>当前贡献 vs 最终利润</DescriptionTerm>
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
                {Object.entries(DATE_BASIS_LABEL)
                  .map(([k, v]) => `${v}（${k}）`)
                  .join("；")}
                。Q2 未配置时必须显式选择，禁止静默采用本月/消费发生日。
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
                  销售/面值/消费/余额为含税（GROSS）；成本/毛差/经营贡献为不含税（NET）。NONE
                  不按零成本计入利润。
                </p>
                <p>
                  <strong>筛选：</strong>
                  {data.filterSummary}
                </p>
                <p>
                  <strong>数据时间：</strong>
                  数据 {formatDateTime(data.freshness.projectionUpdatedAt)} ·
                  同步{" "}
                  {formatDateTime(data.freshness.consumedOutboxWatermark)} ·
                  余额记录{" "}
                  {formatDateTime(data.freshness.balanceSnapshotAt)} · 延迟{" "}
                  {data.freshness.lagSeconds}s / 上限{" "}
                  {data.freshness.maxLagSeconds}s
                </p>
                <p>
                  <strong>覆盖率：</strong>
                  {data.coverage.rate ?? "—"}（阈值{" "}
                  {data.coverage.threshold}
                  ）
                  {data.coverage.profitReferenceOnly
                    ? " · 成本不完整，结果仅供参考"
                    : ""}
                </p>
                <p>
                  <strong>微信排除：</strong>
                  {data.wechatExcludedNote}
                </p>
                <p>
                  <strong>权限版本：</strong>
                  {data.scope.permissionVersion} · 行数 {data.rows.total}
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
    </div>
  )
}
