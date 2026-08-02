"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"
import {
  DownloadIcon,
  SearchIcon,
  TriangleAlertIcon,
} from "lucide-react"

import {
  BusinessEmptyState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  OptionCombobox,
  PageHeader,
  QuickPreviewSheet,
  StatusTrackSummary,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
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
  useSupplierOrdersQuery,
} from "@/features/supplier-orders/queries"
import { SupplierOrderPreviewPanel } from "@/features/supplier-orders/supplier-order-preview-panel"
import type {
  CancelStatus,
  DemoRole,
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

function formatTime(iso: string): string {
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(iso))
  } catch {
    return iso
  }
}

export function SupplierOrdersListPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const url = React.useMemo(
    () => parseSupplierOrdersSearchParams(searchParams),
    [searchParams]
  )

  const listQueryInput = React.useMemo<SupplierOrderListQuery>(
    () => ({
      view: url.view,
      q: url.q,
      supplierId: url.supplierId,
      fulfillmentStatuses: url.fulfillmentStatus
        ? [url.fulfillmentStatus]
        : undefined,
      cancelStatuses: url.cancelStatus ? [url.cancelStatus] : undefined,
      refundStatuses: url.refundStatus ? [url.refundStatus] : undefined,
      paidFrom: url.paidFrom,
      paidTo: url.paidTo,
      page: url.page,
      pageSize: url.pageSize,
      role: url.role,
      maskCost: url.maskCost,
      noSensitive: url.noSensitive,
    }),
    [url]
  )

  const listQuery = useSupplierOrdersQuery(listQueryInput)
  const previewQuery = useSupplierOrderDetailQuery({
    orderId: url.preview ?? "",
    role: url.role,
    maskCost: url.maskCost,
    noSensitive: url.noSensitive,
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
      const qs = buildSupplierOrdersSearchParams(next)
      router.replace(`${pathname}${qs}`, { scroll: false })
    },
    [pathname, router, url]
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

  React.useEffect(() => {
    setFocusedIndex(0)
  }, [url.view, url.q, url.fulfillmentStatus, url.page, rows.length])

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
            {formatTime(row.original.lastBusinessAt)}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default" },
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
                  重放不可用：须先查询明确无结果且可安全重试
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

  const exportCsv = () => {
    const quote = (v: string) => `"${v.replaceAll('"', '""')}"`
    const lines = rows.map((r) =>
      [
        r.orderNo,
        r.mallOrderNo,
        r.supplierName,
        r.fulfillmentLabel,
        r.cancelLabel,
        r.refundLabel,
        r.externalOrderNo ?? "尚未返回",
        r.lastBusinessAt,
      ]
        .map((x) => quote(String(x)))
        .join(",")
    )
    const csv = [
      "供应商订单,商城单号,供应商,履约,取消,退款,外部单号,更新时间",
      ...lines,
    ].join("\n")
    const urlObj = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
    )
    const a = document.createElement("a")
    a.href = urlObj
    a.download = "供应商订单列表.csv"
    a.click()
    URL.revokeObjectURL(urlObj)
    setActionResult({
      status: "succeeded",
      title: "导出已生成",
      description: `已下载当前页 ${rows.length} 条（不含敏感地址）。`,
      reference: `EXP-W26-${rows.length}`,
    })
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
              onClick={exportCsv}
            >
              <DownloadIcon className="size-3.5" />
              导出
            </Button>
          </div>
        }
      />

      <MetricStrip>
        {metrics.map((m) => (
          <MetricFilterItem
            key={m.key}
            label={m.label}
            value={m.value}
            active={
              m.fulfillmentStatus
                ? url.fulfillmentStatus === m.fulfillmentStatus
                : m.view
                  ? url.view === m.view && !url.fulfillmentStatus
                  : false
            }
            onClick={() => {
              if (m.fulfillmentStatus) {
                pushUrl({
                  fulfillmentStatus: m.fulfillmentStatus,
                  view:
                    m.fulfillmentStatus === "RESULT_UNKNOWN"
                      ? "all"
                      : url.view,
                  page: 1,
                })
              } else if (m.view) {
                pushUrl({
                  view: m.view,
                  fulfillmentStatus: undefined,
                  page: 1,
                })
              } else if (m.aftersalePending) {
                pushUrl({
                  view: "all",
                  refundStatus: "FAILED",
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

      <BusinessTableFrame
        title="供应商订单列表"
        description="身份列与操作列固定；履约/取消/退款三轨正交展示。"
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

                <OptionCombobox
                  value={url.supplierId ?? ""}
                  onValueChange={(v) =>
                    pushUrl({
                      supplierId: v || undefined,
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "全部供应商" },
                    ...suppliers.map((s) => ({
                      value: s.id,
                      label: s.name,
                    })),
                  ]}
                  aria-label="供应商"
                  className="w-[9rem]"
                  size="sm"
                  allowClear={false}
                  placeholder="全部供应商"
                />

                <OptionCombobox
                  value={url.fulfillmentStatus ?? ""}
                  onValueChange={(v) =>
                    pushUrl({
                      fulfillmentStatus: (v ||
                        undefined) as SupplierFulfillmentStatus | undefined,
                      page: 1,
                    })
                  }
                  options={[
                    { value: "", label: "履约·全部" },
                    ...FULFILLMENT_STATUSES.map((s) => ({
                      value: s,
                      label: FULFILLMENT_STATUS_LABEL[s],
                    })),
                  ]}
                  aria-label="履约状态"
                  className="w-[8.5rem]"
                  size="sm"
                  allowClear={false}
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
                  <input
                    type="date"
                    className="h-8 rounded-md border border-input bg-background px-2 text-xs"
                    value={url.paidFrom ?? ""}
                    onChange={(e) =>
                      pushUrl({
                        paidFrom: e.target.value || undefined,
                        page: 1,
                      })
                    }
                  />
                </label>
                <label className="flex items-center gap-1 text-xs text-muted-foreground">
                  至
                  <input
                    type="date"
                    className="h-8 rounded-md border border-input bg-background px-2 text-xs"
                    value={url.paidTo ?? ""}
                    onChange={(e) =>
                      pushUrl({
                        paidTo: e.target.value || undefined,
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
                {(url.fulfillmentStatus ||
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
                        fulfillmentStatus: undefined,
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
