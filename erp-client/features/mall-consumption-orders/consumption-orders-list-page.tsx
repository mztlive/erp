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
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import {
  useConsumptionOrderExportMutation,
  useConsumptionOrderListQuery,
} from "@/features/mall-consumption-orders/queries"
import type {
  AttributionStatus,
  CostBasis,
  FulfillmentChain,
  ListDemoFlag,
  MallConsumptionOrderListQuery,
  MallConsumptionOrderMetricKey,
  MallConsumptionOrderRow,
  PaymentSourceFilter,
} from "@/features/mall-consumption-orders/types"
import {
  ATTRIBUTION_STATUS_LABEL,
  ATTRIBUTION_STATUS_TONE,
  COST_BASIS_LABEL,
  COST_BASIS_TONE,
  FACT_TYPE_LABEL,
  FULFILLMENT_CHAIN_LABEL,
  FULFILLMENT_CHAIN_TONE,
} from "@/features/mall-consumption-orders/types"

function formatTime(iso: string) {
  try {
    return new Date(iso).toLocaleString("zh-CN", {
      hour12: false,
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    })
  } catch {
    return iso
  }
}

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

function parseDemoFlag(raw: string | null): ListDemoFlag | undefined {
  if (raw === "no-permission" || raw === "no-scope" || raw === "empty") {
    return raw
  }
  return undefined
}

function paymentCompositionLabel(row: MallConsumptionOrderRow) {
  const { cardAmount, wechatAmount, sourceCount } = row.paymentComposition
  const card = Number(cardAmount) > 0
  const wx = Number(wechatAmount) > 0
  if (card && wx) {
    return `组合 · 卡 ${cardAmount} / 微信 ${wechatAmount}`
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
      if (b.basis === "NONE") {
        return `NONE×${b.lineCount}（空）`
      }
      return `${b.basis}×${b.lineCount}`
    })
    .join(" / ")
}

function supplierSummaryLabel(row: MallConsumptionOrderRow) {
  const s = row.supplierOrderSummary
  if (s.total === 0) {
    if (row.fulfillmentChain === "LEGACY_MANUAL") return "原人工 · 无子订单"
    return "无子订单"
  }
  return `${s.total} 单 · ${s.statuses.join("/")}${s.hasException ? " · 异常" : ""}`
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
  const metric = parseMetric(searchParams.get("metric"))
  const demoFlag = parseDemoFlag(searchParams.get("demo"))
  const pageFromUrl = Math.max(1, Number(searchParams.get("page") ?? "1") || 1)

  const [searchInput, setSearchInput] = React.useState(qParam)
  const searchInputRef = React.useRef<HTMLInputElement | null>(null)
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: pageFromUrl - 1,
    pageSize: 8,
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
    // URL is source of truth for search draft
     
    setSearchInput(qParam)
  }, [qParam])

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
      costBases: costBasis === "all" ? undefined : [costBasis as CostBasis],
      metric: metric === "all" ? undefined : metric,
      page: pagination.pageIndex + 1,
      pageSize: pagination.pageSize,
      demoFlag,
      sort: "occurredAt.desc",
    }),
    [
      attributionStatus,
      costBasis,
      demoFlag,
      fulfillmentChain,
      mallId,
      metric,
      pagination.pageIndex,
      pagination.pageSize,
      paymentSource,
      qParam,
    ]
  )

  const listQuery = useConsumptionOrderListQuery(listQueryInput)
  const data = listQuery.data
  const rows = data?.rows ?? []
  const metrics = data?.metrics ?? []
  const metricValue = (key: MallConsumptionOrderMetricKey) =>
    metrics.find((m) => m.key === key)?.value ?? "—"

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
      const qs = sp.toString()
      router.replace(qs ? `${pathname}?${qs}` : pathname)
    },
    [pathname, router, searchParams]
  )

  const commitSearch = () => {
    replaceParams({ q: searchInput.trim() || undefined })
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
            {formatTime(row.original.paidAt)}
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
        cell: ({ row }) => (
          <span
            className={
              row.original.supplierOrderSummary.hasException
                ? "text-sm text-destructive"
                : "text-sm text-muted-foreground"
            }
          >
            {supplierSummaryLabel(row.original)}
          </span>
        ),
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
                  href={`/commerce/consumption-orders/${row.original.mallOrderId}?section=overview`}
                />
              }
            >
              打开
            </Button>
            {row.original.allowedActions.includes("OPEN_W29") ? (
              <Button
                type="button"
                variant="ghost"
                size="xs"
                render={
                  <Link
                    href={`/governance/integration-errors?from=W25&mallOrderId=${row.original.mallOrderId}`}
                  />
                }
              >
                W29
              </Button>
            ) : null}
          </div>
        ),
      },
    ],
    []
  )

  const empty = data?.emptyReason

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-2.5 p-3 md:p-4">
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
            updatedAt={data ? formatTime(data.factWatermark) : "—"}
            dateTime={data?.factWatermark}
            state={listQuery.isFetching ? "stale" : "fresh"}
            label={`记录更新 · ${data?.permissionVersion ?? "—"}`}
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "refresh",
                label: "刷新",
                icon: RefreshCwIcon,
                variant: "outline",
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
        <div className="flex items-center gap-2 text-sm">
          <span className="whitespace-nowrap text-muted-foreground">演示状态</span>
          <OptionCombobox
            value={demoFlag ?? "normal"}
            onValueChange={(v) => {
              const next = v ?? "normal"
              replaceParams({
                demo: next === "normal" ? undefined : next,
              })
            }}
            options={[
              { value: "normal", label: "正常数据范围" },
              { value: "no-permission", label: "无模块权限" },
              { value: "no-scope", label: "无数据范围" },
              { value: "empty", label: "空数据" },
            ]}
            className="w-40"
            size="sm"
            allowClear={false}
            aria-label="演示空态"
            placeholder="演示空态"
          />
        </div>
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
                label: "权限版本",
                value: exportResult.permissionVersion,
              },
              {
                label: "文件",
                value: exportResult.downloadLabel,
              },
              {
                label: "到期",
                value: formatTime(exportResult.expiresAt),
              },
            ]}
          />
          <BackgroundJobProgress
            mode="all-or-nothing"
            status="succeeded"
            label="导出作业"
            description={`筛选结果 · 字段掩码 · ${exportResult.jobId}`}
            total={exportResult.rowCount}
            completed={exportResult.rowCount}
            succeeded={exportResult.rowCount}
          />
        </div>
      ) : null}

      {exportPreviewOpen ? (
        <div className="space-y-3 rounded-2xl border border-border p-4">
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
            skippedReason="无权限字段以掩码形式导出"
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
          <MetricStrip columns={5} aria-label="消费订单指标筛选">
            <MetricFilterItem
              label="支付成功"
              value={metricValue("paid")}
              active={metric === "paid"}
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
              onClick={() =>
                replaceParams({
                  metric: metric === "cost_none" ? undefined : "cost_none",
                })
              }
            />
          </MetricStrip>

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
                <OptionCombobox
                  value={mallId}
                  onValueChange={(v) =>
                    replaceParams({ mall: v || undefined })
                  }
                  options={[
                    { value: "all", label: "全部商城" },
                    ...(data?.malls ?? []).map((m) => ({
                      value: m.id,
                      label: m.name,
                    })),
                  ]}
                  className="w-44"
                  size="sm"
                  allowClear={false}
                  aria-label="来源商城"
                  placeholder="全部商城"
                />
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
                    { value: "ACTUAL", label: "ACTUAL" },
                    { value: "STANDARD", label: "STANDARD" },
                    { value: "NONE", label: "NONE" },
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
              <Button type="button" variant="ghost" size="sm" onClick={commitSearch}>
                搜索
              </Button>
            }
          />

          {data?.filterSummary ? (
            <p className="text-sm text-muted-foreground" aria-live="polite">
              筛选摘要：{data.filterSummary}
            </p>
          ) : null}

          <BusinessTableFrame
            title="消费订单列表"
            description="商城订单与操作列固定；金额为人民币含税实付。Enter 查看详情。"
            table={
              listQuery.isPending ? (
                <div
                  className="h-64 animate-pulse rounded-lg bg-muted"
                  aria-busy
                />
              ) : listQuery.isError ? (
                <BusinessEmptyState
                  kind="no-data"
                  title="查询失败"
                  description="保留上次成功数据或重试。"
                  action={
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
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
                  description="调整商城、履约链或归集状态后重试。"
                  action={
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => {
                        replaceParams({
                          q: undefined,
                          mall: undefined,
                          fulfillmentChain: undefined,
                          attributionStatus: undefined,
                          paymentSource: undefined,
                          costBasis: undefined,
                          metric: undefined,
                        })
                      }}
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
                  onRowOpen={(row) => {
                    router.push(
                      `/commerce/consumption-orders/${row.mallOrderId}?section=overview`
                    )
                  }}
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
              列表 {data?.pageInfo.total ?? 0} 条 · 页长 {pagination.pageSize}
            </Badge>
          </div>
        </>
      )}
    </div>
  )
}
