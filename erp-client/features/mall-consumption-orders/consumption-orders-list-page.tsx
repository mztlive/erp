"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type {
  ColumnDef,
  ColumnPinningState,
  PaginationState,
} from "@tanstack/react-table"
import {
  DownloadIcon,
  RefreshCwIcon,
  SearchIcon,
} from "lucide-react"

import {
  BackgroundJobProgress,
  BatchImpactPreview,
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  MultiOptionCombobox,
  OptionCombobox,
  PageActions,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
  surfacePanelClassName,
} from "@/components/business"
import { MallSearchCombobox } from "@/features/entity-selectors"
import { cn } from "@/lib/utils"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DateRangePicker } from "@/components/ui/date-picker"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import {
  useConsumptionOrderDetailQuery,
  useConsumptionOrderExportMutation,
  useConsumptionOrderListQuery,
} from "@/features/mall-consumption-orders/queries"
import { ConsumptionOrderPreviewPanel } from "@/features/mall-consumption-orders/consumption-order-preview-panel"
import type {
  AttributionStatus,
  CostBasis,
  FactType,
  FulfillmentChain,
  MallConsumptionOrderListQuery,
  MallConsumptionOrderMetricKey,
  MallConsumptionOrderRow,
  MallConsumptionOrderView,
  PaymentSourceFilter,
  SupplierFulfillmentStatus,
} from "@/features/mall-consumption-orders/types"
import {
  ATTRIBUTION_STATUS_LABEL,
  ATTRIBUTION_STATUS_TONE,
  COST_BASIS_LABEL,
  COST_BASIS_TONE,
  DATA_SOURCE_LABEL,
  FACT_TYPE_LABEL,
  FULFILLMENT_CHAIN_LABEL,
  FULFILLMENT_CHAIN_TONE,
  SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import { formatDateTime } from "@/lib/datetime"

function parseMetric(
  raw: string | null
): MallConsumptionOrderMetricKey | "all" {
  if (
    raw === "paid" ||
    raw === "pending_attr" ||
    raw === "fact_diff" ||
    raw === "auto_exception" ||
    raw === "cost_none"
  ) {
    return raw
  }
  return "all"
}

/** 逗号分隔多值 URL 参数 → 白名单过滤后的数组；非法值忽略。 */
function parseMultiValue<T extends string>(
  raw: string | null,
  allowed: readonly T[]
): T[] {
  if (!raw) return []
  const set = new Set<string>(allowed)
  return raw
    .split(",")
    .map((v) => v.trim())
    .filter((v): v is T => v !== "" && set.has(v))
}

const FACT_TYPES = Object.keys(FACT_TYPE_LABEL) as FactType[]
const SUPPLIER_STATUSES = Object.keys(
  SUPPLIER_STATUS_LABEL
) as SupplierFulfillmentStatus[]
const DATA_SOURCES = ["REALTIME", "BACKFILL"] as const

function paymentCompositionLabel(row: MallConsumptionOrderRow) {
  const { cardAmount, wechatAmount, sourceCount } = row.paymentComposition
  const card = Number(cardAmount) > 0
  const wx = Number(wechatAmount) > 0
  if (card && wx) {
    return `组合 · 卡券 ¥${cardAmount} / 微信 ¥${wechatAmount}`
  }
  if (card) return `卡券 ¥${cardAmount}`
  if (wx) return `微信 ¥${wechatAmount}`
  return `${sourceCount} 来源`
}

function factSummaryLabel(row: MallConsumptionOrderRow) {
  return row.factSummary
    .map(
      (f) =>
        `${FACT_TYPE_LABEL[f.factType]}${f.count > 1 ? `×${f.count}` : ""}`
    )
    .join(" · ")
}

function costBasisLabel(row: MallConsumptionOrderRow) {
  return row.costBasisBreakdown
    .map((b) => {
      const basisLabel = COST_BASIS_LABEL[b.basis] ?? b.basis
      return `${basisLabel}${b.lineCount > 1 ? `×${b.lineCount}` : ""}`
    })
    .join(" / ")
}

function supplierSummaryLabel(row: MallConsumptionOrderRow) {
  const s = row.supplierOrderSummary
  if (s.total === 0) {
    if (row.fulfillmentChain === "LEGACY_MANUAL") return "原人工 · 无子订单"
    return "尚未生成子订单"
  }
  const statusText = s.statuses
    .map((st) => SUPPLIER_STATUS_LABEL[st as SupplierFulfillmentStatus] ?? st)
    .join("/")
  return `${s.total} 单 · ${statusText}${s.hasException ? " · 异常" : ""}`
}

function previewDataSourceLabel(view: MallConsumptionOrderView): string {
  if (view.facts.length === 0) return "—"
  const kinds = Array.from(new Set(view.facts.map((f) => f.dataSource)))
  if (kinds.length === 1) return DATA_SOURCE_LABEL[kinds[0]]
  return DATA_SOURCE_LABEL.MIXED
}

export function ConsumptionOrdersListPage() {
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
    SUPPLIER_STATUSES
  )
  const dataSources = parseMultiValue(searchParams.get("dataSource"), DATA_SOURCES)
  const periodSelected = Boolean(occurredFrom && occurredTo)
  const metric = parseMetric(searchParams.get("metric"))
  const previewId = searchParams.get("preview")
  const pageFromUrl = Math.max(1, Number(searchParams.get("page") ?? "1") || 1)

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const sizeFromUrl = Math.max(
    1,
    Math.min(50, Number(searchParams.get("size") ?? "8") || 8)
  )
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: pageFromUrl - 1,
    pageSize: sizeFromUrl,
  })
  const [columnPinning] = React.useState<ColumnPinningState>({
    left: ["mallOrder"],
    right: ["actions"],
  })
  const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)
  const [exportResult, setExportResult] = React.useState<{
    jobId: string
    rowCount: number
    permissionVersion: string
    maskDisclaimer: string
    downloadLabel: string
    expiresAt: string
  } | null>(null)

  const exportMutation = useConsumptionOrderExportMutation()

  React.useEffect(() => {
    // URL is source of truth for search draft；输入中不被 URL 旧值覆盖（焦点保护）
    const el = searchInputRef.current
    if (el && document.activeElement === el) return
    setSearchInput(qParam)
  }, [qParam])

  // P3：搜索 300ms 防抖自动写 URL（replace），Enter 兜底，`/` 聚焦
  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchInput.trim() === qParam) return
      replaceParams({ q: searchInput.trim() || undefined })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- replaceParams 以当前 URL 快照为准
  }, [searchInput])

  React.useEffect(() => {
    setPagination((p) =>
      p.pageIndex === pageFromUrl - 1 ? p : { ...p, pageIndex: pageFromUrl - 1 }
    )
  }, [pageFromUrl])

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
      costBases: costBasis === "all" ? undefined : [costBasis as CostBasis],
      dataSources: dataSources.length
        ? dataSources
        : undefined,
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
    ]
  )

  const listQuery = useConsumptionOrderListQuery(listQueryInput, {
    enabled: periodSelected,
  })
  const data = listQuery.data
  const rows = data?.rows ?? []
  const metrics = data?.metrics ?? []
  const metricValue = (key: MallConsumptionOrderMetricKey) =>
    metrics.find((m) => m.key === key)?.value ?? "—"

  const previewQuery = useConsumptionOrderDetailQuery(previewId)

  const replaceParams = React.useCallback(
    (patch: Record<string, string | undefined>, resetPage = true) => {
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
    [pathname, router, searchParams]
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
    [pathname, router, searchParams]
  )

  const openPreview = React.useCallback(
    (mallOrderId: string) => {
      replaceParams({ preview: mallOrderId }, false)
    },
    [replaceParams]
  )

  const closePreview = React.useCallback(() => {
    replaceParams({ preview: undefined }, false)
  }, [replaceParams])

  const commitSearch = () => {
    replaceParams({ q: searchInput.trim() || undefined })
  }

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
      metric !== "all"
  )

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

  const confirmExport = async () => {
    const requestId = `req-w25-export-${Date.now()}`
    const result = await exportMutation.mutateAsync({
      selectionSnapshotId: `snap-${requestId}`,
      fieldSetId: "w25-list-default-masked",
      requestId,
      rowCount: data?.pageInfo.total ?? 0,
      filterSummary: data?.filterSummary ?? "",
    })
    setExportResult({
      jobId: result.jobId,
      rowCount: result.rowCount,
      permissionVersion: result.permissionVersion,
      maskDisclaimer: result.maskDisclaimer,
      downloadLabel: result.downloadLabel,
      expiresAt: result.expiresAt,
    })
    setExportPreviewOpen(false)
  }

  const listReturnHref = React.useMemo(() => {
    const qs = searchParams.toString()
    return qs ? `${pathname}?${qs}` : pathname
  }, [pathname, searchParams])

  const columns = React.useMemo<ColumnDef<MallConsumptionOrderRow>[]>(
    () => [
      {
        id: "mallOrder",
        header: "商城订单",
        meta: { label: "商城订单", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-[11rem] max-w-[14rem]">
            <div className="truncate text-sm font-medium">
              <span className="num">{row.original.externalOrderNo}</span>
            </div>
            <div className="truncate text-xs text-muted-foreground">
              <span className="num">{row.original.mallOrderId}</span>
              <span className="mx-1">·</span>
              {row.original.mallName}
            </div>
          </div>
        ),
      },
      {
        id: "customer",
        header: "客户",
        meta: { label: "客户", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">{row.original.customerLabel}</span>
        ),
      },
      {
        id: "paidAt",
        header: "支付时间",
        meta: { label: "支付时间", width: "default", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm text-muted-foreground">
            {formatDateTime(row.original.paidAt, "monthDay", "passthrough")}
          </span>
        ),
      },
      {
        id: "paidAmount",
        header: "实付",
        meta: {
          label: "实付",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <MoneyValue value={row.original.paidAmount} taxBasis="gross" />
        ),
      },
      {
        id: "paymentComposition",
        header: "支付构成",
        meta: { label: "支付构成", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">
            {paymentCompositionLabel(row.original)}
          </span>
        ),
      },
      {
        id: "facts",
        header: "关键记录",
        meta: { label: "关键记录", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {factSummaryLabel(row.original)}
          </span>
        ),
      },
      {
        id: "fulfillmentChain",
        header: "履约链",
        meta: { label: "履约链", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={FULFILLMENT_CHAIN_LABEL[row.original.fulfillmentChain]}
            tone={FULFILLMENT_CHAIN_TONE[row.original.fulfillmentChain]}
          />
        ),
      },
      {
        id: "supplier",
        header: "供应商订单摘要",
        meta: { label: "供应商订单摘要", width: "default" },
        cell: ({ row }) => {
          const label = supplierSummaryLabel(row.original)
          if (row.original.supplierOrderSummary.total > 0) {
            return (
              <Link
                href={`/supplier-api/orders?q=${encodeURIComponent(row.original.externalOrderNo)}&view=all&from=W25&mallOrderId=${encodeURIComponent(row.original.mallOrderId)}&returnTo=${encodeURIComponent(listReturnHref)}`}
                className="text-sm text-primary underline-offset-2 hover:underline"
                aria-label={`查看供应商子订单 ${label}`}
              >
                {label}
              </Link>
            )
          }
          return (
            <span
              className={
                row.original.supplierOrderSummary.hasException
                  ? "text-sm text-destructive"
                  : "text-sm text-muted-foreground"
              }
            >
              {label}
            </span>
          )
        },
      },
      {
        id: "attribution",
        header: "归集",
        meta: { label: "归集", width: "status" },
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={ATTRIBUTION_STATUS_LABEL[row.original.attributionStatus]}
            tone={ATTRIBUTION_STATUS_TONE[row.original.attributionStatus]}
          />
        ),
      },
      {
        id: "costBasis",
        header: "成本口径",
        meta: { label: "成本口径", width: "default" },
        cell: ({ row }) => {
          const primary = row.original.costBasisBreakdown[0]
          return (
            <div className="flex flex-col gap-0.5">
              {primary ? (
                <BusinessStatusBadge
                  context="list"
                  label={COST_BASIS_LABEL[primary.basis]}
                  tone={COST_BASIS_TONE[primary.basis]}
                />
              ) : null}
              <span className="text-xs text-muted-foreground">
                {costBasisLabel(row.original)}
              </span>
            </div>
          )
        },
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            <Button
              type="button"
              variant="outline"
              size="xs"
              render={
                <Link
                  href={`/commerce/consumption-orders/${row.original.mallOrderId}?section=overview&returnTo=${encodeURIComponent(listReturnHref)}`}
                />
              }
            >
              打开中心
            </Button>
            {row.original.allowedActions.includes("OPEN_W29") &&
            row.original.workItemId ? (
              <Button
                type="button"
                variant="ghost"
                size="xs"
                render={
                  <Link
                    href={`/governance/integration-errors?resolveWorkItemId=${row.original.workItemId}&queueContextId=queue:W29:mine:all`}
                  />
                }
              >
                接口错误
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    [listReturnHref]
  )

  const empty = data?.emptyReason

  return (
    <PageScaffold density="compact">
      <PageHeader
        title="商城消费订单"
        breadcrumbs={[
          {
            id: "com",
            label: "商城与发布",
            href: "/commerce/consumption-orders",
          },
          { id: "co", label: "商城消费订单", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={data ? formatDateTime(data.factWatermark, "monthDay", "passthrough") : "—"}
            dateTime={data?.factWatermark}
            state={listQuery.isFetching ? "syncing" : "fresh"}
            label="记录更新"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: "刷新",
                icon: RefreshCwIcon,
                variant: "ghost",
                onClick: () => {
                  void listQuery.refetch()
                },
              },
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled:
                  !data ||
                  data.pageInfo.total === 0 ||
                  exportMutation.isPending ||
                  empty === "NO_PERMISSION" ||
                  empty === "NO_SCOPE",
                onClick: () => setExportPreviewOpen(true),
              },
            ]}
          />
        }
      />

      <Alert
        variant="info"
        className="gap-2 py-2 lg:grid-cols-[auto_minmax(0,1fr)_auto] lg:items-center lg:gap-3"
      >
        <AlertTitle className="whitespace-nowrap">只读记录追溯</AlertTitle>
        <AlertDescription className="min-w-0 lg:truncate">
          {data?.boundaryNotice ??
            "本页只读：不修改支付状态、不编辑分摊、不重试供应商动作；导出与信息揭示均有审计。"}
        </AlertDescription>
      </Alert>

      {exportResult ? (
        <div className="space-y-2">
          <FormalActionResult
            status="succeeded"
            title="导出任务已创建"
            description={exportResult.maskDisclaimer}
            reference={exportResult.jobId}
            facts={[
              { label: "行数", value: String(exportResult.rowCount) },
              {
                label: "文件",
                value: exportResult.downloadLabel,
              },
              {
                label: "到期",
                value: formatDateTime(exportResult.expiresAt, "monthDay", "passthrough"),
              },
            ]}
          />
          <BackgroundJobProgress
            mode="all-or-nothing"
            status="succeeded"
            label="导出作业"
            description={`筛选结果 · 字段打码 · ${exportResult.jobId}`}
            total={exportResult.rowCount}
            completed={exportResult.rowCount}
            succeeded={exportResult.rowCount}
          />
        </div>
      ) : null}

      {exportPreviewOpen ? (
        <div className={cn(surfacePanelClassName, "space-y-3 p-4")}>
          <BatchImpactPreview
            title="导出当前筛选全部"
            description="按当前筛选结果导出，不限于当前页；下载时将重新校验权限。"
            filterSummary={data?.filterSummary ?? "—"}
            selectionScope="当前筛选全部"
            estimated={data?.pageInfo.total ?? 0}
            processable={data?.pageInfo.total ?? 0}
            skipped={0}
            background
            sensitiveFields={[
              "收货地址",
              "手机号",
              "完整支付流水号",
              "卡号/卡密（永不导出）",
              "未授权成本金额",
            ]}
            skippedReason="无权限字段以打码形式导出"
          />
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              disabled={exportMutation.isPending}
              onClick={() => {
                void confirmExport()
              }}
            >
              确认导出
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => setExportPreviewOpen(false)}
            >
              取消
            </Button>
          </div>
        </div>
      ) : null}

      {empty === "NO_PERMISSION" ? (
        <BusinessEmptyState
          kind="no-scope"
          title="无模块权限"
          description="当前角色无权访问商城消费订单。不显示无权限范围的指标。"
        />
      ) : empty === "NO_SCOPE" ? (
        <BusinessEmptyState
          kind="no-scope"
          title="无数据范围"
          description="你可进入此页面，但授权商城/客户范围内没有可查看消费订单。不显示无权限范围的指标。"
        />
      ) : (
        <>
          {/* 指标与普通筛选 AND 共存：指标点击不清理其它筛选（避免隐藏行为）；
              矛盾组合无结果时由「当前筛选无结果」空态解释并引导清除。 */}
          <MetricStrip columns={5} aria-label="消费订单指标筛选">
            <MetricFilterItem
              label="支付成功"
              value={metricValue("paid")}
              active={metric === "paid"}
              disabled={!periodSelected}
              title={periodSelected ? undefined : "选择期间后可筛选"}
              onClick={() =>
                replaceParams({
                  metric: metric === "paid" ? undefined : "paid",
                })
              }
            />
            <MetricFilterItem
              label="待归集"
              value={metricValue("pending_attr")}
              active={metric === "pending_attr"}
              disabled={!periodSelected}
              title={periodSelected ? undefined : "选择期间后可筛选"}
              onClick={() =>
                replaceParams({
                  metric:
                    metric === "pending_attr" ? undefined : "pending_attr",
                })
              }
            />
            <MetricFilterItem
              label="记录差异"
              value={metricValue("fact_diff")}
              active={metric === "fact_diff"}
              disabled={!periodSelected}
              title={periodSelected ? undefined : "选择期间后可筛选"}
              onClick={() =>
                replaceParams({
                  metric: metric === "fact_diff" ? undefined : "fact_diff",
                })
              }
            />
            <MetricFilterItem
              label="自动履约异常"
              value={metricValue("auto_exception")}
              active={metric === "auto_exception"}
              disabled={!periodSelected}
              title={periodSelected ? undefined : "选择期间后可筛选"}
              onClick={() =>
                replaceParams({
                  metric:
                    metric === "auto_exception"
                      ? undefined
                      : "auto_exception",
                })
              }
            />
            <MetricFilterItem
              label="成本未覆盖"
              value={metricValue("cost_none")}
              active={metric === "cost_none"}
              disabled={!periodSelected}
              title={periodSelected ? undefined : "选择期间后可筛选"}
              onClick={() =>
                replaceParams({
                  metric: metric === "cost_none" ? undefined : "cost_none",
                })
              }
            />
          </MetricStrip>
          {!periodSelected ? (
            <p className="text-xs text-muted-foreground">
              选择记录发生起止时间后，可点击指标快捷筛选。
            </p>
          ) : null}

          <BusinessTableFrame
            title="消费订单列表"
            description="商城订单与操作列固定；金额为人民币含税实付。Enter 打开预览抽屉。"
            toolbar={
              <>
                <ListToolbar
                  search={
                    <InputGroup className="w-full">
                      <InputGroupAddon>
                        <SearchIcon className="size-4" />
                      </InputGroupAddon>
                      <InputGroupInput
                        ref={searchInputRef}
                        value={searchInput}
                        onChange={(e) => setSearchInput(e.target.value)}
                        onKeyDown={(e) => {
                          if (e.key === "Enter") commitSearch()
                        }}
                        placeholder="商城单号、客户、ERP 编号"
                        aria-label="搜索消费订单"
                      />
                    </InputGroup>
                  }
                  filters={
                    <>
                      <MallSearchCombobox
                        value={mallId === "all" ? null : mallId}
                        onValueChange={(v) =>
                          replaceParams({ mall: v || "all" })
                        }
                        className="w-44"
                        size="sm"
                        allowClear={false}
                        aria-label="来源商城"
                        placeholder="全部商城"
                      />
                      <DateRangePicker
                        value={
                          occurredFrom || occurredTo
                            ? {
                                from: occurredFrom || undefined,
                                to: occurredTo || undefined,
                              }
                            : undefined
                        }
                        onValueChange={(range) =>
                          replaceParams({
                            occurredFrom: range?.from || undefined,
                            occurredTo: range?.to || undefined,
                          })
                        }
                        placeholder="记录发生时间"
                        className="w-56"
                      />
                      <OptionCombobox
                        value={attributionStatus}
                        onValueChange={(v) =>
                          replaceParams({
                            attributionStatus: v || undefined,
                          })
                        }
                        options={[
                          { value: "all", label: "归集" },
                          { value: "ATTRIBUTED", label: "已归集" },
                          { value: "PENDING", label: "待归集" },
                          { value: "DIFFERENCE", label: "差异" },
                        ]}
                        className="w-32"
                        size="sm"
                        allowClear={false}
                        aria-label="归集状态"
                        placeholder="归集"
                      />
                    </>
                  }
                  secondary={
                    <>
                      <OptionCombobox
                        value={fulfillmentChain}
                        onValueChange={(v) =>
                          replaceParams({
                            fulfillmentChain: v || undefined,
                          })
                        }
                        options={[
                          { value: "all", label: "履约链" },
                          { value: "LEGACY_MANUAL", label: "原人工" },
                          { value: "ERP_AUTOMATED", label: "ERP 自动" },
                        ]}
                        className="w-36"
                        size="sm"
                        allowClear={false}
                        aria-label="履约链"
                        placeholder="履约链"
                      />
                      <MultiOptionCombobox
                        value={factTypes}
                        onValueChange={(v) =>
                          replaceParams({
                            factType: v.length ? v.join(",") : undefined,
                          })
                        }
                        options={FACT_TYPES.map((t) => ({
                          value: t,
                          label: FACT_TYPE_LABEL[t],
                        }))}
                        className="w-40"
                        size="sm"
                        aria-label="事实类型"
                        placeholder="事实类型"
                      />
                      <MultiOptionCombobox
                        value={supplierStatuses}
                        onValueChange={(v) =>
                          replaceParams({
                            supplierStatus: v.length ? v.join(",") : undefined,
                          })
                        }
                        options={SUPPLIER_STATUSES.map((s) => ({
                          value: s,
                          label: SUPPLIER_STATUS_LABEL[s],
                        }))}
                        className="w-40"
                        size="sm"
                        aria-label="供应商状态"
                        placeholder="供应商状态"
                      />
                      <MultiOptionCombobox
                        value={dataSources}
                        onValueChange={(v) =>
                          replaceParams({
                            dataSource: v.length ? v.join(",") : undefined,
                          })
                        }
                        options={DATA_SOURCES.map((d) => ({
                          value: d,
                          label: DATA_SOURCE_LABEL[d],
                        }))}
                        className="w-32"
                        size="sm"
                        aria-label="数据来源"
                        placeholder="数据来源"
                      />
                      <OptionCombobox
                        value={paymentSource}
                        onValueChange={(v) =>
                          replaceParams({
                            paymentSource: v || undefined,
                          })
                        }
                        options={[
                          { value: "all", label: "支付方式" },
                          { value: "CARD", label: "卡券" },
                          { value: "WECHAT", label: "微信" },
                          { value: "MIXED", label: "组合" },
                        ]}
                        className="w-32"
                        size="sm"
                        allowClear={false}
                        aria-label="支付方式"
                        placeholder="支付方式"
                      />
                      <OptionCombobox
                        value={costBasis}
                        onValueChange={(v) =>
                          replaceParams({
                            costBasis: v || undefined,
                          })
                        }
                        options={[
                          { value: "all", label: "成本口径" },
                          { value: "ACTUAL", label: COST_BASIS_LABEL.ACTUAL },
                          { value: "STANDARD", label: COST_BASIS_LABEL.STANDARD },
                          { value: "NONE", label: COST_BASIS_LABEL.NONE },
                        ]}
                        className="w-32"
                        size="sm"
                        allowClear={false}
                        aria-label="成本口径"
                        placeholder="成本口径"
                      />
                    </>
                  }
                  actions={
                    hasActiveFilters ? (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={clearFilters}
                      >
                        清除筛选
                      </Button>
                    ) : null
                  }
                />

                {searchInput.trim() !== qParam ? (
                  <p className="text-xs text-muted-foreground" aria-live="polite">
                    搜索框内容尚未应用，稍候将自动生效；回车可立即搜索。
                  </p>
                ) : null}

                {data?.filterSummary ? (
                  <p className="text-sm text-muted-foreground" aria-live="polite">
                    筛选摘要：{data.filterSummary}
                  </p>
                ) : null}
              </>
            }
            table={
              !periodSelected ? (
                <BusinessEmptyState
                  kind="filter"
                  title="请选择记录发生起止时间"
                  description="默认期间策略未配置：请选择完整的事实发生起止时间后再查询，不静默拉取全量记录。"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                />
              ) : listQuery.isPending ? (
                <div
                  className="h-64 animate-pulse rounded-lg bg-muted"
                  aria-busy
                />
              ) : listQuery.isError ? (
                <BusinessFailureState
                  title="查询失败"
                  error={listQuery.error}
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  action={
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      className="rounded-lg shadow-none"
                      onClick={() => void listQuery.refetch()}
                    >
                      重试
                    </Button>
                  }
                />
              ) : empty === "FILTER_EMPTY" ? (
                <BusinessEmptyState
                  kind="filter"
                  title="当前筛选无结果"
                  description="可调整期间、商城、履约链、归集、支付方式、成本口径、记录类型、供应商状态、数据来源或搜索条件后重试。"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                  action={
                    <Button
                      type="button"
                      variant="secondary"
                      size="sm"
                      className="rounded-lg shadow-none"
                      onClick={clearFilters}
                    >
                      清除筛选
                    </Button>
                  }
                />
              ) : empty === "NO_DATA" || rows.length === 0 ? (
                <BusinessEmptyState
                  kind="no-data"
                  title="当前范围没有消费订单"
                  description="新支付记录到达后会自动显示。"
                  className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                />
              ) : (
                <DataTable
                  data={rows}
                  columns={columns}
                  getRowId={(row) => row.mallOrderId}
                  density="compact"
                  layout="flush"
                  enableColumnPinning
                  columnPinning={columnPinning}
                  pagination={pagination}
                  onPaginationChange={handlePaginationChange}
                  rowCount={data?.pageInfo.total ?? 0}
                  manualPagination
                  loading={listQuery.isFetching}
                  onRowPreview={(row) => openPreview(row.mallOrderId)}
                  onRowOpen={(row) => openPreview(row.mallOrderId)}
                  showPagination
                  pageSizeOptions={[8, 10, 20]}
                />
              )
            }
          />

          <div className="flex flex-wrap gap-2">
            <Badge variant="secondary">仅支持卡券与微信两种支付来源</Badge>
            <Badge variant="outline">无福利账户支付</Badge>
            <Badge variant="outline">
              列表 {data?.pageInfo.total ?? 0} 条 · 每页 {pagination.pageSize} 条
            </Badge>
          </div>
        </>
      )}

      <QuickPreviewSheet
        open={Boolean(previewId)}
        onOpenChange={(open) => {
          if (!open) closePreview()
        }}
        size="detail"
        title={previewQuery.data?.identity.externalOrderNo ?? "商城消费订单预览"}
        identity={
          previewQuery.data ? (
            <span className="num">
              {previewQuery.data.identity.mallOrderId}
              <span className="mx-1">·</span>
              {previewQuery.data.identity.mallName}
            </span>
          ) : null
        }
        summary={
          previewQuery.data ? (
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="preview"
                label={
                  FULFILLMENT_CHAIN_LABEL[previewQuery.data.fulfillment.chain]
                }
                tone={
                  FULFILLMENT_CHAIN_TONE[previewQuery.data.fulfillment.chain]
                }
              />
              <BusinessStatusBadge
                context="preview"
                label={
                  ATTRIBUTION_STATUS_LABEL[
                    previewQuery.data.customer.attributionStatus
                  ]
                }
                tone={
                  ATTRIBUTION_STATUS_TONE[
                    previewQuery.data.customer.attributionStatus
                  ]
                }
              />
              <Badge variant="secondary">
                {previewDataSourceLabel(previewQuery.data)}
              </Badge>
            </div>
          ) : null
        }
        footer={
          previewQuery.data ? (
            <>
              <Button type="button" variant="outline" onClick={closePreview}>
                关闭
              </Button>
              <Button
                type="button"
                variant="outline"
                render={
                  <Link
                    href={`/commerce/consumption-orders/${previewQuery.data.identity.mallOrderId}?section=overview&returnTo=${encodeURIComponent(listReturnHref)}`}
                  />
                }
              >
                打开中心
              </Button>
            </>
          ) : null
        }
      >
        {previewQuery.isPending ? (
          <div className="p-5 text-sm text-muted-foreground">加载预览…</div>
        ) : previewQuery.data ? (
          <ConsumptionOrderPreviewPanel view={previewQuery.data} />
        ) : (
          <div className="p-5 text-sm text-muted-foreground">
            未找到该消费订单
          </div>
        )}
      </QuickPreviewSheet>
    </PageScaffold>
  )
}
