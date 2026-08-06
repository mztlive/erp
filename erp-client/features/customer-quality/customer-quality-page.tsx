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
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  XAxis,
  YAxis,
} from "recharts"
import {
  CalendarRangeIcon,
  DownloadIcon,
  InfoIcon,
  RefreshCwIcon,
  SearchIcon,
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
  GuardedBusinessAction,
  MetricFilterItem,
  MetricItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageHeader,
} from "@/components/business"
import { formatDateTime } from "@/lib/datetime"
import { patchUrl as patchSearchParams } from "@/lib/patch-search-params"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardAction,
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
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { DatePicker } from "@/components/ui/date-picker"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import { Skeleton } from "@/components/ui/skeleton"
import {
  useCustomerQualityPeriodPolicyQuery,
  useCustomerQualityQuery,
  useRefreshCustomerQualityMutation,
  useStartCustomerQualityExportMutation,
} from "@/features/customer-quality/queries"
import type {
  BusinessTag,
  CustomerQualityExportJob,
  CustomerQualityQuery,
  CustomerQualityRow,
  CustomerQualityScenario,
  FundsReviewFilter,
  PeriodSelectionSource,
} from "@/features/customer-quality/types"
import type { DataFreshnessState } from "@/components/business/page"
import { openWorkspaceLabel } from "@/lib/ui-text"

const chartConfig = {
  value: { label: "数值", color: "var(--chart-1)" },
  active: { label: "选中", color: "var(--chart-2)" },
} satisfies ChartConfig

const SCENARIOS = new Set<CustomerQualityScenario>([
  "default",
  "no_period_default",
  "empty",
  "no_scope",
  "forbidden",
  "field_denied",
  "stale",
  "rebuilding",
  "failed",
  "refresh_failed",
])

function parseScenario(raw: string | null): CustomerQualityScenario | undefined {
  if (raw && SCENARIOS.has(raw as CustomerQualityScenario)) {
    return raw as CustomerQualityScenario
  }
  return undefined
}

function parseFundsReview(raw: string | null): FundsReviewFilter {
  return raw === "reviewed_only" ? "reviewed_only" : "all"
}

function formatClock(iso: string) {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      hour: "2-digit",
      minute: "2-digit",
      hour12: false,
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

/** 来源更新位置形如 outbox:cq:2026-08-01T09:35:48+08:00，提取可读时间部分。 */
function formatSourceWatermark(w: string): string {
  const m = w.match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
  return m ? m[0] : w
}

function freshnessPresentation(
  state: "fresh" | "stale" | "rebuilding" | "failed",
  refreshFailed?: boolean,
  refreshing?: boolean
): { state: DataFreshnessState; statusLabel: string } {
  if (refreshing) return { state: "syncing", statusLabel: "正在刷新" }
  if (refreshFailed) return { state: "failed", statusLabel: "刷新失败（保留旧数据）" }
  if (state === "failed") return { state: "failed", statusLabel: "数据加载失败" }
  if (state === "rebuilding") return { state: "syncing", statusLabel: "正在重建" }
  if (state === "stale") return { state: "stale", statusLabel: "数据可能不是最新" }
  return { state: "fresh", statusLabel: "数据已更新" }
}

function buildReturnTo(pathname: string, params: URLSearchParams): string {
  const qs = params.toString()
  return qs ? `${pathname}?${qs}` : pathname
}

function withReturnFocus(
  returnTo: string,
  customerId: string,
  focusMetric?: string
) {
  const [path, query = ""] = returnTo.split("?", 2)
  const params = new URLSearchParams(query)
  params.set("focusCustomerId", customerId)
  if (focusMetric) params.set("focusMetric", focusMetric)
  return `${path}?${params.toString()}`
}

function customerHref(customerId: string, customerName: string, returnTo: string) {
  const p = new URLSearchParams()
  p.set("from", "W15")
  p.set("customerName", customerName)
  p.set("returnTo", returnTo)
  return `/sales/customers/${customerId}?${p.toString()}`
}

function salesOrdersHref(
  row: CustomerQualityRow,
  period: { from: string; to: string },
  returnTo: string,
  businessType?: "VOUCHER" | "GOODS_SERVICE"
) {
  const p = new URLSearchParams()
  p.set("search", row.customerName)
  p.set("customerId", row.customerId)
  p.set("customerName", row.customerName)
  p.set("from", "W15")
  p.set("periodFrom", period.from)
  p.set("periodTo", period.to)
  p.set("returnTo", returnTo)
  if (businessType) {
    p.set(
      "nature",
      businessType === "VOUCHER" ? "card_voucher" : "physical_service"
    )
  }
  return `/sales/orders?${p.toString()}`
}

function receivablesHref(
  row: CustomerQualityRow,
  period: { from: string; to: string },
  returnTo: string
) {
  const p = new URLSearchParams()
  p.set("view", "receivable")
  p.set("customerId", row.customerId)
  p.set("customerName", row.customerName)
  p.set("due", "overdue")
  p.set("from", "W15")
  p.set("periodFrom", period.from)
  p.set("periodTo", period.to)
  p.set("returnTo", returnTo)
  return `/finance/customer-accounts?${p.toString()}`
}

function profitLossHref(
  row: CustomerQualityRow,
  period: { from: string; to: string },
  returnTo: string
) {
  const p = new URLSearchParams()
  p.set("customerId", row.customerId)
  p.set("customerName", row.customerName)
  p.set("from", period.from)
  p.set("to", period.to)
  p.set("source", "W15")
  p.set("returnTo", returnTo)
  return `/analytics/profit-loss?${p.toString()}`
}

function metricReliabilityDetail(
  reliability: string,
  explanation?: string,
  fieldDenied?: boolean
) {
  if (fieldDenied) return "当前角色不可查看"
  if (reliability === "partial") return explanation ?? "部分可靠"
  if (reliability === "unavailable") return explanation ?? "暂无可靠口径"
  return explanation
}

export function CustomerQualityPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()

  const scenario = parseScenario(searchParams.get("scenario"))
  const fromParam = searchParams.get("from")
  const toParam = searchParams.get("to")
  const fundsReview = parseFundsReview(searchParams.get("fundsReview"))
  const businessTypeRaw = searchParams.get("businessType")
  const businessType =
    businessTypeRaw === "VOUCHER" || businessTypeRaw === "GOODS_SERVICE"
      ? businessTypeRaw
      : undefined
  const scaleTag = searchParams.get("scaleTag") ?? undefined
  const profitTag = searchParams.get("profitTag") ?? undefined
  const riskTag = searchParams.get("riskTag") ?? undefined
  const qParam = searchParams.get("q") ?? ""
  const sort =
    searchParams.get("sort") ?? "salesGrossAmount:desc"
  const chartDimension = searchParams.get("chartDimension") ?? undefined
  const chartCode = searchParams.get("chartCode") ?? undefined
  const customerId = searchParams.get("customerId") ?? undefined
  const focusCustomerId = searchParams.get("focusCustomerId") ?? undefined
  const focusMetric = searchParams.get("focusMetric") ?? undefined
  const periodPreset = searchParams.get("periodPreset") ?? undefined
  // 数据范围固定为当前角色默认范围；不接受 URL 隐形覆盖（URL 参数与控件一一对应）
  const scopeId = "scope:team:sales-east"

  const [searchInput, setSearchInput] = React.useState(qParam)
  const [explicitFrom, setExplicitFrom] = React.useState(fromParam ?? "")
  const [explicitTo, setExplicitTo] = React.useState(toParam ?? "")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [tagDialog, setTagDialog] = React.useState<BusinessTag | null>(null)
  const [exportJob, setExportJob] = React.useState<CustomerQualityExportJob | null>(
    null
  )
  const [refreshError, setRefreshError] = React.useState(false)
  const [periodWriteDone, setPeriodWriteDone] = React.useState(false)
  const rowFocusRef = React.useRef<string | null>(focusCustomerId ?? null)

  const periodPolicyQuery = useCustomerQualityPeriodPolicyQuery(scenario)
  const periodPolicy = periodPolicyQuery.data

  // Apply server default period into URL when missing (never silent calendar-year client fallback).
  React.useEffect(() => {
    if (periodWriteDone) return
    if (!periodPolicy) return
    if (fromParam && toParam) {
      setPeriodWriteDone(true)
      return
    }
    if (periodPolicy.hasDefault && periodPolicy.from && periodPolicy.to) {
      const next = new URLSearchParams(searchParams.toString())
      next.set("from", periodPolicy.from)
      next.set("to", periodPolicy.to)
      if (periodPolicy.customerQualityPeriodPolicyId) {
        next.set(
          "customerQualityPeriodPolicyId",
          periodPolicy.customerQualityPeriodPolicyId
        )
      }
      if (periodPolicy.customerQualityPeriodPolicyVersion != null) {
        next.set(
          "customerQualityPeriodPolicyVersion",
          String(periodPolicy.customerQualityPeriodPolicyVersion)
        )
      }
      if (!next.get("periodPreset") && periodPolicy.selectionSource === "SERVER_DEFAULT") {
        next.set("periodPreset", "ytd")
      }
      router.replace(`${pathname}?${next.toString()}`)
      setPeriodWriteDone(true)
    } else {
      setPeriodWriteDone(true)
    }
  }, [
    periodPolicy,
    fromParam,
    toParam,
    periodWriteDone,
    pathname,
    router,
    searchParams,
  ])

  const resolvedFrom = fromParam ?? undefined
  const resolvedTo = toParam ?? undefined
  const hasPeriod = Boolean(resolvedFrom && resolvedTo)
  const needsPeriodBlocker =
    periodWriteDone &&
    !hasPeriod &&
    periodPolicy != null &&
    !periodPolicy.hasDefault

  const periodSelectionSource: PeriodSelectionSource =
    searchParams.get("periodSelectionSource") === "EXPLICIT" ||
    (hasPeriod && !periodPolicy?.hasDefault)
      ? "EXPLICIT"
      : periodPreset
        ? "CONFIGURED_PRESET"
        : "SERVER_DEFAULT"

  const analysisQuery: CustomerQualityQuery | null = React.useMemo(() => {
    if (!resolvedFrom || !resolvedTo) return null
    return {
      from: resolvedFrom,
      to: resolvedTo,
      periodBasis:
        periodSelectionSource === "EXPLICIT"
          ? "EXPLICIT"
          : (periodPolicy?.periodBasis ?? "BUSINESS_DATE"),
      periodSelectionSource,
      customerQualityPeriodPolicyId:
        searchParams.get("customerQualityPeriodPolicyId") ??
        periodPolicy?.customerQualityPeriodPolicyId,
      customerQualityPeriodPolicyVersion: Number(
        searchParams.get("customerQualityPeriodPolicyVersion") ??
          periodPolicy?.customerQualityPeriodPolicyVersion ??
          NaN
      )
        ? Number(
            searchParams.get("customerQualityPeriodPolicyVersion") ??
              periodPolicy?.customerQualityPeriodPolicyVersion
          )
        : undefined,
      scopeId,
      fundsReview,
      businessType,
      scaleTag,
      profitTag,
      riskTag,
      q: qParam || undefined,
      sort,
      page: pagination.pageIndex + 1,
      pageSize: pagination.pageSize,
      chartDimension,
      chartCode,
      customerId,
      scenario,
    }
  }, [
    resolvedFrom,
    resolvedTo,
    periodSelectionSource,
    periodPolicy,
    searchParams,
    scopeId,
    fundsReview,
    businessType,
    scaleTag,
    profitTag,
    riskTag,
    qParam,
    sort,
    pagination.pageIndex,
    pagination.pageSize,
    chartDimension,
    chartCode,
    customerId,
    scenario,
  ])

  const viewQuery = useCustomerQualityQuery(analysisQuery)
  const exportMutation = useStartCustomerQualityExportMutation()
  const refreshMutation = useRefreshCustomerQualityMutation()

  async function handleRefresh() {
    setRefreshError(false)
    try {
      await refreshMutation.mutateAsync()
      await viewQuery.refetch()
    } catch {
      setRefreshError(true)
    }
  }

  const data = viewQuery.data

  function patchUrl(
    patch: Record<string, string | null | undefined>,
    options?: { replace?: boolean }
  ) {
    patchSearchParams({ router, pathname, searchParams }, patch, options)
  }

  const resetPage = React.useCallback(() => {
    setPagination((p) => ({ ...p, pageIndex: 0 }))
  }, [])

  const returnTo = React.useMemo(
    () => buildReturnTo(pathname, new URLSearchParams(searchParams.toString())),
    [pathname, searchParams]
  )

  // 排序状态与顶部「排序」下拉同源（URL sort）；表头点击回写同一参数。
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
          : "salesGrossAmount:desc",
      })
      resetPage()
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [sort]
  )

  const periodInvalid = Boolean(
    resolvedFrom && resolvedTo && resolvedFrom > resolvedTo
  )

  // 定位失败降级：目标客户不在当前页/排序结果时滚动到明细表顶部
  const tableSectionRef = React.useRef<HTMLDivElement>(null)
  const scrollToTableTop = React.useCallback(() => {
    tableSectionRef.current?.scrollIntoView({
      behavior: "smooth",
      block: "start",
    })
    tableSectionRef.current?.focus({ preventScroll: true })
  }, [])

  // Restore row focus after returning from drill
  React.useEffect(() => {
    if (!focusCustomerId || !data) return
    const frame = window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        const row = document.querySelector<HTMLElement>(
          `[data-customer-row="${CSS.escape(focusCustomerId)}"]`
        )
        const metricTarget = focusMetric
          ? document.querySelector<HTMLElement>(
              `[data-customer-id="${CSS.escape(focusCustomerId)}"][data-focus-metric="${CSS.escape(focusMetric)}"]`
            )
          : null
        if (metricTarget ?? row) {
          ;(metricTarget ?? row)?.focus()
          rowFocusRef.current = focusCustomerId
        } else {
          scrollToTableTop()
        }
      })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [focusCustomerId, focusMetric, data, scrollToTableTop])

  const columns = React.useMemo<ColumnDef<CustomerQualityRow>[]>(
    () => [
      {
        id: "customerName",
        accessorKey: "customerName",
        header: "客户",
        meta: { label: "客户", width: "reference" },
        cell: ({ row }) => {
          const r = row.original
          const canW03 = r.allowedDrilldowns.includes("W03")
          return (
            <div
              className="min-w-0"
              data-customer-row={r.customerId}
              tabIndex={-1}
            >
              <div className="flex flex-wrap items-center gap-2">
                {canW03 ? (
                  <Button
                    type="button"
                    variant="link"
                    size="xs"
                    className="h-auto px-0 font-medium"
                    render={
                      <Link
                        href={customerHref(
                          r.customerId,
                          r.customerName,
                          withReturnFocus(returnTo, r.customerId)
                        )}
                      />
                    }
                  >
                    {r.customerName}
                  </Button>
                ) : (
                  <span className="text-sm font-medium">{r.customerName}</span>
                )}
                <span className="num text-xs text-muted-foreground">
                  {r.customerNo}
                </span>
              </div>
              <div className="mt-0.5 text-xs text-muted-foreground">
                {r.ownerLabels.join(" · ")}
              </div>
            </div>
          )
        },
      },
      {
        id: "tags",
        header: "经营标签",
        meta: { label: "经营标签" },
        cell: ({ row }) => (
          <div className="flex flex-wrap gap-1">
            {row.original.tags.map((t) => (
              <button
                key={`${t.type}-${t.code}`}
                type="button"
                className="inline-flex"
                onClick={() => setTagDialog(t)}
                aria-label={`${t.label}：查看规则说明`}
              >
                <BusinessStatusBadge
                  context="list"
                  label={t.label}
                  tone={t.tone}
                />
              </button>
            ))}
          </div>
        ),
      },
      {
        id: "salesGrossAmount",
        accessorFn: (r) => r.salesGrossAmount,
        header: "成交金额（含税）",
        meta: { label: "成交金额（含税）", align: "end", numeric: true },
        cell: ({ row }) => {
          const r = row.original
          const content = (
            <div className="text-right">
              <MoneyValue value={r.salesGrossAmount} taxBasis="gross" />
              <div className="text-xs text-muted-foreground">
                {r.salesOrderCount} 单 · 卡券占比 {r.voucherShare}
              </div>
            </div>
          )
          if (!data || !r.allowedDrilldowns.includes("W05")) return content
          return (
            <Link
              data-customer-id={r.customerId}
              data-focus-metric="salesGrossAmount"
              href={salesOrdersHref(
                r,
                { from: data.period.from, to: data.period.to },
                withReturnFocus(returnTo, r.customerId, "salesGrossAmount"),
                businessType
              )}
              className="block text-right text-primary underline-offset-4 hover:underline"
            >
              {content}
            </Link>
          )
        },
      },
      {
        id: "costCoverageRate",
        accessorFn: (r) => r.costCoverageRate ?? "",
        header: "成本覆盖",
        meta: { label: "成本覆盖", align: "end" },
        cell: ({ row }) => {
          const r = row.original
          if (
            r.costCoveredNetRevenue == null ||
            r.costUncoveredNetRevenue == null ||
            r.costCoverageRate == null
          ) {
            return (
              <span className="text-sm text-muted-foreground">
                卡券/未覆盖 — 不显示为 0
              </span>
            )
          }
          return (
            <div className="text-right text-xs">
              <div>
                覆盖{" "}
                <MoneyValue value={r.costCoveredNetRevenue} taxBasis="net" />
              </div>
              <div className="text-muted-foreground">
                未覆盖{" "}
                <MoneyValue value={r.costUncoveredNetRevenue} taxBasis="net" />
              </div>
              <div className="num font-medium">{r.costCoverageRate}</div>
            </div>
          )
        },
      },
      {
        id: "actualProfitLossNet",
        accessorFn: (r) => r.actualProfitLossNet ?? "",
        header: "实际盈亏（不含税）",
        meta: { label: "实际盈亏（不含税）", align: "end", numeric: true },
        cell: ({ row }) => {
          const r = row.original
          if (r.actualProfitLossNet == null) {
            return (
              <span className="text-sm text-muted-foreground">
                暂无可靠口径
              </span>
            )
          }
          const canW16 = r.allowedDrilldowns.includes("W16")
          const content = (
            <div className="text-right">
              <MoneyValue value={r.actualProfitLossNet} taxBasis="net" />
              {r.marginRate ? (
                <div className="text-xs text-muted-foreground">
                  利润率 {r.marginRate}
                </div>
              ) : null}
            </div>
          )
          if (!canW16 || !data) return content
          return (
            <Link
              data-customer-id={r.customerId}
              data-focus-metric="actualProfitLossNet"
              href={profitLossHref(
                r,
                { from: data.period.from, to: data.period.to },
                withReturnFocus(returnTo, r.customerId, "actualProfitLossNet")
              )}
              className="block text-right text-primary underline-offset-4 hover:underline"
            >
              {content}
            </Link>
          )
        },
      },
      {
        id: "receivableOpenGross",
        accessorFn: (r) => r.receivableOpenGross ?? "",
        header: "应收 / 逾期（含税）",
        meta: { label: "应收 / 逾期（含税）", align: "end" },
        cell: ({ row }) => {
          const r = row.original
          const canW11 = r.allowedDrilldowns.includes("W11")
          return (
            <div className="text-right text-xs">
              <div className="flex flex-wrap items-center justify-end gap-1">
                <MoneyValue
                  value={r.receivableOpenGross}
                  taxBasis="gross"
                  unavailableReason={
                    r.receivableOpenGross == null
                      ? "当前角色不可查看"
                      : undefined
                  }
                />
                {r.cardFundsReviewInsufficient ? (
                  <Badge variant="warning">票款未复核</Badge>
                ) : null}
              </div>
              {canW11 && data && r.overdueGross != null ? (
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="h-auto px-0 text-destructive"
                  render={
                    <Link
                      href={receivablesHref(
                        r,
                        { from: data.period.from, to: data.period.to },
                        withReturnFocus(returnTo, r.customerId, "overdueGross")
                      )}
                      data-customer-id={r.customerId}
                      data-focus-metric="overdueGross"
                    />
                  }
                >
                  逾期{" "}
                  <MoneyValue value={r.overdueGross} taxBasis="gross" />
                </Button>
              ) : (
                <div className="text-muted-foreground">
                  逾期{" "}
                  <MoneyValue
                    value={r.overdueGross}
                    taxBasis="gross"
                    unavailableReason={
                      r.overdueGross == null ? "—" : undefined
                    }
                  />
                </div>
              )}
            </div>
          )
        },
      },
      {
        id: "exceptions",
        header: "异常",
        meta: { label: "异常" },
        cell: ({ row }) => {
          const e = row.original.exceptionCounts
          const parts = [
            e.return ? `退货 ${e.return}` : null,
            e.refund ? `退款 ${e.refund}` : null,
            e.reject ? `拒收 ${e.reject}` : null,
            e.other ? `其他 ${e.other}` : null,
          ].filter(Boolean)
          return (
            <span className="text-sm text-muted-foreground">
              {parts.length ? parts.join(" · ") : "—"}
            </span>
          )
        },
      },
      {
        id: "latestBusinessAt",
        accessorFn: (r) => r.latestBusinessAt ?? "",
        header: "最近业务",
        meta: { label: "最近业务" },
        cell: ({ row }) => (
          <span className="num text-xs text-muted-foreground">
            {row.original.latestBusinessAt
              ? formatDateTime(row.original.latestBusinessAt, "full", "passthrough")
              : "—"}
          </span>
        ),
      },
    ],
    [businessType, data, returnTo]
  )

  const scaleDimension = data?.dimensions.find((d) => d.key === "scale")
  const profitDimension = data?.dimensions.find((d) => d.key === "profit")
  const natureDimension = data?.dimensions.find(
    (d) => d.key === "businessNature"
  )

  const chartFilterSummary = React.useMemo(() => {
    if (!chartDimension || !chartCode || !data) return null
    const dim = data.dimensions.find((d) => d.key === chartDimension)
    const item = dim?.items.find((i) => i.code === chartCode)
    if (!dim || !item) return null
    return {
      dimensionTitle: dim.title,
      itemLabel: item.label,
      resultCount: data.customers.filteredTotal,
    }
  }, [chartDimension, chartCode, data])

  async function handleExport() {
    if (!data || !analysisQuery) return
    const job = await exportMutation.mutateAsync({
      query: analysisQuery,
      filterSummary: data.filterSummary,
      projectionWatermark: data.freshness.sourceWatermark,
      permissionVersion: data.scope.permissionVersion,
      rowCount: data.customers.filteredTotal,
    })
    setExportJob(job)
  }

  function applyExplicitPeriod() {
    if (!explicitFrom || !explicitTo) return
    if (explicitFrom > explicitTo) return
    patchUrl({
      from: explicitFrom,
      to: explicitTo,
      periodSelectionSource: "EXPLICIT",
      periodPreset: null,
      customerQualityPeriodPolicyId: null,
      customerQualityPeriodPolicyVersion: null,
    })
  }

  function applyPreset(presetId: string, from: string, to: string) {
    patchUrl({
      from,
      to,
      periodPreset: presetId,
      periodSelectionSource: "CONFIGURED_PRESET",
      customerQualityPeriodPolicyId:
        periodPolicy?.customerQualityPeriodPolicyId ?? null,
      customerQualityPeriodPolicyVersion:
        periodPolicy?.customerQualityPeriodPolicyVersion != null
          ? String(periodPolicy.customerQualityPeriodPolicyVersion)
          : null,
    })
  }

  // —— Loading shells ——
  if (periodPolicyQuery.isPending || (!periodWriteDone && !needsPeriodBlocker)) {
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

  if (periodPolicyQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="system"
          title="期间配置加载失败"
          description="无法取得系统默认期间策略。请重试；不会静默采用自然年。"
          action={
            <Button
              type="button"
              onClick={() => void periodPolicyQuery.refetch()}
            >
              重试
            </Button>
          }
        />
      </div>
    )
  }

  // —— Period blocker ——
  if (needsPeriodBlocker) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="客户经营质量"
          description="未配置默认统计期间，请选择起止日期后开始分析。"
          breadcrumbs={[
            { id: "an", label: "分析", href: "/analytics/customer-quality" },
            { id: "cq", label: "客户经营质量", current: true },
          ]}
        />
        <Alert variant="warning">
          <CalendarRangeIcon aria-hidden="true" />
          <AlertTitle>请选择统计期间</AlertTitle>
          <AlertDescription>
            尚未设置默认统计期间。选定期间后才会显示指标、图表与明细。
          </AlertDescription>
        </Alert>
        <Card size="sm">
          <CardHeader className="border-b">
            <CardTitle>显式期间</CardTitle>
            <CardDescription>
              选定后作为本页所有统计的唯一期间。
            </CardDescription>
          </CardHeader>
          <CardContent className="flex flex-col gap-4 pt-4 sm:flex-row sm:items-end">
            <div className="grid flex-1 gap-2 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label htmlFor="cq-from">开始日期</Label>
                <DatePicker
                  value={explicitFrom || undefined}
                  onValueChange={(next) => setExplicitFrom(next ?? "")}
                />
              </div>
              <div className="space-y-1.5">
                <Label htmlFor="cq-to">结束日期</Label>
                <DatePicker
                  value={explicitTo || undefined}
                  onValueChange={(next) => setExplicitTo(next ?? "")}
                />
              </div>
            </div>
            <div className="flex flex-col gap-2">
              <Button
                type="button"
                disabled={
                  !explicitFrom ||
                  !explicitTo ||
                  (explicitFrom > explicitTo && explicitFrom !== explicitTo)
                }
                onClick={applyExplicitPeriod}
              >
                开始分析
              </Button>
              {explicitFrom && explicitTo && explicitFrom > explicitTo ? (
                <p className="text-xs text-destructive">
                  开始日期不能晚于结束日期，请调整后提交。
                </p>
              ) : null}
            </div>
          </CardContent>
          {periodPolicy?.presets && periodPolicy.presets.length > 0 ? (
            <CardContent className="border-t pt-4">
              <p className="mb-2 text-sm text-muted-foreground">
                或选择快捷期间：
              </p>
              <div className="flex flex-wrap gap-2">
                {periodPolicy.presets.map((p) => (
                  <Button
                    key={p.id}
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => applyPreset(p.id, p.from, p.to)}
                  >
                    {p.label}
                  </Button>
                ))}
              </div>
            </CardContent>
          ) : null}
        </Card>
      </div>
    )
  }

  if (viewQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="projection"
          title="经营质量数据加载失败"
          description="暂无可用结果。请重试或返回其它模块。"
          action={
            <Button type="button" onClick={() => void viewQuery.refetch()}>
              重试
            </Button>
          }
        />
      </div>
    )
  }

  if (viewQuery.isPending || !data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <Skeleton className="h-10 w-64" />
        <Skeleton className="h-28 w-full" />
        <div className="grid grid-cols-2 gap-2 md:grid-cols-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <Skeleton key={i} className="h-20 rounded-lg" />
          ))}
        </div>
        <Skeleton className="h-72 w-full" />
      </div>
    )
  }

  if (data.emptyKind === "forbidden") {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessFailureState
          kind="permission"
          title="无客户经营质量权限"
          description="当前账号缺少经营质量模块权限。敏感明细已不展示。"
          action={
            <Button type="button" variant="outline" render={<Link href="/workspace" />}>
              返回工作台
            </Button>
          }
        />
      </div>
    )
  }

  const freshUi = freshnessPresentation(
    data.freshness.state,
    data.freshness.refreshFailed,
    viewQuery.isFetching && !viewQuery.isPending
  )

  const isVoucherOnly = businessType === "VOUCHER"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="客户经营质量"
        breadcrumbs={[
          { id: "an", label: "分析", href: "/analytics/customer-quality" },
          { id: "cq", label: "客户经营质量", current: true },
        ]}
        metadata={
          <div className="flex flex-col gap-1">
            <DataFreshness
              updatedAt={formatClock(data.freshness.projectedAt)}
              dateTime={data.freshness.projectedAt}
              state={freshUi.state}
              statusLabel={freshUi.statusLabel}
              label="经营质量汇总"
            />
            <div className="flex flex-wrap gap-2 text-xs text-muted-foreground">
              <span>
                期间 {data.period.from} ~ {data.period.to}
                {data.period.selectionSource === "SERVER_DEFAULT"
                  ? " · 系统默认"
                  : data.period.selectionSource === "CONFIGURED_PRESET"
                    ? " · 配置快捷项"
                    : " · 显式选择"}
              </span>
              <span>· {data.scope.label}</span>
            </div>
          </div>
        }
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={viewQuery.isFetching}
              onClick={() => {
                void handleRefresh()
              }}
            >
              <RefreshCwIcon className="size-4" aria-hidden />
              {viewQuery.isFetching ? "刷新中" : "刷新"}
            </Button>
            <GuardedBusinessAction
              type="button"
              variant="outline"
              size="sm"
              disabled={
                !data.canExport ||
                data.customers.filteredTotal === 0 ||
                exportMutation.isPending
              }
              reason={
                !data.canExport
                  ? "当前角色无导出权限"
                  : data.customers.filteredTotal === 0
                    ? "当前没有客户结果可导出"
                    : exportMutation.isPending
                      ? "导出任务进行中"
                      : undefined
              }
              onClick={() => void handleExport()}
            >
              <DownloadIcon className="size-4" aria-hidden />
              导出
            </GuardedBusinessAction>
          </div>
        }
      />

      {/* Distinct freshness / coverage alerts — not mutually substitutable */}
      {refreshError ? (
        <Alert variant="destructive">
          <AlertTitle>刷新失败</AlertTitle>
          <AlertDescription>
            本次刷新未成功，已保留上次成功结果。请重试；业务记录未被修改。
          </AlertDescription>
        </Alert>
      ) : null}
      {data.freshness.state === "stale" && !data.freshness.refreshFailed ? (
        <Alert variant="warning">
          <AlertTitle>数据可能不是最新</AlertTitle>
          <AlertDescription>
            最近成功更新 {formatDateTime(data.freshness.projectedAt, "full", "passthrough")}；来源更新时间{" "}
            <span className="num">
              {formatSourceWatermark(data.freshness.sourceWatermark)}
            </span>
            。数据可能不是最新，可点击刷新。
          </AlertDescription>
        </Alert>
      ) : null}
      {data.freshness.state === "rebuilding" ? (
        <Alert variant="info">
          <AlertTitle>数据更新中</AlertTitle>
          <AlertDescription>
            更新中，已保留最近成功结果。
          </AlertDescription>
        </Alert>
      ) : null}
      {data.freshness.refreshFailed ? (
        <Alert variant="destructive">
          <AlertTitle>刷新失败</AlertTitle>
          <AlertDescription>
            已保留旧结果。请重试；业务记录未被修改。
          </AlertDescription>
        </Alert>
      ) : null}
      {data.freshness.state === "failed" ? (
        <Alert variant="destructive">
          <AlertTitle>数据加载失败</AlertTitle>
          <AlertDescription>
            显示上次成功数据（若有）。请查看后台任务或稍后重试。
          </AlertDescription>
        </Alert>
      ) : null}
      {viewQuery.isError ? (
        <Alert variant="destructive">
          <AlertTitle>数据更新失败</AlertTitle>
          <AlertDescription>
            已保留上次成功结果，未覆盖业务数据。请重试或调整筛选。
          </AlertDescription>
        </Alert>
      ) : null}

      {/* Filters */}
      <Card size="sm">
        <CardContent className="flex flex-col gap-3 pt-4">
          <div className="flex flex-wrap items-end gap-3">
            <div className="space-y-1.5">
              <Label htmlFor="cq-period-from">期间起</Label>
              <DatePicker
                className="w-[10.5rem]"
                value={resolvedFrom || undefined}
                onValueChange={(next) => {
                  patchUrl({
                    from: next || null,
                    periodSelectionSource: "EXPLICIT",
                    periodPreset: null,
                  })
                  resetPage()
                }}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="cq-period-to">期间止</Label>
              <DatePicker
                className="w-[10.5rem]"
                value={resolvedTo || undefined}
                onValueChange={(next) => {
                  patchUrl({
                    to: next || null,
                    periodSelectionSource: "EXPLICIT",
                    periodPreset: null,
                  })
                  resetPage()
                }}
              />
            </div>
            {periodInvalid ? (
              <p
                id="cq-period-invalid"
                className="w-full text-xs text-destructive"
                role="alert"
              >
                开始日期晚于结束日期，将查询不到结果，请调整。
              </p>
            ) : null}
            {periodPolicy?.presets?.length ? (
              <div className="space-y-1.5">
                <Label htmlFor="cq-preset">快捷期间</Label>
                <OptionCombobox
                  id="cq-preset"
                  value={periodPreset ?? ""}
                  onValueChange={(v) => {
                    const id = v ?? ""
                    const preset = periodPolicy.presets?.find((p) => p.id === id)
                    if (preset) {
                      applyPreset(preset.id, preset.from, preset.to)
                      resetPage()
                    } else patchUrl({ periodPreset: null })
                  }}
                  options={[
                    { value: "", label: "自定义" },
                    ...periodPolicy.presets.map((p) => ({
                      value: p.id,
                      label: p.label,
                    })),
                  ]}
                  className="w-40"
                  size="sm"
                  allowClear={false}
                  aria-label="快捷期间"
                  placeholder="自定义"
                />
              </div>
            ) : null}
            <div className="space-y-1.5">
              <Label htmlFor="cq-funds">票款口径</Label>
              <OptionCombobox
                id="cq-funds"
                value={fundsReview}
                onValueChange={(v) => {
                  patchUrl({
                    fundsReview:
                      (v ?? "all") === "reviewed_only"
                        ? "reviewed_only"
                        : null,
                  })
                  resetPage()
                }}
                options={[
                  { value: "all", label: "全部授权记录" },
                  { value: "reviewed_only", label: "仅已复核卡券票款" },
                ]}
                className="w-44"
                size="sm"
                allowClear={false}
                aria-label="票款口径"
                placeholder="票款口径"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="cq-nature">业务性质</Label>
              <OptionCombobox
                id="cq-nature"
                value={businessType ?? ""}
                onValueChange={(v) => {
                  patchUrl({
                    businessType: v || null,
                  })
                  resetPage()
                }}
                options={[
                  { value: "", label: "全部" },
                  { value: "VOUCHER", label: "卡券" },
                  { value: "GOODS_SERVICE", label: "非卡券" },
                ]}
                className="w-36"
                size="sm"
                allowClear={false}
                aria-label="业务性质"
                placeholder="全部"
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="cq-sort">排序</Label>
              <OptionCombobox
                id="cq-sort"
                value={sort}
                onValueChange={(v) => {
                  patchUrl({ sort: v ?? sort })
                  resetPage()
                }}
                options={[
                  { value: "salesGrossAmount:desc", label: "成交金额降序" },
                  {
                    value: "actualProfitLossNet:desc",
                    label: "实际盈亏降序",
                  },
                  { value: "overdueGross:desc", label: "逾期金额降序" },
                  { value: "costCoverageRate:asc", label: "覆盖率升序" },
                  { value: "latestBusinessAt:desc", label: "最近业务" },
                ]}
                className="w-44"
                size="sm"
                allowClear={false}
                aria-label="排序"
                placeholder="排序"
              />
            </div>
            <div className="min-w-[12rem] flex-1 space-y-1.5">
              <Label htmlFor="cq-q">搜索客户</Label>
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  id="cq-q"
                  value={searchInput}
                  placeholder="客户编号 / 名称"
                  onChange={(e) => setSearchInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      patchUrl({ q: searchInput.trim() || null })
                      setPagination((p) => ({ ...p, pageIndex: 0 }))
                    }
                  }}
                />
              </InputGroup>
            </div>
            <Button
              type="button"
              size="sm"
              onClick={() => {
                patchUrl({ q: searchInput.trim() || null })
                setPagination((p) => ({ ...p, pageIndex: 0 }))
              }}
            >
              应用
            </Button>
            {(qParam ||
              scaleTag ||
              profitTag ||
              riskTag ||
              chartCode ||
              businessType ||
              fundsReview === "reviewed_only" ||
              customerId) && (
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => {
                  setSearchInput("")
                  patchUrl({
                    q: null,
                    scaleTag: null,
                    profitTag: null,
                    riskTag: null,
                    chartDimension: null,
                    chartCode: null,
                    businessType: null,
                    fundsReview: null,
                    customerId: null,
                    sort: null,
                    focusMetric: null,
                  })
                  resetPage()
                }}
              >
                清除筛选
              </Button>
            )}
          </div>
          <p className="text-xs text-muted-foreground" aria-live="polite">
            当前口径：{data.filterSummary} · 明细{" "}
            {data.customers.filteredTotal}/{data.customers.total} 户
          </p>
        </CardContent>
      </Card>

      {data.emptyKind === "no-scope" ? (
        <BusinessEmptyState
          kind="no-scope"
          title="当前角色无客户数据范围"
          description="当前角色无客户数据范围，请申请权限。"
        />
      ) : (
        <>
          {/* Coverage: card funds + cost — always co-displayed with affected metrics */}
          <div className="grid min-w-0 gap-4 xl:grid-cols-2">
            <Card size="sm" data-slot="card-funds-coverage-notice">
              <CardHeader className="border-b">
                <CardTitle>卡券票款复核进度</CardTitle>
                <CardDescription>
                  与受影响应收指标同屏；未复核不得假装可靠。
                </CardDescription>
                <CardAction>
                  <Badge
                    variant={
                      data.coverage.cardFundsState === "complete"
                        ? "success"
                        : "warning"
                    }
                  >
                    {data.coverage.cardFundsReviewRate}
                  </Badge>
                </CardAction>
              </CardHeader>
              <CardContent className="space-y-3 pt-4">
                <p className="text-sm">
                  已复核{" "}
                  <span className="num font-medium">
                    {data.coverage.reviewedVoucherOrderCount}
                  </span>{" "}
                  / 应复核{" "}
                  <span className="num font-medium">
                    {data.coverage.requiredVoucherOrderCount}
                  </span>{" "}
                  张卡券销售单（
                  {data.coverage.cardFundsReviewRate}）
                </p>
                {data.coverage.cardFundsState !== "complete" ? (
                  <Alert variant="warning">
                    <AlertTitle>票款复核不足</AlertTitle>
                    <AlertDescription className="flex flex-wrap items-center gap-2">
                      应收余额、逾期金额等指标标记为部分可靠。可切换「仅已复核」或前往卡券票款复核。
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() =>
                          patchUrl({ fundsReview: "reviewed_only" })
                        }
                      >
                        仅看已复核
                      </Button>
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        render={
                          <Link href="/finance/card-funds-review?from=W15" />
                        }
                      >
                        {openWorkspaceLabel("W13")}
                      </Button>
                    </AlertDescription>
                  </Alert>
                ) : null}
              </CardContent>
            </Card>

            <CostCoverageNotice
              basis={data.coverage.costBasis}
              coveragePercent={data.coverage.costCoveragePercent}
              coverageLabel={data.coverage.costCoverageRate}
              coverageState={data.coverage.costCoverageState}
              breakdown={{
                ACTUAL: data.coverage.costCoveredNetRevenue,
                STANDARD: "—",
                NONE: data.coverage.costUncoveredNetRevenue,
              }}
              profitBasis="非卡券净收入 − 实际净成本（不含税）；卡券不计入"
              notice={
                <>
                  成本覆盖收入{" "}
                  <span className="num">
                    {data.coverage.costCoveredNetRevenue}
                  </span>
                  、未覆盖收入{" "}
                  <span className="num">
                    {data.coverage.costUncoveredNetRevenue}
                  </span>
                  、覆盖率{" "}
                  <span className="num">{data.coverage.costCoverageRate}</span>
                  。缺失成本不显示为 0，利润须与覆盖率同屏解读。
                </>
              }
            />
          </div>

          {isVoucherOnly ? (
            <Alert variant="info">
              <AlertTitle>业务性质：卡券</AlertTitle>
              <AlertDescription>
                卡券实际经营结果请前往卡券经营分析；本页不显示卡券实际盈亏，卡券收入仍计入规模与回款分析。
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  className="ml-2"
                  render={
                    <Link
                      href={`/analytics/card-business?from=${encodeURIComponent(data.period.from)}&to=${encodeURIComponent(data.period.to)}&source=W15`}
                    />
                  }
                >
                  {openWorkspaceLabel("W28")}
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {/* Metrics */}
          <MetricStrip columns={4} aria-label="客户经营质量核心指标">
            {data.metrics
              .filter((m) => m.visible)
              .map((m) => {
                const active = focusMetric === m.key
                const detail = metricReliabilityDetail(
                  m.reliability,
                  m.explanation,
                  m.fieldDenied
                )
                const valueNode =
                  m.fieldDenied || m.reliability === "unavailable" ? (
                    <span className="text-muted-foreground">
                      {m.fieldDenied ? "当前角色不可查看" : "暂无可靠口径"}
                    </span>
                  ) : (
                    m.value
                  )
                if (
                  m.key === "overdueGross" ||
                  m.key === "actualProfitLossNet" ||
                  m.key === "salesGrossAmount"
                ) {
                  return (
                    <MetricFilterItem
                      key={m.key}
                      label={m.label}
                      value={valueNode}
                      detail={detail}
                      active={active}
                      onClick={() => {
                        patchUrl({
                          focusMetric: active ? null : m.key,
                        })
                        if (!active) scrollToTableTop()
                      }}
                    />
                  )
                }
                return (
                  <MetricItem
                    key={m.key}
                    label={m.label}
                    value={valueNode}
                    detail={detail}
                    status={
                      m.reliability === "partial"
                        ? { label: "部分可靠", tone: "warning" }
                        : m.reliability === "unavailable"
                          ? { label: "不可用", tone: "neutral" }
                          : undefined
                    }
                  />
                )
              })}
          </MetricStrip>

          {/* Charts + equivalent tables */}
          <div className="grid min-w-0 gap-4 xl:grid-cols-2">
            <Card size="sm">
              <CardHeader className="border-b">
                <CardTitle>{scaleDimension?.title ?? "客户规模分层"}</CardTitle>
                <CardDescription>
                  点击柱形筛选明细
                  {scaleDimension?.ruleVersion
                    ? ` · 标签规则版本 v${scaleDimension.ruleVersion}`
                    : ""}
                  。柱形颜色仅作区分，具体数值见下方表格。
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4 pt-4">
                <ChartContainer
                  config={chartConfig}
                  className="aspect-[16/9] w-full"
                >
                  <BarChart
                    data={[...(scaleDimension?.items ?? [])].map((i) => ({
                      label: i.label,
                      code: i.code,
                      value: Number(String(i.value).replace(/[^\d.-]/g, "")) || 0,
                      raw: i.value,
                    }))}
                    accessibilityLayer
                  >
                    <CartesianGrid vertical={false} />
                    <XAxis dataKey="label" tickLine={false} axisLine={false} />
                    <YAxis tickLine={false} axisLine={false} width={48} />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Bar dataKey="value" radius={4}>
                      {(scaleDimension?.items ?? []).map((item) => (
                        <Cell
                          key={item.code}
                          cursor="pointer"
                          fill={
                            chartDimension === "scale" &&
                            chartCode === item.code
                              ? "var(--color-active)"
                              : "var(--color-value)"
                          }
                          onClick={() => {
                            const nextActive =
                              chartDimension === "scale" &&
                              chartCode === item.code
                            patchUrl({
                              chartDimension: nextActive ? null : "scale",
                              chartCode: nextActive ? null : item.code,
                              scaleTag: nextActive ? null : item.code,
                            })
                            setPagination((p) => ({ ...p, pageIndex: 0 }))
                          }}
                        />
                      ))}
                    </Bar>
                  </BarChart>
                </ChartContainer>
                <div className="overflow-x-auto">
                  <table className="w-full text-sm">
                    <caption className="sr-only">
                      客户规模分层等价数据表
                    </caption>
                    <thead>
                      <tr className="border-b text-left text-muted-foreground">
                        <th className="py-1.5 pr-3 font-medium">分层</th>
                        <th className="py-1.5 pr-3 font-medium">成交规模</th>
                        <th className="py-1.5 pr-3 font-medium">占比</th>
                        <th className="py-1.5 font-medium">户数</th>
                      </tr>
                    </thead>
                    <tbody>
                      {(scaleDimension?.items ?? []).map((item) => {
                        const selected =
                          chartDimension === "scale" && chartCode === item.code
                        return (
                          <tr
                            key={item.code}
                            className={
                              selected ? "bg-accent/60" : "border-b border-border/60"
                            }
                          >
                            <td className="py-1.5 pr-3">
                              <Button
                                type="button"
                                size="xs"
                                variant={selected ? "secondary" : "ghost"}
                                aria-pressed={selected}
                                onClick={() => {
                                  const nextActive = selected
                                  patchUrl({
                                    chartDimension: nextActive ? null : "scale",
                                    chartCode: nextActive ? null : item.code,
                                    scaleTag: nextActive ? null : item.code,
                                  })
                                  setPagination((p) => ({
                                    ...p,
                                    pageIndex: 0,
                                  }))
                                }}
                              >
                                {item.label}
                              </Button>
                            </td>
                            <td className="num py-1.5 pr-3">{item.value}</td>
                            <td className="num py-1.5 pr-3">
                              {item.share ?? "—"}
                            </td>
                            <td className="num py-1.5">{item.count ?? "—"}</td>
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>
              </CardContent>
            </Card>

            <Card size="sm">
              <CardHeader className="border-b">
                <CardTitle>
                  {profitDimension?.title ?? "利润贡献分布"}
                </CardTitle>
                <CardDescription>
                  仅成本完整非卡券；卡券收入不进入利润标签
                  {profitDimension?.ruleVersion
                    ? ` · 标签规则版本 v${profitDimension.ruleVersion}`
                    : ""}
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-4 pt-4">
                {isVoucherOnly ? (
                  <p className="text-sm text-muted-foreground">
                    当前为卡券业务性质筛选，利润贡献图隐藏。
                  </p>
                ) : (
                  <>
                    <ChartContainer
                      config={chartConfig}
                      className="aspect-[16/9] w-full"
                    >
                      <BarChart
                        data={[...(profitDimension?.items ?? [])].map((i) => ({
                          label: i.label,
                          code: i.code,
                          value:
                            Number(String(i.value).replace(/[^\d.-]/g, "")) || 0,
                        }))}
                        accessibilityLayer
                      >
                        <CartesianGrid vertical={false} />
                        <XAxis
                          dataKey="label"
                          tickLine={false}
                          axisLine={false}
                        />
                        <YAxis tickLine={false} axisLine={false} width={48} />
                        <ChartTooltip content={<ChartTooltipContent />} />
                        <Bar dataKey="value" radius={4}>
                          {(profitDimension?.items ?? []).map((item) => (
                            <Cell
                              key={item.code}
                              cursor="pointer"
                              fill={
                                chartDimension === "profit" &&
                                chartCode === item.code
                                  ? "var(--color-active)"
                                  : "var(--color-value)"
                              }
                              onClick={() => {
                                const nextActive =
                                  chartDimension === "profit" &&
                                  chartCode === item.code
                                patchUrl({
                                  chartDimension: nextActive ? null : "profit",
                                  chartCode: nextActive ? null : item.code,
                                  profitTag: nextActive ? null : item.code,
                                })
                                setPagination((p) => ({ ...p, pageIndex: 0 }))
                              }}
                            />
                          ))}
                        </Bar>
                      </BarChart>
                    </ChartContainer>
                    <div className="overflow-x-auto">
                      <table className="w-full text-sm">
                        <caption className="sr-only">
                          利润贡献分布等价数据表
                        </caption>
                        <thead>
                          <tr className="border-b text-left text-muted-foreground">
                            <th className="py-1.5 pr-3 font-medium">标签</th>
                            <th className="py-1.5 pr-3 font-medium">
                              盈亏（不含税）
                            </th>
                            <th className="py-1.5 pr-3 font-medium">占比</th>
                            <th className="py-1.5 font-medium">户数</th>
                          </tr>
                        </thead>
                        <tbody>
                          {(profitDimension?.items ?? []).map((item) => {
                            const selected =
                              chartDimension === "profit" &&
                              chartCode === item.code
                            return (
                              <tr
                                key={item.code}
                                className={
                                  selected
                                    ? "bg-accent/60"
                                    : "border-b border-border/60"
                                }
                              >
                                <td className="py-1.5 pr-3">
                                  <Button
                                    type="button"
                                    size="xs"
                                    variant={selected ? "secondary" : "ghost"}
                                    aria-pressed={selected}
                                    onClick={() => {
                                      const nextActive = selected
                                      patchUrl({
                                        chartDimension: nextActive
                                          ? null
                                          : "profit",
                                        chartCode: nextActive
                                          ? null
                                          : item.code,
                                        profitTag: nextActive
                                          ? null
                                          : item.code,
                                      })
                                      setPagination((p) => ({
                                        ...p,
                                        pageIndex: 0,
                                      }))
                                    }}
                                  >
                                    {item.label}
                                  </Button>
                                </td>
                                <td className="num py-1.5 pr-3">
                                  {item.value}
                                </td>
                                <td className="num py-1.5 pr-3">
                                  {item.share ?? "—"}
                                </td>
                                <td className="num py-1.5">
                                  {item.count ?? "—"}
                                </td>
                              </tr>
                            )
                          })}
                        </tbody>
                      </table>
                    </div>
                  </>
                )}
                {natureDimension ? (
                  <div className="border-t pt-3">
                    <p className="mb-2 text-sm font-medium">
                      {natureDimension.title}
                    </p>
                    <ul className="grid gap-1 text-sm sm:grid-cols-2">
                      {natureDimension.items.map((item) => (
                        <li
                          key={item.code}
                          className="flex justify-between gap-2 text-muted-foreground"
                        >
                          <span>
                            {item.label}
                            {item.code === "VOUCHER" ? (
                              <span className="ml-1 text-xs">
                                （计规模/回款，不计盈亏）
                              </span>
                            ) : null}
                          </span>
                          <span className="num">
                            {item.value} · {item.share}
                          </span>
                        </li>
                      ))}
                    </ul>
                  </div>
                ) : null}
              </CardContent>
            </Card>
          </div>

          {chartFilterSummary ? (
            <Alert variant="info">
              <AlertTitle>图表筛选已生效</AlertTitle>
              <AlertDescription>
                <span aria-live="polite">
                  {chartFilterSummary.dimensionTitle} ·{" "}
                  {chartFilterSummary.itemLabel} · 结果{" "}
                  <span className="num font-medium">
                    {chartFilterSummary.resultCount}
                  </span>{" "}
                  户
                </span>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  className="ml-2"
                  onClick={() => {
                    patchUrl({
                      chartDimension: null,
                      chartCode: null,
                      scaleTag: null,
                      profitTag: null,
                    })
                    resetPage()
                  }}
                >
                  清除图表筛选
                </Button>
              </AlertDescription>
            </Alert>
          ) : null}

          {/* Customer detail table */}
          {data.emptyKind === "no-data" ? (
            <BusinessEmptyState
              kind="no-data"
              title="期间内无授权经营记录"
              description="可调整统计期间或数据范围后重查。"
            />
          ) : data.emptyKind === "filter" ? (
            <BusinessEmptyState
              kind="filter"
              title="当前筛选无客户结果"
              description={
                <>
                  总体指标仍按当前期间与权限范围聚合（未因表格筛选归零）。筛选：
                  {data.filterSummary}
                </>
              }
              action={
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    patchUrl({
                      q: null,
                      scaleTag: null,
                      profitTag: null,
                      riskTag: null,
                      chartDimension: null,
                      chartCode: null,
                      businessType: null,
                      fundsReview: null,
                      customerId: null,
                      sort: null,
                      focusMetric: null,
                    })
                    resetPage()
                  }}
                >
                  清除筛选
                </Button>
              }
            />
          ) : (
            <div
              ref={tableSectionRef}
              tabIndex={-1}
              data-slot="customer-detail-table"
              className="focus:outline-none"
            >
              <BusinessTableFrame
                title="客户明细"
                description="点击客户进入客户中心；逾期与实际盈亏可分别下钻。金额口径与指标、图表、导出一致。"
                table={
                  <DataTable
                    data={[...data.customers.items]}
                    columns={columns}
                    getRowId={(row) => row.customerId}
                    rowCount={data.customers.filteredTotal}
                    pagination={pagination}
                    onPaginationChange={setPagination}
                    sorting={tableSorting}
                    onSortingChange={handleTableSortingChange}
                    layout="flush"
                    density="compact"
                  />
                }
              />
            </div>
          )}
        </>
      )}

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
            exportJob.status === "succeeded" ? exportJob.total : undefined
          }
          label="客户经营质量导出"
          description={
            <>
              期间 {exportJob.period.from} ~ {exportJob.period.to}。
              {exportJob.filterSummary}。数据更新时间{" "}
              <span className="num">
                {formatSourceWatermark(exportJob.projectionWatermark)}
              </span>
              。{exportJob.amountBasisNote}
              {exportJob.downloadLabel ? (
                <span className="mt-1 block font-medium">
                  可下载（保留 7 天）：{exportJob.downloadLabel}
                  {exportJob.expiresAt
                    ? ` · 失效 ${formatDateTime(exportJob.expiresAt, "full", "passthrough")}`
                    : ""}
                </span>
              ) : null}
            </>
          }
        />
      ) : null}

      <Dialog
        open={tagDialog != null}
        onOpenChange={(open) => {
          if (!open) setTagDialog(null)
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <InfoIcon className="size-4" aria-hidden="true" />
              经营标签说明
            </DialogTitle>
            <DialogDescription>
              标签由系统固定规则生成，页面不提供人工修改入口。
            </DialogDescription>
          </DialogHeader>
          {tagDialog ? (
            <div className="space-y-3 text-sm">
              <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                  context="list"
                  label={tagDialog.label}
                  tone={tagDialog.tone}
                />
                <Badge variant="outline">
                  规则版本 {tagDialog.ruleVersion}
                </Badge>
                <Badge variant="neutral">
                  {tagDialog.type === "scale"
                    ? "规模"
                    : tagDialog.type === "profit"
                      ? "利润贡献"
                      : "回款风险"}
                </Badge>
              </div>
              <p className="text-muted-foreground">{tagDialog.explanation}</p>
              {tagDialog.type === "profit" ? (
                <p className="text-xs text-muted-foreground">
                  卡券收入进入规模和回款分析，但不进入利润贡献标签与实际盈亏。
                </p>
              ) : null}
            </div>
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  )
}
