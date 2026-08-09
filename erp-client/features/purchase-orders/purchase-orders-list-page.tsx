"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  DownloadIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react"
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/react-table"

import {
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  BusinessFailureState,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  MoneyValue,
  OptionCombobox,
  PageActions,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
  StatusTrackSummary,
  surfaceInsetClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"

import { PurchaseOrderPreviewPanel } from "@/features/purchase-orders/purchase-order-preview-panel"
import {
  useCreateFromBasisMutation,
  useCreationBasesQuery,
  usePurchaseOrderCenterQuery,
  usePurchaseOrderExportDataQuery,
  usePurchaseOrdersQuery,
} from "@/features/purchase-orders/queries"
import type { PurchaseOrderListQuery } from "@/features/purchase-orders/api"
import type {
  PurchaseOrderListItem,
  PurchaseOrderMetricFilter,
  PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"
import {
  FULFILLMENT_RESPONSIBILITY_LABEL,
  PO_METRIC_LABEL,
  PO_STATUS_FILTER_LABEL,
  PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"
import {
  buildPurchaseOrdersSearchParams,
  parsePurchaseOrdersSearchParams,
  type PurchaseOrdersUrlState,
} from "@/features/purchase-orders/url-state"

function displayNo(row: PurchaseOrderListItem) {
  return row.purchaseNo ?? row.draftLabel ?? "采购单（未编号）"
}

/** 状态枚举 ≥5：用 Combobox，禁止长 Toggle 横排（ui-filter-design §3.2） */
const PO_STATUS_FILTER_OPTIONS = (
  Object.entries(PO_STATUS_FILTER_LABEL) as Array<
    [PurchaseOrderStatusFilter, string]
  >
)
  .filter(([value]) => value !== "all")
  .map(([value, label]) => ({ value, label }))

export function PurchaseOrdersListPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const basisFromUrl = searchParams.get("basisId")

  const url = React.useMemo(
    () => parsePurchaseOrdersSearchParams(searchParams),
    [searchParams]
  )

  const pushUrl = React.useCallback(
    (patch: Partial<PurchaseOrdersUrlState>) => {
      const next = { ...url, ...patch }
      router.replace(
        `${pathname}${buildPurchaseOrdersSearchParams(next)}`,
        { scroll: false }
      )
    },
    [pathname, router, url]
  )

  // 跨单据跳转的返回目标：当前列表（保留筛选）。basisId 只服务于建单 Dialog，
  // 带回去会在返回时误弹建单框，故剔除。
  const listReturnHref = React.useMemo(() => {
    const sp = new URLSearchParams(searchParams.toString())
    sp.delete("basisId")
    const qs = sp.toString()
    return qs ? `${pathname}?${qs}` : pathname
  }, [pathname, searchParams])

  const [sortBy, sortDir] = React.useMemo(() => {
    const [id, dir] = (url.sort ?? "").split(":")
    if (!id || (dir !== "asc" && dir !== "desc")) {
      return [undefined, undefined] as const
    }
    return [id, dir] as const
  }, [url.sort])

  // 「可建单依据」是动作卡不是筛选卡：URL 带该值时按全部列表处理。
  // metric=pending_create 只由建单入口携带，指标条上无对应高亮控件（其它分支的
  // metricKey 均为有控件的高亮枚举值，按原值消费即可，无需额外分支处理）。
  const effectiveMetric = url.metric === "pending_create" ? "all" : url.metric

  const listQueryInput = React.useMemo<PurchaseOrderListQuery>(
    () => ({
      q: url.q,
      status: url.status,
      metric: effectiveMetric,
      page: url.page,
      pageSize: url.pageSize,
      sortBy,
      sortDir,
    }),
    [effectiveMetric, sortBy, sortDir, url]
  )
  const listQuery = usePurchaseOrdersQuery(listQueryInput)
  const exportQuery = usePurchaseOrderExportDataQuery(listQueryInput)
  const basesQuery = useCreationBasesQuery()
  const createMutation = useCreateFromBasisMutation()

  const pageRows = React.useMemo(
    () => listQuery.data?.rows ?? [],
    [listQuery.data]
  )
  const metrics = listQuery.data?.metrics ?? []
  const total = listQuery.data?.total ?? 0

  const search = url.q ?? ""
  const statusFilter = url.status
  const metricKey = url.metric
  const [searchDraft, setSearchDraft] = React.useState(search)
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [focusedIndex, setFocusedIndex] = React.useState(0)
  const [createOpen, setCreateOpen] = React.useState(false)
  const [selectedBasisId, setSelectedBasisId] = React.useState<string>("")
  const [actionResult, setActionResult] = React.useState<{
    status: "succeeded" | "failed" | "unknown"
    title: string
    description: string
    reference?: string
  } | null>(null)

  const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

  const pagination = React.useMemo<PaginationState>(
    () => ({
      pageIndex: Math.max(0, url.page - 1),
      pageSize: url.pageSize,
    }),
    [url.page, url.pageSize]
  )

  const sorting = React.useMemo<SortingState>(
    () =>
      sortBy && sortDir
        ? [{ id: sortBy, desc: sortDir === "desc" }]
        : [],
    [sortBy, sortDir]
  )

  // P4：清除=清搜索/状态/指标筛选并回第 1 页，保留排序与视图参数；
  // 空态与工具栏常驻清除共用同一函数（D19）。
  const hasActiveFilters =
    Boolean(url.q) || statusFilter !== "all" || effectiveMetric !== "all"
  const clearFilters = React.useCallback(() => {
    pushUrl({ q: undefined, status: "all", metric: "all", page: 1 })
  }, [pushUrl])

  React.useEffect(() => {
    setSearchDraft(search)
  }, [search])

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchDraft.trim() === (url.q ?? "")) return
      pushUrl({ q: searchDraft.trim() || undefined, page: 1 })
    }, 300)
    return () => globalThis.clearTimeout(handle)
  }, [pushUrl, searchDraft, url.q])

  const previewQuery = usePurchaseOrderCenterQuery(previewId ?? "")

  React.useEffect(() => {
    setFocusedIndex(0)
  }, [metricKey, pageRows.length, search, statusFilter])

  React.useEffect(() => {
    const data = listQuery.data
    if (!data || data.page === url.page) return
    pushUrl({ page: data.page })
  }, [listQuery.data, pushUrl, url.page])

  // 键盘导航：仅列表可见且预览/建单弹层未打开时生效；焦点行滚动到可视区。
  React.useEffect(() => {
    const focusedRow = pageRows[focusedIndex]
    if (!focusedRow) return
    rowRefs.current.get(focusedRow.purchaseOrderId)?.scrollIntoView({
      block: "nearest",
    })
  }, [focusedIndex, pageRows])

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
        if (event.key === "/" && target.tagName !== "INPUT") {
          // allow
        } else if (event.key !== "Escape") {
          return
        }
      }

      if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault()
        document
          .querySelector<HTMLInputElement>('[data-slot="po-list-search"]')
          ?.focus()
        return
      }

      // 预览抽屉或建单弹框打开时，后台列表不响应 j/k/Enter，避免状态污染
      if (previewId) {
        if (event.key === "Escape") {
          event.preventDefault()
          const id = previewId
          setPreviewId(null)
          requestAnimationFrame(() => {
            rowRefs.current.get(id)?.focus()
          })
        }
        return
      }
      if (createOpen) return

      if (pageRows.length === 0) return

      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        setFocusedIndex((i) => Math.min(pageRows.length - 1, i + 1))
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault()
        setFocusedIndex((i) => Math.max(0, i - 1))
      } else if (event.key === "Enter") {
        event.preventDefault()
        const row = pageRows[focusedIndex]
        if (row) setPreviewId(row.purchaseOrderId)
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [createOpen, focusedIndex, pageRows, previewId])

  const exportCsv = React.useCallback(async () => {
    const result = await exportQuery.refetch()
    const rows = result.data ?? []
    if (rows.length === 0) return
    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const body = rows.map((row) =>
      [
        displayNo(row),
        row.statusLabel,
        row.supplierName,
        row.salesOrderNo,
        PURCHASE_TYPE_LABEL[row.purchaseType],
        row.costMasked ? "***" : row.grossAmount,
        row.paymentProgress,
        row.fulfillmentProgress,
        row.ownerName,
      ]
        .map((value) => quote(String(value)))
        .join(",")
    )
    const csv = [
      "采购单号,状态,供应商,来源销售单,类型,含税金额,付款,履约,负责人",
      ...body,
    ].join("\n")
    const url = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = "采购单列表.csv"
    anchor.click()
    URL.revokeObjectURL(url)
    setActionResult({
      status: "succeeded",
      title: "导出已生成",
      description: `已下载当前筛选 ${rows.length} 条。`,
      reference: `EXPORT-${rows.length}`,
    })
  }, [exportQuery])

  const openBases = basesQuery.data?.filter((b) => !b.consumed) ?? []

  React.useEffect(() => {
    if (!basisFromUrl) return
    // W07/W05 携带创建依据：打开建单 Dialog，不要求 work_item
    setSelectedBasisId(basisFromUrl)
    setCreateOpen(true)
  }, [basisFromUrl])

  const handleCreate = async () => {
    if (!selectedBasisId) return
    const basis = openBases.find((b) => b.basisId === selectedBasisId)
    const result = await createMutation.mutateAsync({
      basisId: selectedBasisId,
      idempotencyKey: `create-basis-${selectedBasisId}-${Date.now()}`,
    })
    if (result.status === "succeeded") {
      setCreateOpen(false)
      setActionResult({
        status: "succeeded",
        title: "已创建采购草稿",
        description: `${result.data.draftLabel} · 已使用采购二次确认创建依据（销售单 ${basis?.salesOrderNo ?? "—"} · ${basis?.supplierName ?? "—"}）。`,
        reference: result.reference,
      })
      router.push(
        `/procurement/orders/${result.data.purchaseOrderId}?mode=edit`
      )
    } else if (result.status === "failed") {
      setActionResult({
        status: "failed",
        title: "建单失败",
        description: result.message,
      })
    }
  }

  const columns = React.useMemo<ColumnDef<PurchaseOrderListItem>[]>(
    () => [
      {
        id: "document",
        accessorFn: (row) => displayNo(row),
        header: "采购单号",
        meta: { label: "采购单号", width: "reference" },
        cell: ({ row }) => (
          <div
            className="flex min-w-0 items-center gap-2"
            ref={(el) => {
              if (el) {
                rowRefs.current.set(row.original.purchaseOrderId, el)
              } else {
                rowRefs.current.delete(row.original.purchaseOrderId)
              }
            }}
            tabIndex={
              pageRows[focusedIndex]?.purchaseOrderId ===
              row.original.purchaseOrderId
                ? 0
                : -1
            }
            data-focused={
              pageRows[focusedIndex]?.purchaseOrderId ===
              row.original.purchaseOrderId
                ? "true"
                : undefined
            }
            style={
              pageRows[focusedIndex]?.purchaseOrderId ===
              row.original.purchaseOrderId
                ? { backgroundColor: "var(--accent)", borderRadius: "0.375rem" }
                : undefined
            }
          >
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="num px-0"
                  aria-label={`预览 ${displayNo(row.original)}`}
                  onClick={() => setPreviewId(row.original.purchaseOrderId)}
                >
                  {displayNo(row.original)}
                </Button>
                <BusinessStatusBadge
                  context="list"
                  label={row.original.statusLabel}
                  tone={row.original.statusTone}
                />
              </div>
              <div className="truncate text-xs text-muted-foreground">
                {row.original.supplierName}
              </div>
            </div>
            <Badge variant="secondary" className="shrink-0">
              {PURCHASE_TYPE_LABEL[row.original.purchaseType]}
            </Badge>
          </div>
        ),
      },
      {
        id: "source",
        accessorKey: "salesOrderNo",
        header: "来源销售单",
        meta: { label: "来源销售单", width: "reference" },
        cell: ({ row }) => (
          <Link
            href={`/sales/orders/${row.original.salesOrderId}?from=W08&returnTo=${encodeURIComponent(listReturnHref)}`}
            className="num text-sm text-primary underline-offset-2 hover:underline"
            aria-label={`查看来源销售单 ${row.original.salesOrderNo}`}
          >
            {row.original.salesOrderNo}
          </Link>
        ),
      },
      {
        id: "type",
        header: "类型 / 履约",
        meta: { label: "类型与履约责任", width: "default" },
        cell: ({ row }) => (
          <div className="text-xs">
            <div>{PURCHASE_TYPE_LABEL[row.original.purchaseType]}</div>
            <div className="text-muted-foreground">
              {
                FULFILLMENT_RESPONSIBILITY_LABEL[
                  row.original.fulfillmentResponsibility
                ]
              }
            </div>
          </div>
        ),
      },
      {
        id: "tracks",
        header: "进度",
        meta: { label: "多轨进度", width: "tracks" },
        cell: ({ row }) => (
          <StatusTrackSummary
            variant="inline"
            className="flex-nowrap gap-x-2 gap-y-0"
            tracks={[
              {
                id: "pay",
                label: "付款",
                status: {
                  label: row.original.paymentProgress,
                  tone:
                    row.original.paymentProgress === "已付"
                      ? "success"
                      : row.original.paymentProgress === "部分"
                        ? "info"
                        : "neutral",
                },
              },
              {
                id: "ff",
                label: "履约",
                status: {
                  label: row.original.fulfillmentProgress,
                  tone:
                    row.original.paymentGate === "BLOCKED"
                      ? "warning"
                      : row.original.fulfillmentProgress === "完成"
                        ? "success"
                        : "neutral",
                },
              },
            ]}
          />
        ),
      },
      {
        id: "amount",
        accessorKey: "grossAmount",
        header: "含税金额",
        meta: {
          label: "含税金额",
          width: "amount",
          align: "end",
          numeric: true,
        },
        enableSorting: true,
        cell: ({ row }) =>
          row.original.costMasked ? (
            <span className="text-sm text-muted-foreground">•••</span>
          ) : (
            <MoneyValue value={row.original.grossAmount} taxBasis="gross" />
          ),
      },
      {
        id: "paymentTerm",
        header: "付款条件",
        meta: { label: "付款条件", width: "default" },
        cell: ({ row }) => (
          <span className="text-xs text-muted-foreground">
            {row.original.paymentTermLabel}
          </span>
        ),
      },
      {
        id: "owner",
        accessorKey: "ownerName",
        header: "负责人",
        meta: { label: "负责人", width: "default" },
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        cell: ({ row }) => {
          const canReview = row.original.allowedActions.includes("REVIEW")
          const canEdit = row.original.allowedActions.includes("EDIT")
          const canFulfill = row.original.allowedActions.includes("FULFILL")
          const fulfillBlocker = row.original.actionBlockers.find(
            (b) => b.action === "FULFILL"
          )
          return (
            <div className="flex justify-end gap-1">
              <Button
                type="button"
                variant="ghost"
                size="xs"
                onClick={() => setPreviewId(row.original.purchaseOrderId)}
              >
                预览
              </Button>
              <Button
                type="button"
                variant="outline"
                size="xs"
                render={
                  <Link
                    href={`/procurement/orders/${row.original.purchaseOrderId}`}
                  />
                }
              >
                详情
              </Button>
              {canEdit ? (
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  render={
                    <Link
                      href={`/procurement/orders/${row.original.purchaseOrderId}?mode=edit`}
                    />
                  }
                >
                  编辑
                </Button>
              ) : null}
              {canReview ? (
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  render={
                    <Link
                      href={`/procurement/orders/${row.original.purchaseOrderId}?mode=review`}
                    />
                  }
                >
                  去审核
                </Button>
              ) : null}
              {canFulfill ? (
                <Button
                  type="button"
                  variant="outline"
                  size="xs"
                  render={
                    <Link
                      href={`/fulfillment?lane=procurement&scope=mine&purchaseOrderId=${row.original.purchaseOrderId}&from=W08&returnTo=${encodeURIComponent(listReturnHref)}`}
                    />
                  }
                >
                  去交付
                </Button>
              ) : fulfillBlocker ? (
                <Button
                  type="button"
                  variant="ghost"
                  size="xs"
                  disabled
                  title={fulfillBlocker.message}
                >
                  交付已阻断
                </Button>
              ) : null}
            </div>
          )
        },
      },
    ],
    [focusedIndex, listReturnHref, pageRows]
  )

  if (listQuery.isPending) {
    return (
      <PageScaffold density="compact">
        <PageHeader title="采购单" description="正在加载列表…" />
        <div className="h-24 animate-pulse rounded-lg bg-muted" />
        <div className="h-96 animate-pulse rounded-lg bg-muted" />
      </PageScaffold>
    )
  }

  if (listQuery.isError) {
    return (
      <PageScaffold density="compact">
        <PageHeader
          title="采购单"
          description="列表加载失败"
          actions={
            <Button type="button" onClick={() => void listQuery.refetch()}>
              重试
            </Button>
          }
        />
        <BusinessFailureState
          title="列表加载失败"
          error={listQuery.error}
          onRetry={() => void listQuery.refetch()}
        />
      </PageScaffold>
    )
  }

  return (
    <PageScaffold density="compact">
      <PageHeader
        title="采购单"
        breadcrumbs={[
          { id: "proc", label: "采购与履约", href: "/procurement/confirm" },
          { id: "orders", label: "采购单", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={
              listQuery.data?.freshness.updatedAt
                ? new Date(listQuery.data.freshness.updatedAt).toLocaleString(
                    "zh-CN",
                    { hour12: false }
                  )
                : "刚刚"
            }
            dateTime={listQuery.data?.freshness.updatedAt}
            state="fresh"
            label="列表数据"
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: total === 0,
                onClick: () => void exportCsv(),
              },
              {
                actionKey: "create",
                label: "新建采购单",
                icon: PlusIcon,
                mobileVisibility: "hide",
                onClick: () => {
                  setSelectedBasisId(openBases[0]?.basisId ?? "")
                  setCreateOpen(true)
                },
              },
            ]}
          />
        }
      />

      {actionResult ? (
        <FormalActionResult
          status={
            actionResult.status === "failed"
              ? "rejected"
              : actionResult.status === "unknown"
                ? "unknown"
                : "succeeded"
          }
          title={actionResult.title}
          description={actionResult.description}
          reference={actionResult.reference}
          actions={
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() => setActionResult(null)}
            >
              关闭
            </Button>
          }
        />
      ) : null}

      <MetricStrip
        columns={Math.min(4, Math.max(2, metrics.length)) as 2 | 3 | 4}
        aria-label="采购单指标筛选"
      >
        {metrics.map((metric) => (
          <MetricFilterItem
            key={metric.key}
            label={metric.label}
            value={metric.count}
            detail={metric.detail}
            active={
              metric.key !== "pending_create" &&
              metricKey === metric.key
            }
            onClick={() => {
              if (metric.key === "pending_create") {
                setSelectedBasisId(openBases[0]?.basisId ?? "")
                setCreateOpen(true)
                return
              }
              pushUrl({
                metric: metric.key as PurchaseOrderMetricFilter,
                page: 1,
              })
            }}
          />
        ))}
      </MetricStrip>

      <BusinessTableFrame
        title="采购单列表"
        description={
          metricKey === "all" && statusFilter === "all"
            ? "搜索采购单号、供应商或来源销售单；键盘 j/k 移动行，Enter 打开预览，/ 聚焦搜索。"
            : `当前筛选：${PO_METRIC_LABEL[effectiveMetric]} · ${PO_STATUS_FILTER_LABEL[statusFilter]}`
        }
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  data-slot="po-list-search"
                  value={searchDraft}
                  onChange={(event) => {
                    setSearchDraft(event.target.value)
                  }}
                  placeholder="采购单号、供应商、来源销售单"
                  aria-label="搜索采购单"
                />
              </InputGroup>
            }
            filters={
              <OptionCombobox
                value={statusFilter === "all" ? null : statusFilter}
                options={PO_STATUS_FILTER_OPTIONS}
                placeholder="状态：全部"
                size="sm"
                aria-label="按状态筛选"
                inputClassName="w-[9.5rem]"
                onValueChange={(v) => {
                  pushUrl({
                    status: (v as PurchaseOrderStatusFilter | null) ?? "all",
                    page: 1,
                  })
                }}
              />
            }
            actions={
              <div className="flex items-center gap-2">
                <span className="text-xs text-muted-foreground" aria-live="polite">
                  共 {total.toLocaleString("zh-CN")} 条
                </span>
                {hasActiveFilters ? (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={clearFilters}
                  >
                    清除筛选
                  </Button>
                ) : null}
              </div>
            }
          />
        }
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.purchaseOrderId}
            rowCount={total}
            pagination={pagination}
            onPaginationChange={(next) => {
              pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
            }}
            sorting={sorting}
            onSortingChange={(next) => {
              const nextSort = next[0]
              pushUrl({
                sort: nextSort
                  ? `${nextSort.id}:${nextSort.desc ? "desc" : "asc"}`
                  : undefined,
                page: 1,
              })
            }}
            layout="flush"
            density="compact"
            loading={listQuery.isFetching}
            defaultColumnPinning={{ left: ["document"], right: ["actions"] }}
            onRowPreview={(row) => setPreviewId(row.purchaseOrderId)}
            onRowOpen={(row) => setPreviewId(row.purchaseOrderId)}
            errorState={
              <BusinessFailureState
                kind="system"
                title="列表加载失败"
                description="未能加载采购单列表，请重试；若持续失败可稍后再来。"
                onRetry={() => void listQuery.refetch()}
              />
            }
            emptyTitle={
              hasActiveFilters
                ? "没有符合条件的采购单"
                : undefined
            }
            emptyDescription="当前筛选没有匹配的采购单，可调整或清除筛选后重试。"
            emptyAction={
              <div className="flex flex-wrap gap-2">
                {hasActiveFilters ? (
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="rounded-lg shadow-none"
                    onClick={clearFilters}
                  >
                    清除筛选
                  </Button>
                ) : (
                  <Button
                    type="button"
                    variant="secondary"
                    size="sm"
                    className="rounded-lg shadow-none"
                    render={<Link href="/procurement/confirm" />}
                  >
                    去采购二次确认
                  </Button>
                )}
              </div>
            }
          />
        }
      />

      <QuickPreviewSheet
        open={previewId != null}
        onOpenChange={(open) => {
          if (!open) {
            const id = previewId
            setPreviewId(null)
            if (id) {
              requestAnimationFrame(() => {
                rowRefs.current.get(id)?.focus()
              })
            }
          }
        }}
        size="detail"
        title={
          previewQuery.data?.header.supplierSnapshot ?? "采购单预览"
        }
        identity={
          previewQuery.data ? (
            <span className="num">
              {previewQuery.data.identity.purchaseNo ??
                previewQuery.data.identity.draftLabel}{" "}
              ·{" "}
              {previewQuery.data.identity.revisionNo
                ? `v${previewQuery.data.identity.revisionNo}`
                : "草稿"}{" "}
              · {previewQuery.data.header.salesOrderNo}
            </span>
          ) : null
        }
        summary={
          previewQuery.data ? (
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="preview"
                label={previewQuery.data.identity.statusLabel}
                tone={previewQuery.data.identity.statusTone}
              />
              <Badge variant="secondary">
                {
                  PURCHASE_TYPE_LABEL[
                    previewQuery.data.header.purchaseType
                  ]
                }
              </Badge>
            </div>
          ) : null
        }
        footer={
          previewQuery.data ? (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => {
                  const id = previewId
                  setPreviewId(null)
                  if (id) {
                    requestAnimationFrame(() => {
                      rowRefs.current.get(id)?.focus()
                    })
                  }
                }}
              >
                关闭
              </Button>
              <Button
                type="button"
                variant="outline"
                render={
                  <Link
                    href={`/procurement/orders/${previewQuery.data.identity.purchaseOrderId}`}
                  />
                }
              >
                查看详情
              </Button>
              {previewQuery.data.allowedActions.includes("EDIT") ? (
                <Button
                  type="button"
                  render={
                    <Link
                      href={`/procurement/orders/${previewQuery.data.identity.purchaseOrderId}?mode=edit`}
                    />
                  }
                >
                  去编辑
                </Button>
              ) : null}
              {previewQuery.data.allowedActions.includes("REVIEW") ? (
                <Button
                  type="button"
                  render={
                    <Link
                      href={`/procurement/orders/${previewQuery.data.identity.purchaseOrderId}?mode=review`}
                    />
                  }
                >
                  去审核
                </Button>
              ) : null}
              {previewQuery.data.allowedActions.includes("FULFILL") ? (
                <Button
                  type="button"
                  variant="outline"
                  render={
                    <Link
                      href={`/fulfillment?lane=procurement&scope=mine&purchaseOrderId=${previewQuery.data.identity.purchaseOrderId}&from=W08&returnTo=${encodeURIComponent(listReturnHref)}`}
                    />
                  }
                >
                  去交付
                </Button>
              ) : previewQuery.data.actionBlockers.some(
                  (b) => b.action === "FULFILL"
                ) ? (
                <Button type="button" variant="outline" disabled>
                  履约已阻断
                </Button>
              ) : null}
            </>
          ) : null
        }
      >
        {previewQuery.isPending ? (
          <div className="p-5 text-sm text-muted-foreground">加载预览…</div>
        ) : previewQuery.data ? (
          <PurchaseOrderPreviewPanel order={previewQuery.data} />
        ) : (
          <div className="p-5 text-sm text-muted-foreground">无法加载预览</div>
        )}
      </QuickPreviewSheet>

      <Dialog open={createOpen} onOpenChange={setCreateOpen}>
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>从采购创建依据建单</DialogTitle>
            <DialogDescription>
              仅使用采购二次确认产生的创建依据，无需额外建单任务。
              同一依据上的拆单维度已固定，不可跨销售单或跨供应商合并。
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3">
            {openBases.length === 0 && !basisFromUrl ? (
              <p className="text-sm text-muted-foreground">
                当前没有可消费的创建依据。请先在采购二次确认完成确认。
              </p>
            ) : (
              <label className="grid gap-1.5 text-sm">
                <span>选择创建依据</span>
                <OptionCombobox
                  className="w-full"
                  value={selectedBasisId}
                  onValueChange={(v) =>
                    setSelectedBasisId(v ?? selectedBasisId)
                  }
                  options={[
                    ...(basisFromUrl &&
                    !openBases.some((b) => b.basisId === basisFromUrl)
                      ? [
                          {
                            value: basisFromUrl,
                            label: "来自采购二次确认的固定结果",
                          },
                        ]
                      : []),
                    ...openBases.map((basis) => ({
                      value: basis.basisId,
                      label: `${basis.salesOrderNo} · ${basis.supplierName} · ${PURCHASE_TYPE_LABEL[basis.purchaseType]} · 估 ${basis.estimatedGross}`,
                    })),
                  ]}
                  allowClear={false}
                  aria-label="选择创建依据"
                  placeholder="选择创建依据"
                />
              </label>
            )}
            {selectedBasisId
              ? (() => {
                  const basis = openBases.find(
                    (b) => b.basisId === selectedBasisId
                  )
                  if (!basis) return null
                  return (
                    <div className={`${surfaceInsetClassName} p-3 text-xs text-muted-foreground`}>
                      <p className="font-medium text-foreground">
                        拆单键（不可混拼）
                      </p>
                      <ul className="mt-1 list-disc space-y-0.5 pl-4">
                        <li>销售单 {basis.salesOrderNo}</li>
                        <li>供应商 {basis.supplierName}</li>
                        <li>
                          类型 {PURCHASE_TYPE_LABEL[basis.purchaseType]} · 履约{" "}
                          {
                            FULFILLMENT_RESPONSIBILITY_LABEL[
                              basis.fulfillmentResponsibility
                            ]
                          }
                        </li>
                        <li>付款 {basis.paymentTermLabel}</li>
                        <li>{basis.lines.length} 条已确认分行</li>
                      </ul>
                    </div>
                  )
                })()
              : null}
          </div>
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              取消
            </DialogClose>
            <Button
              type="button"
              disabled={!selectedBasisId || createMutation.isPending}
              onClick={() => void handleCreate()}
            >
              {createMutation.isPending ? "创建中…" : "创建草稿并打开"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </PageScaffold>
  )
}
