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
  DownloadIcon,
  SearchIcon,
  TriangleAlertIcon,
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
  MultiOptionCombobox,
  OptionCombobox,
  PageHeader,
  QuickPreviewSheet,
  StatusTrackSummary,
  SupplierCombobox,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { listSupplierOptions } from "@/features/supplier-orders/api"
import {
  useQueryResultMutation,
  useSupplierOrderDetailQuery,
  useSupplierOrderExportMutation,
  useSupplierOrdersQuery,
} from "@/features/supplier-orders/queries"
import { SupplierOrderPreviewPanel } from "@/features/supplier-orders/supplier-order-preview-panel"
import type {
  CancelStatus,
  DemoRole,
  ExportCommand,
  ListView,
  RefundStatus,
  SupplierFulfillmentStatus,
  SupplierOrderListQuery,
  SupplierOrderListRow,
} from "@/features/supplier-orders/types"
import {
  CANCEL_STATUS_LABEL,
  CANCEL_STATUSES,
  FULFILLMENT_STATUS_LABEL,
  FULFILLMENT_STATUSES,
  REFUND_STATUS_LABEL,
  REFUND_STATUSES,
  VIEW_LABEL,
} from "@/features/supplier-orders/types"
import {
  buildSupplierOrdersSearchParams,
  parseSupplierOrdersSearchParams,
} from "@/features/supplier-orders/url-state"
import { formatDateTime } from "@/lib/datetime"

const SORT_COLUMN_TO_FIELD: Record<
  string,
  NonNullable<SupplierOrderListQuery["sortBy"]>
> = {
  identity: "orderNo",
  mall: "mallOrderNo",
  external: "externalOrderNo",
  updated: "lastBusinessAt",
}

export function SupplierOrdersListPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const url = React.useMemo(
    () => parseSupplierOrdersSearchParams(searchParams),
    [searchParams]
  )
  const returnTo = searchParams.get("returnTo") ?? undefined

  const listQueryInput = React.useMemo<SupplierOrderListQuery>(
    () => ({
      view: url.view,
      q: url.q,
      supplierId: url.supplierId,
      fulfillmentStatuses: url.fulfillmentStatuses,
      cancelStatuses: url.cancelStatus ? [url.cancelStatus] : undefined,
      refundStatuses: url.refundStatus ? [url.refundStatus] : undefined,
      paidFrom: url.paidFrom,
      paidTo: url.paidTo,
      page: url.page,
      pageSize: url.pageSize,
      role: url.role,
      sortBy: url.sort ? SORT_COLUMN_TO_FIELD[url.sort] : undefined,
      sortDir: url.dir,
    }),
    [url]
  )

  const listQuery = useSupplierOrdersQuery(listQueryInput)
  const previewQuery = useSupplierOrderDetailQuery({
    orderId: url.preview ?? "",
    role: url.role,
    enabled: Boolean(url.preview),
  })
  const queryResultMutation = useQueryResultMutation()

  const suppliers = React.useMemo(() => listSupplierOptions(), [])
  const [searchDraft, setSearchDraft] = React.useState(url.q ?? "")
  const [focusedIndex, setFocusedIndex] = React.useState(0)
  const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())
  const [actionResult, setActionResult] = React.useState<{
    status: "succeeded" | "failed" | "unknown" | "blocked"
    title: string
    description: string
    reference?: string
  } | null>(null)
  const [exportPreviewOpen, setExportPreviewOpen] = React.useState(false)
  const [pendingExport, setPendingExport] =
    React.useState<ExportCommand | null>(null)
  const [exportResult, setExportResult] = React.useState<{
    jobId: string
    rowCount: number
    permissionVersion: string
    maskDisclaimer: string
    downloadLabel: string
    expiresAt: string
  } | null>(null)

  const exportMutation = useSupplierOrderExportMutation()

  React.useEffect(() => {
    setSearchDraft(url.q ?? "")
  }, [url.q])

  // W25 钻取：supplierOrderId / from=W25 时进入对象中心（TaskTab 身份稳定）
  React.useEffect(() => {
    const soId =
      searchParams.get("supplierOrderId") ?? searchParams.get("preview")
    const from = searchParams.get("from")
    if (
      soId &&
      (from === "W25" ||
        from === "mall-order" ||
        searchParams.get("openCenter") === "1")
    ) {
      const mall =
        searchParams.get("mallOrderId") ?? searchParams.get("sourceId")
      const qs = new URLSearchParams()
      if (from) qs.set("from", from === "W25" ? "mall-order" : from)
      if (mall) qs.set("sourceId", mall)
      const s = qs.toString()
      router.replace(
        `/supplier-api/orders/${soId}${s ? `?${s}` : ""}`
      )
    }
  }, [router, searchParams])

  const pushUrl = React.useCallback(
    (patch: Partial<typeof url>) => {
      const next = { ...url, ...patch }
      let qs = buildSupplierOrdersSearchParams(next)
      // URL state codec 不声明 returnTo，筛选/分页变化时手动保留返回上下文
      if (returnTo) {
        qs += `${qs ? "&" : "?"}returnTo=${encodeURIComponent(returnTo)}`
      }
      router.replace(`${pathname}${qs}`, { scroll: false })
    },
    [pathname, router, url, returnTo]
  )

  const rows = listQuery.data?.rows ?? []
  const metrics = listQuery.data?.metrics ?? []
  const total = listQuery.data?.pageInfo.total ?? 0

  const pagination = React.useMemo<PaginationState>(
    () => ({
      pageIndex: Math.max(0, url.page - 1),
      pageSize: url.pageSize,
    }),
    [url.page, url.pageSize]
  )

  const sorting = React.useMemo<SortingState>(
    () =>
      url.sort && SORT_COLUMN_TO_FIELD[url.sort]
        ? [{ id: url.sort, desc: url.dir === "desc" }]
        : [],
    [url.dir, url.sort]
  )

  const handleSortingChange = React.useCallback(
    (next: SortingState) => {
      const head = next[0]
      pushUrl({
        sort: head && SORT_COLUMN_TO_FIELD[head.id] ? head.id : undefined,
        dir: head ? (head.desc ? "desc" : "asc") : undefined,
        page: 1,
      })
    },
    [pushUrl]
  )

  React.useEffect(() => {
    setFocusedIndex(0)
  }, [url.view, url.q, url.fulfillmentStatuses, url.page, rows.length])

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT" ||
          target.isContentEditable)
      ) {
        if (event.key !== "Escape") return
      }

      if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault()
        document
          .querySelector<HTMLInputElement>('[data-slot="sfo-list-search"]')
          ?.focus()
        return
      }

      if (rows.length === 0) return

      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        setFocusedIndex((i) => Math.min(rows.length - 1, i + 1))
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault()
        setFocusedIndex((i) => Math.max(0, i - 1))
      } else if (event.key === "Enter") {
        event.preventDefault()
        const row = rows[focusedIndex]
        if (row) pushUrl({ preview: row.orderId })
      } else if (event.key === "Escape" && url.preview) {
        event.preventDefault()
        const id = url.preview
        pushUrl({ preview: undefined })
        requestAnimationFrame(() => {
          rowRefs.current.get(id)?.focus()
        })
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [focusedIndex, pushUrl, rows, url.preview])

  const openPreview = React.useCallback(
    (orderId: string) => pushUrl({ preview: orderId }),
    [pushUrl]
  )

  const closePreview = React.useCallback(() => {
    const id = url.preview
    pushUrl({ preview: undefined })
    if (id) {
      requestAnimationFrame(() => {
        rowRefs.current.get(id)?.focus()
      })
    }
  }, [pushUrl, url.preview])

  const handleQueryFromList = async (row: SupplierOrderListRow) => {
    if (!row.allowedActions.includes("QUERY_RESULT")) {
      setActionResult({
        status: "blocked",
        title: "无法查询原结果",
        description:
          row.actionBlockers.find((b) => b.action === "QUERY_RESULT")
            ?.message ?? "当前订单不可查询",
      })
      return
    }
    // 打开预览并在取得详情锁版本后查询
    openPreview(row.orderId)
    const { fetchSupplierOrderDetail } = await import(
      "@/features/supplier-orders/api"
    )
    const detail = await fetchSupplierOrderDetail({
      orderId: row.orderId,
      role: url.role,
    })
    if (!detail) {
      setActionResult({
        status: "failed",
        title: "无法加载订单",
        description: "未找到供应商订单详情",
      })
      return
    }
    const res = await queryResultMutation.mutateAsync({
      orderId: row.orderId,
      expectedLockVersion: detail.order.lockVersion,
      targetSupplierActionId: detail.placeActionId,
      operationId: `op-query-list-${Date.now()}`,
      idempotencyKey: `query-list-${row.orderId}-${Date.now()}`,
      workItemId: detail.workItem?.workItemId,
      expectedSubjectHash: detail.workItem?.subjectHash,
      expectedSubjectVersion: detail.workItem?.subjectVersion,
    })
    setActionResult({
      status:
        res.status === "unknown"
          ? "unknown"
          : res.status === "blocked"
            ? "blocked"
            : res.status === "succeeded"
              ? "succeeded"
              : "failed",
      title:
        res.status === "succeeded"
          ? "查询原结果已完成"
          : res.status === "unknown"
            ? "查询结果仍未知"
            : "查询未成功",
      description: res.message,
      reference: res.reference,
    })
  }

  const columns = React.useMemo<ColumnDef<SupplierOrderListRow>[]>(
    () => [
      {
        id: "identity",
        accessorKey: "orderNo",
        header: "供应商订单",
        meta: { label: "供应商订单", width: "reference" },
        cell: ({ row }) => (
          <div
            className="flex min-w-0 flex-col gap-0.5"
            ref={(el) => {
              if (el) rowRefs.current.set(row.original.orderId, el)
              else rowRefs.current.delete(row.original.orderId)
            }}
            tabIndex={
              rows[focusedIndex]?.orderId === row.original.orderId ? 0 : -1
            }
            data-focused={
              rows[focusedIndex]?.orderId === row.original.orderId
                ? "true"
                : undefined
            }
          >
            <Button
              type="button"
              variant="link"
              size="xs"
              className="num h-auto justify-start px-0"
              aria-label={`预览 ${row.original.orderNo}`}
              onClick={() => openPreview(row.original.orderId)}
            >
              {row.original.orderNo}
            </Button>
            <span className="truncate text-[11px] text-muted-foreground">
              {row.original.supplierName}
            </span>
          </div>
        ),
      },
      {
        id: "mall",
        accessorKey: "mallOrderNo",
        header: "商城单号",
        meta: { label: "商城订单", width: "reference" },
        cell: ({ row }) => (
          <Link
            href={`/commerce/consumption-orders?q=${encodeURIComponent(row.original.mallOrderNo)}`}
            className="num text-sm text-primary underline-offset-2 hover:underline"
            onClick={(e) => e.stopPropagation()}
          >
            {row.original.mallOrderNo}
          </Link>
        ),
      },
      {
        id: "tracks",
        header: "履约 / 取消 / 退款",
        meta: { label: "三轨状态", width: "tracks" },
        enableSorting: false,
        cell: ({ row }) => (
          <StatusTrackSummary
            variant="inline"
            className="flex-nowrap gap-x-2 gap-y-0"
            aria-label={`${row.original.orderNo} 三轨状态`}
            tracks={[
              {
                id: "ff",
                label: "履约",
                status: {
                  label: row.original.fulfillmentLabel,
                  tone: row.original.fulfillmentTone,
                },
              },
              {
                id: "cancel",
                label: "取消",
                status: {
                  label: row.original.cancelLabel,
                  tone: row.original.cancelTone,
                },
              },
              {
                id: "refund",
                label: "退款",
                status: {
                  label: row.original.refundLabel,
                  tone: row.original.refundTone,
                },
              },
            ]}
          />
        ),
      },
      {
        id: "external",
        accessorKey: "externalOrderNo",
        header: "外部单号",
        meta: { label: "供应商外部单号", width: "reference" },
        cell: ({ row }) =>
          row.original.externalOrderNo ? (
            <span className="num text-xs">{row.original.externalOrderNo}</span>
          ) : (
            <span className="text-xs text-muted-foreground">尚未返回</span>
          ),
      },
      {
        id: "updated",
        accessorKey: "lastBusinessAt",
        header: "更新时间",
        meta: { label: "最近业务变化", width: "default" },
        cell: ({ row }) => (
          <span className="num text-xs text-muted-foreground">
            {formatDateTime(row.original.lastBusinessAt, "monthDayIntl", "passthrough")}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default" },
        enableSorting: false,
        cell: ({ row }) => {
          const r = row.original
          const canQuery = r.allowedActions.includes("QUERY_RESULT")
          const canReplay = r.allowedActions.includes("REPLAY")
          return (
            <div className="flex flex-wrap items-center gap-1">
              <Button
                type="button"
                size="xs"
                variant="outline"
                onClick={() => openPreview(r.orderId)}
              >
                预览
              </Button>
              <Button
                type="button"
                size="xs"
                variant="outline"
                render={
                  <Link href={`/supplier-api/orders/${r.orderId}`} />
                }
              >
                中心
              </Button>
              {r.fulfillmentStatus === "RESULT_UNKNOWN" ? (
                <Button
                  type="button"
                  size="xs"
                  disabled={!canQuery || queryResultMutation.isPending}
                  onClick={() => void handleQueryFromList(r)}
                >
                  查询原结果
                </Button>
              ) : null}
              {r.fulfillmentStatus === "RESULT_UNKNOWN" && !canReplay ? (
                <span className="sr-only">
                  不可重试：需先查询确认无结果且系统允许重试
                </span>
              ) : null}
            </div>
          )
        },
      },
    ],
    // eslint-disable-next-line react-hooks/exhaustive-deps -- handlers stable enough for list
    [focusedIndex, openPreview, queryResultMutation.isPending, rows]
  )

  const confirmExport = async () => {
    const requestId = `req-w26-export-${Date.now()}`
    const command: ExportCommand = {
      selectionSnapshotId: `snap-${requestId}`,
      fieldSetId: "w26-list-default-masked",
      requestId,
      rowCount: listQuery.data?.pageInfo.total ?? 0,
      filterSummary: listQuery.data?.filterSummary ?? "",
    }
    setPendingExport(command)
    await runExport(command)
  }

  const retryExport = async () => {
    if (!pendingExport) return
    await runExport(pendingExport)
  }

  const runExport = async (command: ExportCommand) => {
    const result = await exportMutation.mutateAsync(command)
    setExportResult({
      jobId: result.jobId,
      rowCount: result.rowCount,
      permissionVersion: result.permissionVersion,
      maskDisclaimer: result.maskDisclaimer,
      downloadLabel: result.downloadLabel,
      expiresAt: result.expiresAt,
    })
    setPendingExport(null)
    setExportPreviewOpen(false)
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="供应商订单"
        breadcrumbs={[
          { id: "api", label: "供应商 API", href: "/supplier-api/orders" },
          { id: "so", label: "供应商订单", current: true },
        ]}
        actions={
          <div className="flex flex-wrap items-center gap-2">
            <DataFreshness
              updatedAt="刚刚"
              dateTime={listQuery.data?.queriedAt}
              state={listQuery.isFetching ? "syncing" : "fresh"}
              label="列表数据"
            />
            <OptionCombobox
              value={url.role}
              onValueChange={(v) =>
                pushUrl({ role: (v ?? url.role) as DemoRole, page: 1 })
              }
              options={[
                { value: "procurement", label: "采购" },
                { value: "cs", label: "客服" },
                { value: "ops", label: "运营" },
                { value: "finance", label: "财务" },
                { value: "admin", label: "管理员" },
              ]}
              aria-label="演示角色"
              className="w-[7.5rem]"
              size="sm"
              allowClear={false}
              placeholder="演示角色"
            />
            <Button
              type="button"
              variant="outline"
              size="sm"
              disabled={
                !listQuery.data ||
                total === 0 ||
                exportMutation.isPending
              }
              onClick={() => setExportPreviewOpen(true)}
            >
              <DownloadIcon className="size-3.5" />
              导出
            </Button>
          </div>
        }
      />

      {returnTo ? (
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-2xl border border-border bg-card px-4 py-2.5 text-sm">
          <span className="text-muted-foreground">
            从关联页面进来的。返回时会回到原来的位置。
          </span>
          <Button
            type="button"
            size="sm"
            variant="outline"
            render={<Link href={returnTo} />}
          >
            返回来源
          </Button>
        </div>
      ) : null}

      <MetricStrip>
        {metrics.map((m) => (
          <MetricFilterItem
            key={m.key}
            label={m.label}
            value={m.value}
            active={
              m.fulfillmentStatus
                ? url.fulfillmentStatuses?.includes(m.fulfillmentStatus) ??
                  false
                : m.view
                  ? url.view === m.view && !url.fulfillmentStatuses?.length
                  : false
            }
            onClick={() => {
              if (m.fulfillmentStatus) {
                pushUrl({
                  fulfillmentStatuses: [m.fulfillmentStatus],
                  view:
                    m.fulfillmentStatus === "RESULT_UNKNOWN"
                      ? "all"
                      : url.view,
                  page: 1,
                })
              } else if (m.view) {
                pushUrl({
                  view: m.view,
                  fulfillmentStatuses: undefined,
                  page: 1,
                })
              } else if (m.aftersalePending) {
                pushUrl({
                  view: "all",
                  refundStatus: "REFUND_FAILED",
                  page: 1,
                })
              }
            }}
          />
        ))}
      </MetricStrip>

      {actionResult ? (
        <FormalActionResult
          status={
            actionResult.status === "failed"
              ? "rejected"
              : actionResult.status
          }
          title={actionResult.title}
          description={actionResult.description}
          reference={actionResult.reference}
          actions={
            <Button
              type="button"
              size="sm"
              variant="outline"
              onClick={() => setActionResult(null)}
            >
              关闭
            </Button>
          }
        />
      ) : null}

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
                value: formatDateTime(exportResult.expiresAt, "monthDayIntl", "passthrough"),
              },
            ]}
          />
          <BackgroundJobProgress
            mode="all-or-nothing"
            status="succeeded"
            label="导出作业"
            description={`筛选快照 · 字段打码 · ${exportResult.jobId}`}
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
            description="按当前筛选快照导出，不限于当前页；结果 7 天内可下载，下载时将重新校验权限。"
            filterSummary={listQuery.data?.filterSummary ?? "—"}
            selectionScope="当前筛选全部"
            estimated={total}
            processable={total}
            skipped={0}
            background
            sensitiveFields={["收货地址", "手机号", "未授权成本金额"]}
            skippedReason="无权限字段以打码形式导出，默认列不含收货地址"
          />
          <div className="flex flex-wrap gap-2">
            {exportMutation.isError ? (
              <p className="w-full text-sm text-destructive" aria-live="polite">
                导出任务创建失败，可按原筛选快照重试。
              </p>
            ) : null}
            <Button
              type="button"
              size="sm"
              disabled={exportMutation.isPending}
              onClick={() => {
                if (exportMutation.isError && pendingExport) {
                  void retryExport()
                } else {
                  void confirmExport()
                }
              }}
            >
              {exportMutation.isError && pendingExport
                ? "按原快照重试"
                : "确认导出"}
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

      <BusinessTableFrame
        title="供应商订单列表"
        description="身份列与操作列固定；履约/取消/退款三种状态独立展示。"
        toolbar={
          <ListToolbar
            search={
              <InputGroup className="w-full max-w-sm">
                <InputGroupAddon>
                  <SearchIcon className="size-3.5" />
                </InputGroupAddon>
                <InputGroupInput
                  data-slot="sfo-list-search"
                  value={searchDraft}
                  onChange={(e) => setSearchDraft(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      pushUrl({ q: searchDraft || undefined, page: 1 })
                    }
                  }}
                  onBlur={() => {
                    if ((url.q ?? "") !== searchDraft) {
                      pushUrl({ q: searchDraft || undefined, page: 1 })
                    }
                  }}
                  placeholder="供应商订单号、商城订单、外部单号"
                  aria-label="搜索供应商订单"
                />
              </InputGroup>
            }
            filters={
              <div className="flex flex-wrap items-center gap-2">
                <ToggleGroup
                  value={[url.view]}
                  onValueChange={(values) => {
                    const next =
                      (values[0] as ListView | undefined) ?? "actionable"
                    pushUrl({ view: next, page: 1 })
                  }}
                  variant="outline"
                  size="sm"
                  spacing={0}
                >
                  {(
                    Object.keys(VIEW_LABEL) as ListView[]
                  ).map((v) => (
                    <ToggleGroupItem key={v} value={v}>
                      {VIEW_LABEL[v]}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>

                <SupplierCombobox
                  value={url.supplierId || undefined}
                  onValueChange={(id) =>
                    pushUrl({
                      supplierId: id || undefined,
                      page: 1,
                    })
                  }
                  suppliers={suppliers.map((s) => ({
                    supplierId: s.id,
                    supplierName: s.name,
                    statusLabel: "可选",
                    statusTone: "neutral",
                  }))}
                  aria-label="供应商"
                  className="w-[12rem]"
                  placeholder="全部供应商"
                />

                <MultiOptionCombobox
                  value={url.fulfillmentStatuses ?? []}
                  onValueChange={(values) =>
                    pushUrl({
                      fulfillmentStatuses:
                        values.length > 0
                          ? (values as SupplierFulfillmentStatus[])
                          : undefined,
                      page: 1,
                    })
                  }
                  options={FULFILLMENT_STATUSES.map((s) => ({
                    value: s,
                    label: FULFILLMENT_STATUS_LABEL[s],
                  }))}
                  aria-label="履约状态"
                  className="w-[10rem]"
                  size="sm"
                  placeholder="履约·全部"
                />

                <OptionCombobox
                  value={url.cancelStatus ?? ""}
                  onValueChange={(v) =>
                    pushUrl({
                      cancelStatus: (v ||
                        undefined) as CancelStatus | undefined,
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "取消·全部" },
                    ...CANCEL_STATUSES.map((s) => ({
                      value: s,
                      label: CANCEL_STATUS_LABEL[s],
                    })),
                  ]}
                  aria-label="取消状态"
                  className="w-[7.5rem]"
                  size="sm"
                  allowClear={false}
                  placeholder="取消·全部"
                />

                <OptionCombobox
                  value={url.refundStatus ?? ""}
                  onValueChange={(v) =>
                    pushUrl({
                      refundStatus: (v ||
                        undefined) as RefundStatus | undefined,
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "退款·全部" },
                    ...REFUND_STATUSES.map((s) => ({
                      value: s,
                      label: REFUND_STATUS_LABEL[s],
                    })),
                  ]}
                  aria-label="退款状态"
                  className="w-[7.5rem]"
                  size="sm"
                  allowClear={false}
                  placeholder="退款·全部"
                />

                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  支付自
                  <DatePicker
                    className="w-[9.5rem]"
                    value={url.paidFrom || undefined}
                    onValueChange={(next) =>
                      pushUrl({
                        paidFrom: next || undefined,
                        page: 1,
                      })
                    }
                  />
                </label>
                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  至
                  <DatePicker
                    className="w-[9.5rem]"
                    value={url.paidTo || undefined}
                    onValueChange={(next) =>
                      pushUrl({
                        paidTo: next || undefined,
                        page: 1,
                      })
                    }
                  />
                </label>
              </div>
            }
            actions={
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span aria-live="polite">
                  共 {total.toLocaleString("zh-CN")} 条
                </span>
                {(url.fulfillmentStatuses?.length ||
                  url.cancelStatus ||
                  url.refundStatus ||
                  url.q ||
                  url.supplierId) && (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={() =>
                      pushUrl({
                        fulfillmentStatuses: undefined,
                        cancelStatus: undefined,
                        refundStatus: undefined,
                        q: undefined,
                        supplierId: undefined,
                        paidFrom: undefined,
                        paidTo: undefined,
                        page: 1,
                        view: "actionable",
                      })
                    }
                  >
                    清除筛选
                  </Button>
                )}
              </div>
            }
          />
        }
        table={
          listQuery.isError ? (
            <BusinessEmptyState
              kind="no-data"
              title="加载失败"
              description="无法取得供应商订单列表，请重试。"
              action={
                <Button
                  type="button"
                  size="sm"
                  onClick={() => void listQuery.refetch()}
                >
                  重试
                </Button>
              }
            />
          ) : !listQuery.isPending && rows.length === 0 ? (
            <BusinessEmptyState
              kind="filter"
              title="当前范围没有供应商订单"
              description="调整视图、供应商或支付时间，或从商城消费订单钻取。"
              action={
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  render={<Link href="/commerce/consumption-orders" />}
                >
                  打开商城消费订单
                </Button>
              }
            />
          ) : (
            <DataTable
              data={rows}
              columns={columns}
              getRowId={(row) => row.orderId}
              rowCount={total}
              sorting={sorting}
              onSortingChange={handleSortingChange}
              pagination={pagination}
              onPaginationChange={(next) => {
                pushUrl({
                  page: next.pageIndex + 1,
                  pageSize: next.pageSize,
                })
              }}
              layout="flush"
              density="compact"
              defaultColumnPinning={{
                left: ["identity"],
                right: ["actions"],
              }}
              onRowPreview={(row) => openPreview(row.orderId)}
              onRowOpen={(row) => openPreview(row.orderId)}
            />
          )
        }
      />

      <QuickPreviewSheet
        open={Boolean(url.preview)}
        onOpenChange={(open) => {
          if (!open) closePreview()
        }}
        size="detail"
        title={previewQuery.data?.order.supplierName ?? "供应商订单预览"}
        identity={
          previewQuery.data ? (
            <span className="num">
              {previewQuery.data.order.orderNo}
              {previewQuery.data.order.externalOrderNo
                ? ` · ${previewQuery.data.order.externalOrderNo}`
                : " · 外部单号尚未返回"}
            </span>
          ) : null
        }
        summary={
          previewQuery.data ? (
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="preview"
                label={previewQuery.data.order.fulfillmentLabel}
                tone={previewQuery.data.order.fulfillmentTone}
              />
              <Badge variant="secondary">
                取消 {previewQuery.data.order.cancelLabel}
              </Badge>
              <Badge variant="secondary">
                退款 {previewQuery.data.order.refundLabel}
              </Badge>
              {previewQuery.data.order.fulfillmentStatus ===
              "RESULT_UNKNOWN" ? (
                <Badge variant="outline" className="gap-1">
                  <TriangleAlertIcon className="size-3" />
                  须先查询
                </Badge>
              ) : null}
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
                    href={`/supplier-api/orders/${previewQuery.data.order.id}`}
                  />
                }
              >
                查看详情
              </Button>
              {previewQuery.data.allowedActions.includes("QUERY_RESULT") ? (
                <Button
                  type="button"
                  disabled={queryResultMutation.isPending}
                  onClick={() => {
                    void queryResultMutation
                      .mutateAsync({
                        orderId: previewQuery.data!.order.id,
                        expectedLockVersion:
                          previewQuery.data!.order.lockVersion,
                        targetSupplierActionId:
                          previewQuery.data!.placeActionId,
                        operationId: `op-query-preview-${Date.now()}`,
                        idempotencyKey: `query-preview-${previewQuery.data!.order.id}-${Date.now()}`,
                        workItemId: previewQuery.data!.workItem?.workItemId,
                        expectedSubjectHash:
                          previewQuery.data!.workItem?.subjectHash,
                        expectedSubjectVersion:
                          previewQuery.data!.workItem?.subjectVersion,
                      })
                      .then((res) => {
                        setActionResult({
                          status:
                            res.status === "failed"
                              ? "failed"
                              : res.status === "blocked"
                                ? "blocked"
                                : res.status === "unknown"
                                  ? "unknown"
                                  : "succeeded",
                          title:
                            res.status === "succeeded"
                              ? "查询原结果已完成"
                              : "查询未形成终局成功",
                          description: res.message,
                          reference: res.reference,
                        })
                      })
                  }}
                >
                  查询原结果
                </Button>
              ) : null}
            </>
          ) : null
        }
      >
        {previewQuery.isPending ? (
          <div className="p-5 text-sm text-muted-foreground">加载预览…</div>
        ) : previewQuery.data ? (
          <SupplierOrderPreviewPanel order={previewQuery.data} />
        ) : (
          <div className="p-5 text-sm text-muted-foreground">
            未找到该供应商订单
          </div>
        )}
      </QuickPreviewSheet>
    </div>
  )
}
