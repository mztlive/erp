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
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  Bar,
  BarChart,
  CartesianGrid,
  Legend,
  Line,
  LineChart,
  XAxis,
  YAxis,
} from "recharts"

import {
  BackgroundJobProgress,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MetricItem,
  MetricStrip,
  OptionCombobox,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
} from "@/components/business"
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
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Progress } from "@/components/ui/progress"
import { Separator } from "@/components/ui/separator"
import { Skeleton } from "@/components/ui/skeleton"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
  useCostEntriesForRowQuery,
  useExportJobQuery,
  useMarkCorrectionPendingMutation,
  usePeriodBasisConfigQuery,
  useProfitLossViewQuery,
  useStartProfitLossExportMutation,
} from "@/features/actual-profit-loss/queries"
import type {
  CostEntryDetail,
  PeriodPreset,
  ProfitLossCoverage,
  ProfitLossDimension,
  ProfitLossQuery,
  ProfitLossRow,
  ProjectionFreshnessState,
} from "@/features/actual-profit-loss/types"
import {
  COVERAGE_FILTER_LABEL,
  COVERAGE_STATE_UI,
  DIMENSION_LABEL,
} from "@/features/actual-profit-loss/types"
import type { DataFreshnessState } from "@/components/business/page"
import { openWorkspaceLabel } from "@/lib/ui-text"

const SCOPE_LABEL = "非卡券 · 不含税"

const trendChartConfig = {
  revenue: { label: "不含税收入", color: "var(--chart-1)" },
  cost: { label: "实际成本", color: "var(--chart-2)" },
  profit: { label: "实际盈亏", color: "var(--chart-3)" },
} satisfies ChartConfig

const compositionChartConfig = {
  net: { label: "不含税金额", color: "var(--chart-4)" },
} satisfies ChartConfig

function formatMoneyDisplay(value: string | undefined | null): string {
  if (value == null || value === "") return "—"
  if (value === "—") return "—"
  const n = Number(value)
  if (!Number.isFinite(n)) return value
  const formatted = new Intl.NumberFormat("zh-CN", {
    style: "currency",
    currency: "CNY",
    minimumFractionDigits: 2,
  }).format(n)
  return formatted
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

function parseCoverage(raw: string | null): ProfitLossCoverage {
  if (raw === "uncovered" || raw === "all" || raw === "covered") return raw
  return "covered"
}

function parseDimension(raw: string | null): ProfitLossDimension {
  if (
    raw === "customer" ||
    raw === "scenario" ||
    raw === "fulfillment" ||
    raw === "cost_type" ||
    raw === "sales_order"
  ) {
    return raw
  }
  return "sales_order"
}

function parsePreset(raw: string | null): PeriodPreset {
  if (raw === "last-month" || raw === "quarter-to-date" || raw === "month-to-date") {
    return raw
  }
  return "month-to-date"
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

function mapFreshnessState(
  state: ProjectionFreshnessState,
  options?: { refreshFailed?: boolean; refreshing?: boolean }
): {
  uiState: DataFreshnessState
  statusLabel: string
} {
  if (options?.refreshing) {
    return { uiState: "syncing", statusLabel: "正在刷新数据" }
  }
  if (options?.refreshFailed) {
    return { uiState: "failed", statusLabel: "刷新失败 · 保留旧数据" }
  }
  switch (state) {
    case "stale":
      return { uiState: "stale", statusLabel: "数据陈旧 · 来源更新时间已超前" }
    case "rebuilding":
      return { uiState: "syncing", statusLabel: "数据更新中" }
    case "failed":
      return { uiState: "failed", statusLabel: "数据更新失败" }
    default:
      return { uiState: "fresh", statusLabel: "数据已更新" }
  }
}

function coveragePercentNumber(rate: string): number {
  const n = Number(rate.replace("%", ""))
  return Number.isFinite(n) ? Math.min(100, Math.max(0, n)) : 0
}

function MoneyCell({
  value,
  negativeAsText = true,
  href,
  onClick,
  ariaLabel,
}: {
  value: string | undefined
  negativeAsText?: boolean
  href?: string
  onClick?: () => void
  ariaLabel?: string
}) {
  const display = formatMoneyDisplay(value)
  const isNeg =
    negativeAsText && value != null && value !== "—" && Number(value) < 0
  const content = (
    <span
      className={`num text-sm ${isNeg ? "text-destructive" : ""}`}
      aria-label={
        ariaLabel ??
        (value == null
          ? "金额不可用"
          : `人民币 ${display}，不含税${isNeg ? "，负值" : ""}`)
      }
    >
      {isNeg ? `亏损 ${display}` : display}
    </span>
  )
  if (onClick) {
    return (
      <button
        type="button"
        className="text-left underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        onClick={onClick}
      >
        {content}
      </button>
    )
  }
  if (href) {
    return (
      <Link
        href={href}
        className="underline-offset-2 hover:underline"
        target="_blank"
        rel="noreferrer"
      >
        {content}
      </Link>
    )
  }
  return content
}

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

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [costDetailRow, setCostDetailRow] = React.useState<ProfitLossRow | null>(
    null
  )
  const [selectedCostEntryId, setSelectedCostEntryId] = React.useState<
    string | null
  >(null)
  const [exportJobId, setExportJobId] = React.useState<string | null>(null)
  const [refreshFailed, setRefreshFailed] = React.useState(false)
  const [refreshing, setRefreshing] = React.useState(false)
  const rowFocusRef = React.useRef<Map<string, HTMLElement | null>>(new Map())
  const restoreFocusIdRef = React.useRef<string | null>(null)

  const basisQuery = usePeriodBasisConfigQuery({
    scenario: basisConfigScenario,
  })
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
  }, [
    basisConfig,
    periodBasisUrl,
    searchParams,
    pathname,
    router,
    from,
    to,
  ])

  const allowedCodes = React.useMemo(
    () => new Set(basisConfig?.allowedPeriodBases.map((b) => b.code) ?? []),
    [basisConfig]
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

  const viewQuery = useProfitLossViewQuery(plQuery, analysisReady)
  const exportMutation = useStartProfitLossExportMutation()
  const exportJobQuery = useExportJobQuery(exportJobId)
  const markCorrection = useMarkCorrectionPendingMutation()

  const costIds = costDetailRow?.costEntryIds ?? []
  const costEntriesQuery = useCostEntriesForRowQuery(costIds)

  React.useEffect(() => {
    setSearchInput(qParam)
  }, [qParam])

  React.useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "/" || event.metaKey || event.ctrlKey || event.altKey)
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
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchInput])

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
  const pageRows = React.useMemo(() => {
    if (!data) return []
    const start = pagination.pageIndex * pagination.pageSize
    return data.rows.items.slice(start, start + pagination.pageSize)
  }, [data, pagination.pageIndex, pagination.pageSize])

  const openCostDetail = React.useCallback((row: ProfitLossRow) => {
    if (!row.allowedDrilldowns.includes("cost_entry") || row.costEntryIds.length === 0) {
      return
    }
    restoreFocusIdRef.current = row.rowId
    setCostDetailRow(row)
    setSelectedCostEntryId(row.costEntryIds[0] ?? null)
  }, [])

  const columns = React.useMemo<ColumnDef<ProfitLossRow>[]>(
    () => [
      {
        id: "identity",
        header: "销售单号",
        meta: { label: "销售单号", width: "default" as const },
        cell: ({ row }) => {
          const r = row.original
          const href = r.objectId
            ? `/sales/orders/${encodeURIComponent(r.objectId)}`
            : undefined
          return (
            <div className="flex flex-col gap-0.5">
              {href ? (
                <Link
                  href={href}
                  className="font-medium text-primary underline-offset-2 hover:underline"
                  ref={(el) => {
                    rowFocusRef.current.set(r.rowId, el)
                  }}
                >
                  {r.identityLabel}
                </Link>
              ) : (
                <span className="font-medium">{r.identityLabel}</span>
              )}
              <span className="text-xs text-muted-foreground">
                {r.customerLabel}
              </span>
            </div>
          )
        },
      },
      {
        id: "scenario",
        header: "福利场景",
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {row.original.benefitScenarios?.join("、") || "—"}
          </span>
        ),
      },
      {
        id: "fulfillment",
        header: "履约方式",
        cell: ({ row }) => (
          <span className="text-sm">
            {row.original.fulfillmentModes?.join("、") || "—"}
          </span>
        ),
      },
      {
        id: "revenue",
        header: "不含税收入",
        meta: { label: "不含税收入", numeric: true, align: "end" as const },
        cell: ({ row }) => (
          <MoneyCell value={row.original.netSalesRevenue} />
        ),
      },
      {
        id: "procurement",
        header: "实际采购成本",
        meta: {
          label: "实际采购成本（不含税）",
          numeric: true,
          align: "end" as const,
        },
        cell: ({ row }) => {
          const r = row.original
          if (r.coverageState === "UNCOVERED") {
            return (
              <span className="text-sm text-muted-foreground">未覆盖</span>
            )
          }
          if (r.actualProcurementCostNet == null) {
            return (
              <span className="text-sm text-muted-foreground">无权限</span>
            )
          }
          return (
            <MoneyCell
              value={r.actualProcurementCostNet}
              onClick={
                r.allowedDrilldowns.includes("cost_entry")
                  ? () => openCostDetail(r)
                  : undefined
              }
            />
          )
        },
      },
      {
        id: "fulfillmentCost",
        header: "实际履约费用",
        meta: {
          label: "实际履约费用（不含税）",
          numeric: true,
          align: "end" as const,
        },
        cell: ({ row }) => {
          const r = row.original
          if (r.coverageState === "UNCOVERED") {
            return (
              <span className="text-sm text-muted-foreground">未覆盖</span>
            )
          }
          if (r.actualFulfillmentCostNet == null) {
            return (
              <span className="text-sm text-muted-foreground">无权限</span>
            )
          }
          return (
            <MoneyCell
              value={r.actualFulfillmentCostNet}
              onClick={
                r.allowedDrilldowns.includes("cost_entry")
                  ? () => openCostDetail(r)
                  : undefined
              }
            />
          )
        },
      },
      {
        id: "reductions",
        header: "成本冲减",
        meta: { label: "成本冲减", numeric: true, align: "end" as const },
        cell: ({ row }) => {
          const r = row.original
          if (r.reductionsNet == null) {
            return <span className="text-sm text-muted-foreground">—</span>
          }
          return <MoneyCell value={r.reductionsNet} />
        },
      },
      {
        id: "profit",
        header: "实际盈亏",
        meta: {
          label: "实际盈亏（不含税）",
          numeric: true,
          align: "end" as const,
        },
        cell: ({ row }) => {
          const r = row.original
          if (r.actualProfitLossNet == null) {
            return (
              <span
                className="text-sm text-muted-foreground"
                title={r.marginUnavailableReason}
              >
                {r.coverageState === "UNCOVERED"
                  ? "不可用（未覆盖）"
                  : (r.marginUnavailableReason ?? "不可用")}
              </span>
            )
          }
          const href = r.objectId
            ? `/sales/orders/${encodeURIComponent(r.objectId)}`
            : undefined
          return <MoneyCell value={r.actualProfitLossNet} href={href} />
        },
      },
      {
        id: "margin",
        header: "实际利润率",
        meta: { label: "实际利润率", numeric: true, align: "end" as const },
        cell: ({ row }) => {
          const r = row.original
          if (r.marginRate == null) {
            return (
              <span
                className="text-sm text-muted-foreground"
                title={r.marginUnavailableReason}
              >
                {r.marginUnavailableReason ?? "不适用"}
              </span>
            )
          }
          return <span className="num text-sm">{r.marginRate}</span>
        },
      },
      {
        id: "coverage",
        header: "覆盖状态",
        cell: ({ row }) => {
          const r = row.original
          const ui = COVERAGE_STATE_UI[r.coverageState]
          const reason = r.coverageBlockers.map((b) => b.message).join("；")
          return (
            <BusinessStatusBadge
              context="list"
              label={ui.label}
              tone={ui.tone}
              description={reason || undefined}
            />
          )
        },
      },
      {
        id: "latestCost",
        header: "最近成本发生",
        cell: ({ row }) => (
          <span className="num text-xs text-muted-foreground">
            {formatDateTime(row.original.latestCostOccurredAt)}
          </span>
        ),
      },
    ],
    [openCostDetail]
  )

  const freshnessUi = data
    ? mapFreshnessState(data.freshness.state, {
        refreshFailed,
        refreshing,
      })
    : { uiState: "unknown" as const, statusLabel: "等待查询" }

  const trendChartData = React.useMemo(() => {
    if (!data) return []
    return data.trend.map((t) => ({
      period: t.period,
      revenue: Number(t.netSalesRevenue) / 10000,
      cost:
        t.actualCostNet === "—" ? null : Number(t.actualCostNet) / 10000,
      profit:
        t.actualProfitLossNet == null
          ? null
          : Number(t.actualProfitLossNet) / 10000,
      reliability: t.reliability,
    }))
  }, [data])

  const compositionChartData = React.useMemo(() => {
    if (!data?.fieldPermissions.canViewCost) return []
    return data.costComposition
      .filter((c) => c.netAmount !== "—" && Number(c.netAmount) !== 0)
      .map((c) => ({
        label: c.label,
        net: Number(c.netAmount) / 10000,
        share: c.share,
      }))
  }, [data])

  const selectedEntry: CostEntryDetail | null = React.useMemo(() => {
    if (!costEntriesQuery.data || !selectedCostEntryId) return null
    return (
      costEntriesQuery.data.find((e) => e.costEntryId === selectedCostEntryId) ??
      null
    )
  }, [costEntriesQuery.data, selectedCostEntryId])

  async function handleRefresh() {
    setRefreshing(true)
    setRefreshFailed(false)
    try {
      if (freshnessDemo === "failed") {
        // 演示刷新失败：保留旧数据
        await new Promise((r) => globalThis.setTimeout(r, 400))
        setRefreshFailed(true)
      } else {
        await viewQuery.refetch()
        await basisQuery.refetch()
      }
    } catch {
      setRefreshFailed(true)
    } finally {
      setRefreshing(false)
    }
  }

  async function handleExport() {
    if (!data || !plQuery || !analysisReady) return
    if (!data.fieldPermissions.canExport) return
    const job = await exportMutation.mutateAsync({
      query: plQuery,
      view: data,
      coverage,
    })
    setExportJobId(job.jobId)

    // 客户端落盘附带水印元数据（与后台任务一致）
    const wm = job.watermark
    const quote = (v: string) => `"${v.replaceAll('"', '""')}"`
    const metaLines = [
      `# 业务口径=非卡券·不含税`,
      `# periodFrom=${wm.periodFrom}`,
      `# periodTo=${wm.periodTo}`,
      `# periodBasis=${wm.periodBasis}`,
      `# formulaVersion=${wm.formulaVersion}`,
      `# coverage=${wm.coverage}`,
      `# scopeId=${wm.scopeId}`,
      `# scopeLabel=${wm.scopeLabel}`,
      `# permissionVersion=${wm.permissionVersion}`,
      `# projectedAt=${wm.projectedAt}`,
      `# sourceWatermark=${wm.sourceWatermark}`,
      `# amountBasis=${wm.amountBasis}`,
      `# businessType=${wm.businessType}`,
      `# rowCount=${wm.rowCount}`,
      `# jobId=${job.jobId}`,
    ]
    const header =
      "销售单号,客户,不含税收入,实际采购成本,实际履约费用,成本冲减,实际盈亏,利润率,覆盖状态,缺口原因"
    const body = data.rows.items.map((r) =>
      [
        r.identityLabel,
        r.customerLabel ?? "",
        r.netSalesRevenue,
        r.actualProcurementCostNet ?? "",
        r.actualFulfillmentCostNet ?? "",
        r.reductionsNet ?? "",
        r.actualProfitLossNet ?? "",
        r.marginRate ?? r.marginUnavailableReason ?? "",
        r.coverageState,
        r.coverageBlockers.map((b) => b.message).join("|"),
      ]
        .map((c) => quote(String(c)))
        .join(",")
    )
    const csv = [...metaLines, header, ...body].join("\n")
    const url = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = `实际盈亏-非卡券不含税-${job.jobId}.csv`
    anchor.click()
    URL.revokeObjectURL(url)
  }

  // —— 初载 / 配置加载 ——
  if (basisQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title={`实际经营盈亏（${SCOPE_LABEL}）`}
          description="读取期间归属口径配置…"
          breadcrumbs={[
            { id: "an", label: "分析", href: "/analytics/profit-loss" },
            { id: "pl", label: "实际经营盈亏", current: true },
          ]}
        />
        <Skeleton className="h-16 w-full" />
        <Skeleton className="h-24 w-full" />
        <div className="grid gap-2">
          {Array.from({ length: 5 }).map((_, i) => (
            <Skeleton key={i} className="h-10 w-full" />
          ))}
        </div>
      </div>
    )
  }

  if (basisQuery.isError || !basisConfig) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title={`实际经营盈亏（${SCOPE_LABEL}）`}
          breadcrumbs={[
            { id: "an", label: "分析", href: "/analytics/profit-loss" },
            { id: "pl", label: "实际经营盈亏", current: true },
          ]}
        />
        <BusinessFailureState
          kind="system"
          title="期间归属口径配置读取失败"
          description="无法读取期间归属口径配置，分析与导出已暂停。"
          action={
            <Button type="button" onClick={() => void basisQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
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
                updatedAt={formatDateTime(data.freshness.projectedAt)}
                dateTime={data.freshness.projectedAt}
                state={freshnessUi.uiState}
                statusLabel={freshnessUi.statusLabel}
                label="经营汇总"
              />
              <span className="text-xs text-muted-foreground">
                公式版本 {data.formulaVersion}
              </span>
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
                variant: "outline",
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
      <Card size="sm">
        <CardHeader className="border-b">
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
              value={periodPreset}
              onValueChange={(v) => {
                const preset = parsePreset(v ?? "month-to-date")
                const range = resolvePeriod(preset)
                patchUrl({
                  periodPreset: preset,
                  from: range.from,
                  to: range.to,
                })
                setPagination((p) => ({ ...p, pageIndex: 0 }))
              }}
              options={[
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
                ...basisConfig.allowedPeriodBases.map((opt) => ({
                  value: opt.code,
                  label:
                    opt.label +
                    (basisConfig.configuredPeriodBasis === opt.code
                      ? "（默认口径）"
                      : ""),
                })),
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
                      patchUrl({ periodBasis: opt.code }, { replace: true })
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
        <Card size="sm">
          <CardHeader>
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
              kind="system"
              title="盈亏数据加载失败"
              description="当前无可展示的结果。请重试或返回其他模块。"
              action={
                <Button type="button" onClick={() => void viewQuery.refetch()}>
                  重试
                </Button>
              }
            />
          ) : null}

          {data ? (
            <>
              {data.freshness.state === "stale" || refreshFailed ? (
                <Alert variant="warning">
                  <AlertTitle>
                    {refreshFailed ? "刷新失败" : "数据陈旧"}
                  </AlertTitle>
                  <AlertDescription>
                    {refreshFailed
                          ? "保留上次成功数据供只读查阅；可再次刷新。不会用本页估算覆盖金额。"
                      : `数据更新于 ${formatDateTime(data.freshness.projectedAt)}，来源已于 ${formatDateTime(data.freshness.sourceWatermark)} 更新。`}
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

              {data.correctionPendingNotice ? (
                <Alert>
                  <AlertTitle>来源纠错已登记</AlertTitle>
                  <AlertDescription>
                    {data.correctionPendingNotice}
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
                      期间 {exportJobQuery.data.watermark.periodFrom}~
                      {exportJobQuery.data.watermark.periodTo} · 归属口径{" "}
                      {exportJobQuery.data.watermark.periodBasis} · 数据更新于{" "}
                      {exportJobQuery.data.watermark.projectedAt}
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

              <Alert>
                <AlertTitle>
                  成本覆盖 · {SCOPE_LABEL} · 可靠性{" "}
                  {data.coverage.reliability === "reliable"
                    ? "可靠"
                    : data.coverage.reliability === "partial"
                      ? "部分可靠"
                      : "不可用"}
                </AlertTitle>
                <AlertDescription className="space-y-3">
                  <Progress
                    value={coveragePercentNumber(data.coverage.coverageRate)}
                  >
                    <span className="text-xs">成本覆盖率</span>
                    <span className="num ml-auto text-sm">
                      {data.coverage.coverageRate}
                    </span>
                  </Progress>
                  <div className="grid gap-2 text-sm sm:grid-cols-3">
                    <div>
                      <span className="text-muted-foreground">覆盖收入 </span>
                      <span className="num font-medium">
                        {formatMoneyDisplay(data.coverage.coveredNetRevenue)}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">未覆盖收入 </span>
                      <span className="num font-medium">
                        {formatMoneyDisplay(data.coverage.uncoveredNetRevenue)}
                      </span>
                    </div>
                    <div>
                      <span className="text-muted-foreground">利润可靠性 </span>
                      <BusinessStatusBadge
                        context="detail"
                        label={
                          data.coverage.coverageState === "complete"
                            ? "完整"
                            : data.coverage.coverageState === "partial"
                              ? "部分覆盖"
                              : "完全未覆盖"
                        }
                        tone={
                          data.coverage.coverageState === "complete"
                            ? "success"
                            : data.coverage.coverageState === "partial"
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

              <MetricStrip columns={5} aria-label="实际经营盈亏核心指标（非卡券·不含税）">
                <MetricItem
                  label="不含税销售收入"
                  value={formatMoneyDisplay(data.totals.netSalesRevenue)}
                />
                <MetricItem
                  label="实际采购成本"
                  value={
                    data.totals.actualProcurementCostNet != null
                      ? formatMoneyDisplay(data.totals.actualProcurementCostNet)
                      : "无权限"
                  }
                  detail="实际发生+冲减 · 不含税"
                />
                <MetricItem
                  label="实际履约费用"
                  value={
                    data.totals.actualFulfillmentCostNet != null
                      ? formatMoneyDisplay(data.totals.actualFulfillmentCostNet)
                      : "无权限"
                  }
                  detail="印刷/仓储/配送等 · 不含税"
                />
                <MetricItem
                  label="实际经营盈亏"
                  value={
                    data.totals.actualProfitLossNet != null
                      ? formatMoneyDisplay(data.totals.actualProfitLossNet)
                      : (data.totals.marginUnavailableReason ?? "不可用")
                  }
                  detail={
                    data.coverage.reliability === "partial"
                      ? "部分覆盖 · 仅可靠子集"
                      : SCOPE_LABEL
                  }
                  status={
                    data.totals.actualProfitLossNet != null &&
                    Number(data.totals.actualProfitLossNet) < 0
                      ? { label: "亏损", tone: "destructive" }
                      : data.coverage.reliability === "partial"
                        ? { label: "部分可靠", tone: "warning" }
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
                  <span className="mt-1 block">{data.excludedNote}</span>
                  <span className="mt-1 block">
                    当前范围：{data.filterSummary} · 权限版本{" "}
                    {data.scope.permissionVersion}
                  </span>
                </AlertDescription>
              </Alert>

              <div className="grid min-w-0 gap-4 xl:grid-cols-[3fr_2fr]">
                <Card size="sm">
                  <CardHeader className="border-b">
                    <CardTitle>盈亏趋势（{SCOPE_LABEL} · 万元）</CardTitle>
                    <CardDescription>
                      收入 / 实际成本 / 实际盈亏，与汇总同范围。
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="pt-4">
                    {data.fieldPermissions.canViewProfit ? (
                      <>
                        <ChartContainer
                          config={trendChartConfig}
                          className="aspect-[16/9] w-full"
                        >
                          <LineChart data={trendChartData} accessibilityLayer>
                            <CartesianGrid vertical={false} />
                            <XAxis
                              dataKey="period"
                              tickLine={false}
                              axisLine={false}
                            />
                            <YAxis tickLine={false} axisLine={false} width={40} />
                            <ChartTooltip content={<ChartTooltipContent />} />
                            <Legend />
                            <Line
                              type="monotone"
                              dataKey="revenue"
                              name="不含税收入"
                              stroke="var(--color-revenue)"
                              strokeWidth={2}
                              dot={false}
                            />
                            <Line
                              type="monotone"
                              dataKey="cost"
                              name="实际成本"
                              stroke="var(--color-cost)"
                              strokeWidth={2}
                              strokeDasharray="4 4"
                              dot={false}
                            />
                            <Line
                              type="monotone"
                              dataKey="profit"
                              name="实际盈亏"
                              stroke="var(--color-profit)"
                              strokeWidth={2}
                              dot={false}
                            />
                          </LineChart>
                        </ChartContainer>
                        <div className="mt-3 overflow-x-auto">
                          <table className="w-full text-left text-xs">
                            <caption className="sr-only">
                              盈亏趋势数据表，金额单位万元，非卡券不含税
                            </caption>
                            <thead>
                              <tr className="border-b text-muted-foreground">
                                <th className="py-1 pr-2">期间</th>
                                <th className="py-1 pr-2 text-right">收入</th>
                                <th className="py-1 pr-2 text-right">成本</th>
                                <th className="py-1 pr-2 text-right">盈亏</th>
                                <th className="py-1">可靠性</th>
                              </tr>
                            </thead>
                            <tbody>
                              {data.trend.map((t) => (
                                <tr key={t.period} className="border-b border-border/60">
                                  <td className="py-1 pr-2">{t.period}</td>
                                  <td className="num py-1 pr-2 text-right">
                                    {formatMoneyDisplay(t.netSalesRevenue)}
                                  </td>
                                  <td className="num py-1 pr-2 text-right">
                                    {formatMoneyDisplay(t.actualCostNet)}
                                  </td>
                                  <td className="num py-1 pr-2 text-right">
                                    {t.actualProfitLossNet != null
                                      ? formatMoneyDisplay(t.actualProfitLossNet)
                                      : "不可用"}
                                  </td>
                                  <td className="py-1 text-muted-foreground">
                                    {t.reliability}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      </>
                    ) : (
                      <p className="text-sm text-muted-foreground">
                        无利润查看权限，趋势图暂不展示。
                      </p>
                    )}
                  </CardContent>
                </Card>

                <Card size="sm">
                  <CardHeader className="border-b">
                    <CardTitle>成本构成（{SCOPE_LABEL}）</CardTitle>
                    <CardDescription>
                      仅统计实际成本与冲减；返点等冲减显示为负值贡献。
                    </CardDescription>
                  </CardHeader>
                  <CardContent className="pt-4">
                    {data.fieldPermissions.canViewCost &&
                    compositionChartData.length > 0 ? (
                      <>
                        <ChartContainer
                          config={compositionChartConfig}
                          className="aspect-[16/9] w-full"
                        >
                          <BarChart
                            data={compositionChartData}
                            accessibilityLayer
                            layout="vertical"
                            margin={{ left: 48 }}
                          >
                            <CartesianGrid horizontal={false} />
                            <XAxis type="number" tickLine={false} axisLine={false} />
                            <YAxis
                              type="category"
                              dataKey="label"
                              tickLine={false}
                              axisLine={false}
                              width={72}
                            />
                            <ChartTooltip content={<ChartTooltipContent />} />
                            <Bar
                              dataKey="net"
                              name="不含税金额（万元）"
                              fill="var(--color-net)"
                              radius={4}
                            />
                          </BarChart>
                        </ChartContainer>
                        <ul className="mt-3 space-y-1 text-xs">
                          {data.costComposition.map((c) => (
                            <li
                              key={c.costType}
                              className="flex justify-between gap-2 border-b border-border/50 py-1"
                            >
                              <span>{c.label}</span>
                              <span className="num">
                                {formatMoneyDisplay(c.netAmount)}
                                {c.share ? (
                                  <span className="ml-2 text-muted-foreground">
                                    {c.share}
                                  </span>
                                ) : null}
                              </span>
                            </li>
                          ))}
                        </ul>
                      </>
                    ) : (
                      <p className="text-sm text-muted-foreground">
                        {data.fieldPermissions.canViewCost
                          ? "当前范围无成本构成数据。"
                          : "无成本明细权限；不展示构成占比，避免通过图表比例泄露。"}
                      </p>
                    )}
                  </CardContent>
                </Card>
              </div>

              {/* EXPECTED / CONFIRMED 对照区 */}
              <Card size="sm">
                <CardHeader className="border-b">
                  <CardTitle>预计/已确认成本参考</CardTitle>
                  <CardDescription>
                    仅执行期对照，不参与实际经营盈亏或实际利润率；与实际成本使用不同文案样式。
                  </CardDescription>
                </CardHeader>
                <CardContent className="pt-4">
                  <div className="grid gap-3 md:grid-cols-2">
                    {data.stageReference.map((line) => (
                      <div
                        key={line.stage}
                        className="rounded-lg border border-dashed border-border bg-muted/30 p-3"
                      >
                        <div className="flex items-center justify-between gap-2">
                          <span className="text-sm font-medium">{line.label}</span>
                          <Badge variant="outline">{line.stage}</Badge>
                        </div>
                        <DescriptionList columns="two" className="mt-2">
                          <DescriptionItem>
                            <DescriptionTerm>采购（对照）</DescriptionTerm>
                            <DescriptionDetails>
                              <span className="num text-muted-foreground">
                                {formatMoneyDisplay(line.procurementCostNet)}
                              </span>
                            </DescriptionDetails>
                          </DescriptionItem>
                          <DescriptionItem>
                            <DescriptionTerm>履约（对照）</DescriptionTerm>
                            <DescriptionDetails>
                              <span className="num text-muted-foreground">
                                {formatMoneyDisplay(line.fulfillmentCostNet)}
                              </span>
                            </DescriptionDetails>
                          </DescriptionItem>
                          <DescriptionItem>
                            <DescriptionTerm>合计（对照）</DescriptionTerm>
                            <DescriptionDetails>
                              <span className="num text-muted-foreground">
                                {formatMoneyDisplay(line.totalNet)}
                              </span>
                            </DescriptionDetails>
                          </DescriptionItem>
                        </DescriptionList>
                        <p className="mt-2 text-xs text-muted-foreground">
                          {line.note}
                        </p>
                      </div>
                    ))}
                  </div>
                </CardContent>
              </Card>

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
                      onChange={(e) => setSearchInput(e.target.value)}
                      aria-label="搜索销售单或客户"
                    />
                  </InputGroup>
                }
                filters={
                  <div className="flex flex-wrap items-center gap-2">
                    <Label htmlFor="coverage-filter" className="sr-only">
                      成本覆盖
                    </Label>
                    <OptionCombobox
                      id="coverage-filter"
                      value={coverage}
                      onValueChange={(v) => {
                        patchUrl({
                          coverage: (v ?? coverage) as ProfitLossCoverage,
                        })
                        setPagination((p) => ({ ...p, pageIndex: 0 }))
                      }}
                      options={(
                        Object.keys(
                          COVERAGE_FILTER_LABEL
                        ) as ProfitLossCoverage[]
                      ).map((key) => ({
                        value: key,
                        label: COVERAGE_FILTER_LABEL[key],
                      }))}
                      className="w-[10rem]"
                      size="sm"
                      allowClear={false}
                      aria-label="成本覆盖"
                      placeholder="成本覆盖"
                    />
                  </div>
                }
              />

              <Tabs
                value={dimension}
                onValueChange={(v) => {
                  patchUrl({ dimension: v })
                  setPagination((p) => ({ ...p, pageIndex: 0 }))
                }}
              >
                <TabsList>
                  {(
                    Object.keys(DIMENSION_LABEL) as ProfitLossDimension[]
                  ).map((key) => (
                    <TabsTrigger key={key} value={key}>
                      {DIMENSION_LABEL[key]}
                    </TabsTrigger>
                  ))}
                </TabsList>
              </Tabs>

              {data.rows.total === 0 ? (
                <BusinessEmptyState
                  kind="filter"
                  title="当前筛选无非卡券经营结果"
                  description={`范围：${data.filterSummary}。可调整期间、覆盖口径或清除搜索。`}
                  action={
                    <Button
                      type="button"
                      variant="outline"
                      onClick={() => {
                        patchUrl({
                          q: null,
                          customerId: null,
                          salesOrderId: null,
                          coverage: "covered",
                        })
                      }}
                    >
                      清除筛选
                    </Button>
                  }
                />
              ) : (
                <BusinessTableFrame
                  title={`明细 · ${DIMENSION_LABEL[dimension]}（${SCOPE_LABEL}）`}
                  description={`共 ${data.rows.total} 行 · 与指标/图表同一数据范围 · 点击盈亏下钻销售单· 点击成本金额打开成本记录 detail`}
                  table={
                    <DataTable
                      data={pageRows}
                      columns={columns}
                      getRowId={(row) => row.rowId}
                      rowCount={data.rows.total}
                      pagination={pagination}
                      onPaginationChange={setPagination}
                      layout="flush"
                      density="compact"
                    />
                  }
                />
              )}

              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={markCorrection.isPending}
                  onClick={() => {
                    void markCorrection.mutateAsync()
                  }}
                >
                  演示：来源纠错后等待刷新
                </Button>
                <span className="text-xs text-muted-foreground self-center">
                  纠错后不本地改金额；固定提示等待数据追平。
                </span>
              </div>
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
        title="成本记录 detail"
        description="只读 · 不含税；含税仅作税额展示。"
        identity={
          costDetailRow ? (
            <span>
              销售单 {costDetailRow.identityLabel} · {costDetailRow.customerLabel}
            </span>
          ) : null
        }
        summary={
          costDetailRow ? (
            <BusinessStatusBadge
              context="preview"
              label={COVERAGE_STATE_UI[costDetailRow.coverageState].label}
              tone={COVERAGE_STATE_UI[costDetailRow.coverageState].tone}
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
              <Button type="button" size="sm" variant="outline"
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
          {costDetailRow && costDetailRow.coverageBlockers.length > 0 ? (
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
          ) : costEntriesQuery.data && costEntriesQuery.data.length > 0 ? (
            <>
              <div className="flex flex-wrap gap-2">
                {costEntriesQuery.data.map((entry) => (
                  <Button
                    key={entry.costEntryId}
                    type="button"
                    size="sm"
                    variant={
                      selectedCostEntryId === entry.costEntryId
                        ? "default"
                        : "outline"
                    }
                    onClick={() => setSelectedCostEntryId(entry.costEntryId)}
                  >
                    {entry.costTypeLabel} · {entry.stageLabel}
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
    </div>
  )
}

const W16_FORMULA_HINT =
  "实际经营盈亏（不含税）= 非卡券不含税销售收入 − 非卡券不含税实际采购成本 − 非卡券不含税实际履约费用"

function CostEntryDetailBody({ entry }: { entry: CostEntryDetail }) {
  return (
    <div className="space-y-4">
      <DescriptionList columns="two">
        <DescriptionItem>
          <DescriptionTerm>费用类型</DescriptionTerm>
          <DescriptionDetails>
            {entry.costTypeLabel}（{entry.costType}）
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>阶段</DescriptionTerm>
          <DescriptionDetails>
            <Badge variant="secondary">
              {entry.stage} · {entry.stageLabel}
            </Badge>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>范围</DescriptionTerm>
          <DescriptionDetails>
            {entry.costScopeLabel}（{entry.costScope}）
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>供应商</DescriptionTerm>
          <DescriptionDetails>{entry.supplierName ?? "—"}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>含税金额</DescriptionTerm>
          <DescriptionDetails>
            <span className="num">{formatMoneyDisplay(entry.amountGross)}</span>
            <span className="ml-1 text-xs text-muted-foreground">
              仅展示，不参与利润
            </span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>税率 / 税额</DescriptionTerm>
          <DescriptionDetails>
            <span className="num">
              {entry.taxRate} / {formatMoneyDisplay(entry.taxAmount)}
            </span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>不含税金额</DescriptionTerm>
          <DescriptionDetails>
            <span className="num font-medium">
              {formatMoneyDisplay(entry.amountNet)}
            </span>
            <span className="ml-1 text-xs text-muted-foreground">利润口径</span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>发生时间</DescriptionTerm>
          <DescriptionDetails>
            <span className="num">{formatDateTime(entry.occurredAt)}</span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>来源类型</DescriptionTerm>
          <DescriptionDetails>
            {entry.sourceTypeLabel}（{entry.sourceType}）
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>来源单据</DescriptionTerm>
          <DescriptionDetails>
            {entry.sourceDocumentNo}
            <span className="ml-1 text-xs text-muted-foreground">
              {entry.sourceDocumentId}
            </span>
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>来源明细</DescriptionTerm>
          <DescriptionDetails>
            {entry.sourceLineLabel ?? "—"}
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>来源版本</DescriptionTerm>
          <DescriptionDetails>{entry.sourceVersion}</DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>销售单 / 明细</DescriptionTerm>
          <DescriptionDetails>
            {entry.salesOrderNo}
            {entry.salesOrderLineLabel
              ? ` · ${entry.salesOrderLineLabel}`
              : ""}
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>原成本引用</DescriptionTerm>
          <DescriptionDetails>
            {entry.originalCostEntryLabel ?? "—（非冲减）"}
          </DescriptionDetails>
        </DescriptionItem>
        <DescriptionItem>
          <DescriptionTerm>凭证授权摘要</DescriptionTerm>
          <DescriptionDetails>{entry.voucherSummary ?? "—"}</DescriptionDetails>
        </DescriptionItem>
      </DescriptionList>

      {entry.correctionHref ? (
        <Alert>
          <AlertTitle>前往纠错来源</AlertTitle>
          <AlertDescription className="flex flex-col gap-2">
            <span>
              W16 不执行变更确认。打开原业务对象使用变更/冲减流程后，返回本页等待数据刷新。
            </span>
            <Button type="button" size="sm" variant="outline"
              render={<Link href={entry.correctionHref} target="_blank" />}
            >
                {entry.correctionLabel ?? "打开来源"}
                <ExternalLinkIcon className="ml-1 size-3.5" />
            </Button>
          </AlertDescription>
        </Alert>
      ) : null}
    </div>
  )
}
