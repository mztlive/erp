"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import {
  DownloadIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
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
  QuickPreviewSheet,
  StatusTrackSummary,
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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { PurchaseOrderPreviewPanel } from "@/features/purchase-orders/purchase-order-preview-panel"
import {
  useCreateFromBasisMutation,
  useCreationBasesQuery,
  usePurchaseOrderCenterQuery,
  usePurchaseOrdersQuery,
} from "@/features/purchase-orders/queries"
import type {
  PurchaseOrderListItem,
  ViewerRole,
} from "@/features/purchase-orders/types"
import {
  FULFILLMENT_RESPONSIBILITY_LABEL,
  PURCHASE_TYPE_LABEL,
} from "@/features/purchase-orders/types"

type StatusFilter =
  | "all"
  | "DRAFT"
  | "PENDING_REVIEW"
  | "EFFECTIVE"
  | "PARTIAL"
  | "COMPLETED"

type MetricKey =
  | "all"
  | "pending_create"
  | "draft"
  | "review"
  | "fulfill"
  | "gate_blocked"

function displayNo(row: PurchaseOrderListItem) {
  return row.purchaseNo ?? row.draftLabel ?? row.purchaseOrderId
}

export function PurchaseOrdersListPage() {
  const router = useRouter()
  const searchParams = useSearchParams()
  const basisFromUrl = searchParams.get("basisId")
  const [viewerRole, setViewerRole] =
    React.useState<ViewerRole>("procurement")
  const listQuery = usePurchaseOrdersQuery(viewerRole)
  const basesQuery = useCreationBasesQuery()
  const createMutation = useCreateFromBasisMutation()

  const allRows = listQuery.data?.rows ?? []
  const metrics = listQuery.data?.metrics ?? []

  const [search, setSearch] = React.useState("")
  const [statusFilter, setStatusFilter] = React.useState<StatusFilter>("all")
  const [metricKey, setMetricKey] = React.useState<MetricKey>("all")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
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

  const resetPagination = React.useCallback(() => {
    setPagination((previous) =>
      previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
    )
  }, [])

  const filtered = React.useMemo(() => {
    const q = search.trim().toLowerCase()
    return allRows.filter((row) => {
      if (statusFilter !== "all" && row.status !== statusFilter) return false
      if (metricKey === "draft" && row.status !== "DRAFT") return false
      if (metricKey === "review" && row.status !== "PENDING_REVIEW") return false
      if (
        metricKey === "fulfill" &&
        !(
          (row.status === "EFFECTIVE" || row.status === "PARTIAL") &&
          row.fulfillmentProgress !== "完成"
        )
      ) {
        return false
      }
      if (metricKey === "gate_blocked" && row.paymentGate !== "BLOCKED") {
        return false
      }
      if (metricKey === "pending_create") {
        // metric opens create dialog, list stays unfiltered by this alone
      }
      if (!q) return true
      const hay = [
        row.purchaseNo,
        row.draftLabel,
        row.supplierName,
        row.salesOrderNo,
        row.ownerName,
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase()
      return hay.includes(q)
    })
  }, [allRows, metricKey, search, statusFilter])

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return filtered.slice(start, start + pagination.pageSize)
  }, [filtered, pagination.pageIndex, pagination.pageSize])

  const previewQuery = usePurchaseOrderCenterQuery(
    previewId ?? "",
    viewerRole
  )

  React.useEffect(() => {
    setFocusedIndex(0)
  }, [filtered.length, metricKey, search, statusFilter])

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
      } else if (event.key === "Escape" && previewId) {
        event.preventDefault()
        const id = previewId
        setPreviewId(null)
        requestAnimationFrame(() => {
          rowRefs.current.get(id)?.focus()
        })
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [focusedIndex, pageRows, previewId])

  const exportCsv = React.useCallback(() => {
    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const rows = filtered.map((row) =>
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
      ...rows,
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
      description: `已下载当前筛选 ${filtered.length} 条（已按角色遮罩成本字段）。`,
      reference: `EXP-W08-${filtered.length}`,
    })
  }, [filtered])

  const openBases = basesQuery.data?.filter((b) => !b.consumed) ?? []

  React.useEffect(() => {
    if (!basisFromUrl) return
    // W07/W05 携带创建依据：打开建单 Dialog，不要求 work_item
    setSelectedBasisId(basisFromUrl)
    setCreateOpen(true)
  }, [basisFromUrl])

  const handleCreate = async () => {
    if (!selectedBasisId) return
    const result = await createMutation.mutateAsync({
      basisId: selectedBasisId,
      idempotencyKey: `create-basis-${selectedBasisId}-${Date.now()}`,
    })
    if (result.status === "succeeded") {
      setCreateOpen(false)
      setActionResult({
        status: "succeeded",
        title: "已创建采购草稿",
        description: `${result.data.draftLabel} · 已使用创建依据 ${selectedBasisId}`,
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
          <span className="num text-sm">{row.original.salesOrderNo}</span>
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
                中心
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
                    <Link href="/fulfillment?lane=procurement&scope=mine" />
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
                  履约
                </Button>
              ) : null}
            </div>
          )
        },
      },
    ],
    [focusedIndex, pageRows]
  )

  if (listQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="采购单" description="正在加载列表…" />
        <div className="h-24 animate-pulse rounded-2xl bg-muted" />
        <div className="h-96 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (listQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="采购单"
          description="列表加载失败"
          actions={
            <Button type="button" onClick={() => void listQuery.refetch()}>
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
        title="采购单"
        breadcrumbs={[
          { id: "proc", label: "采购与履约", href: "/procurement/confirm" },
          { id: "orders", label: "采购单", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt="刚刚"
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
                disabled: filtered.length === 0,
                onClick: exportCsv,
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

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-muted-foreground">演示角色视图</span>
        <ToggleGroup
          value={[viewerRole]}
          onValueChange={(values) => {
            const next = values[0] as ViewerRole | undefined
            if (next) setViewerRole(next)
          }}
          variant="outline"
          size="sm"
          spacing={0}
        >
          <ToggleGroupItem value="procurement">采购</ToggleGroupItem>
          <ToggleGroupItem value="finance">财务</ToggleGroupItem>
          <ToggleGroupItem value="sales">销售</ToggleGroupItem>
          <ToggleGroupItem value="warehouse">仓储</ToggleGroupItem>
        </ToggleGroup>
        {(viewerRole === "sales" || viewerRole === "warehouse") && (
          <span className="text-xs text-muted-foreground">
            成本金额已打码
          </span>
        )}
      </div>

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
            active={metricKey === metric.key}
            onClick={() => {
              if (metric.key === "pending_create") {
                setSelectedBasisId(openBases[0]?.basisId ?? "")
                setCreateOpen(true)
                return
              }
              setMetricKey(metric.key as MetricKey)
              resetPagination()
            }}
          />
        ))}
      </MetricStrip>

      <BusinessTableFrame
        title="采购单列表"
        description={
          metricKey === "all" && statusFilter === "all"
            ? "紧凑布局；采购单号与行级操作列固定。键盘 j/k 移动，Enter 预览，/ 搜索。"
            : `当前筛选：${metricKey} · ${statusFilter}`
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
                  value={search}
                  onChange={(event) => {
                    setSearch(event.target.value)
                    resetPagination()
                  }}
                  placeholder="采购单号、供应商、来源销售单"
                  aria-label="搜索采购单"
                />
              </InputGroup>
            }
            filters={
              <ToggleGroup
                value={[statusFilter]}
                onValueChange={(values) => {
                  const next = (values[0] as StatusFilter | undefined) ?? "all"
                  setStatusFilter(next)
                  resetPagination()
                }}
                variant="outline"
                size="sm"
                spacing={0}
              >
                <ToggleGroupItem value="all">全部</ToggleGroupItem>
                <ToggleGroupItem value="DRAFT">草稿</ToggleGroupItem>
                <ToggleGroupItem value="PENDING_REVIEW">待审核</ToggleGroupItem>
                <ToggleGroupItem value="EFFECTIVE">已生效</ToggleGroupItem>
                <ToggleGroupItem value="PARTIAL">部分执行</ToggleGroupItem>
                <ToggleGroupItem value="COMPLETED">已完成</ToggleGroupItem>
              </ToggleGroup>
            }
            actions={
              <span className="text-xs text-muted-foreground" aria-live="polite">
                共 {filtered.length.toLocaleString("zh-CN")} 条
              </span>
            }
          />
        }
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.purchaseOrderId}
            rowCount={filtered.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
            defaultColumnPinning={{ left: ["document"], right: ["actions"] }}
            onRowPreview={(row) => setPreviewId(row.purchaseOrderId)}
            onRowOpen={(row) => setPreviewId(row.purchaseOrderId)}
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
                    <Link href="/fulfillment?lane=procurement&scope=mine" />
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
                            label: `${basisFromUrl} · 来自采购二次确认的固定结果`,
                          },
                        ]
                      : []),
                    ...openBases.map((basis) => ({
                      value: basis.basisId,
                      label: `${basis.basisId} · ${basis.salesOrderNo} · ${basis.supplierName} · ${PURCHASE_TYPE_LABEL[basis.purchaseType]} · 估 ${basis.estimatedGross}`,
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
                    <div className="rounded-lg border border-border bg-muted/40 p-3 text-xs text-muted-foreground">
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
    </div>
  )
}
