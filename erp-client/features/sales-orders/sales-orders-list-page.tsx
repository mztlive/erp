"use client"

import * as React from "react"
import {
  DownloadIcon,
  FilterIcon,
  PlusIcon,
  PrinterIcon,
  SearchIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
  BusinessObjectRef,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MoneyValue,
  PageActions,
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
import {
  MOCK_SALES_ORDERS,
  NATURE_LABEL,
  OWNER_LABEL,
} from "@/features/sales-orders/mock-data"
import { SalesOrderPaperDialog } from "@/features/sales-orders/sales-order-paper-dialog"
import { SalesOrderPreviewPanel } from "@/features/sales-orders/sales-order-preview-panel"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

type NatureFilter = "all" | "physical_service" | "card_voucher"

function matchesSearch(order: SalesOrderListItem, query: string) {
  if (!query) return true
  const q = query.trim().toLowerCase()
  return (
    order.documentNumber.toLowerCase().includes(q) ||
    order.customerName.toLowerCase().includes(q) ||
    order.contractNumber.toLowerCase().includes(q) ||
    order.ownerName.toLowerCase().includes(q)
  )
}

export function SalesOrdersListPage() {
  const [search, setSearch] = React.useState("")
  const [natureFilter, setNatureFilter] =
    React.useState<NatureFilter>("all")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [paperId, setPaperId] = React.useState<string | null>(null)
  const [openNotice, setOpenNotice] = React.useState<string | null>(null)

  const filtered = React.useMemo(() => {
    return MOCK_SALES_ORDERS.filter((order) => {
      if (natureFilter !== "all" && order.nature !== natureFilter) {
        return false
      }
      return matchesSearch(order, search)
    })
  }, [natureFilter, search])

  React.useEffect(() => {
    setPagination((prev) => ({ ...prev, pageIndex: 0 }))
  }, [search, natureFilter])

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return filtered.slice(start, start + pagination.pageSize)
  }, [filtered, pagination.pageIndex, pagination.pageSize])

  const previewOrder = React.useMemo(
    () => MOCK_SALES_ORDERS.find((item) => item.id === previewId) ?? null,
    [previewId]
  )

  const paperOrder = React.useMemo(
    () => MOCK_SALES_ORDERS.find((item) => item.id === paperId) ?? null,
    [paperId]
  )

  const metrics = React.useMemo(() => {
    const all = MOCK_SALES_ORDERS
    return {
      total: all.length,
      pendingConfirm: all.filter(
        (o) => o.primaryStatus.label === "待二次确认"
      ).length,
      inFulfillment: all.filter((o) => o.primaryStatus.label === "履约中")
        .length,
      cardVoucher: all.filter((o) => o.nature === "card_voucher").length,
    }
  }, [])

  const openCenter = React.useCallback((documentNumber: string) => {
    setOpenNotice(documentNumber)
    setPreviewId(null)
  }, [])

  const openPaper = React.useCallback((id: string) => {
    setPaperId(id)
  }, [])

  const columns = React.useMemo<ColumnDef<SalesOrderListItem>[]>(
    () => [
      {
        id: "document",
        accessorKey: "documentNumber",
        header: "销售单",
        meta: { label: "销售单", width: "reference" },
        cell: ({ row }) => (
          <BusinessObjectRef
            objectType={NATURE_LABEL[row.original.nature]}
            stableNumber={row.original.documentNumber}
            title={row.original.customerName}
            status={row.original.primaryStatus}
            onOpen={() => setPreviewId(row.original.id)}
            openLabel={`预览 ${row.original.documentNumber}`}
          />
        ),
      },
      {
        id: "contract",
        accessorKey: "contractNumber",
        header: "合同",
        meta: { label: "合同", width: "default" },
        cell: ({ row }) => (
          <span className="num text-sm text-foreground">
            {row.original.contractNumber}
          </span>
        ),
      },
      {
        id: "ownerSystem",
        header: "主责",
        meta: { label: "主责系统", width: "status" },
        cell: ({ row }) => (
          <Badge
            variant={
              row.original.ownerSystem === "erp" ? "info" : "secondary"
            }
          >
            {OWNER_LABEL[row.original.ownerSystem]}
          </Badge>
        ),
      },
      {
        id: "tracks",
        header: "进度",
        meta: { label: "多轨进度", width: "tracks" },
        cell: ({ row }) => (
          <StatusTrackSummary
            variant="inline"
            tracks={[
              {
                id: "fulfillment",
                label: "履约",
                status: row.original.fulfillment,
              },
              {
                id: "collection",
                label: "回款",
                status: row.original.collection,
              },
              {
                id: "invoicing",
                label: "开票",
                status: row.original.invoicing,
              },
            ]}
          />
        ),
      },
      {
        id: "amount",
        accessorKey: "amountGross",
        header: "成交金额",
        meta: {
          label: "成交金额",
          width: "amount",
          align: "end",
          numeric: true,
        },
        cell: ({ row }) => (
          <MoneyValue value={row.original.amountGross} taxBasis="gross" />
        ),
      },
      {
        id: "owner",
        accessorKey: "ownerName",
        header: "负责人",
        meta: { label: "负责人", width: "default" },
      },
      {
        id: "submittedAt",
        accessorKey: "submittedAt",
        header: "提交时间",
        meta: { label: "提交时间", width: "default", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm text-muted-foreground">
            {row.original.submittedAt}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "status", align: "end" },
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            <Button
              type="button"
              variant="ghost"
              size="xs"
              onClick={() => setPreviewId(row.original.id)}
            >
              预览
            </Button>
            <Button
              type="button"
              variant="outline"
              size="xs"
              onClick={() => openCenter(row.original.documentNumber)}
            >
              中心
            </Button>
          </div>
        ),
      },
    ],
    [openCenter]
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-5 p-4 md:p-6">
      <PageHeader
        title="销售单"
        description="单击行打开半屏详情预览（左右分栏读主事实）；纸质投影用 PaperDocument；对象中心仅用于履约/票款等作业。"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "orders", label: "销售单", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt="刚刚"
            dateTime={new Date().toISOString()}
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
              },
              {
                actionKey: "create",
                label: "新建销售单",
                icon: PlusIcon,
              },
            ]}
          />
        }
      />

      <div className="grid gap-px overflow-hidden rounded-lg border border-grid bg-grid sm:grid-cols-2 lg:grid-cols-4">
        <MetricCard label="全部销售单" value={metrics.total} />
        <MetricCard
          label="待二次确认"
          value={metrics.pendingConfirm}
          hint="采购闸门"
        />
        <MetricCard
          label="履约中"
          value={metrics.inFulfillment}
          hint="含长期卡券"
        />
        <MetricCard
          label="卡券销售单"
          value={metrics.cardVoucher}
          hint="一期主责商城"
        />
      </div>

      {openNotice ? (
        <div
          role="status"
          className="rounded-lg border border-info-border bg-info-soft px-4 py-3 text-sm text-info-soft-foreground"
        >
          演示环境：对象中心「{openNotice}」尚未接入。核对与打印请用侧栏预览 /
          纸质投影。
          <Button
            type="button"
            variant="link"
            size="xs"
            className="ml-2"
            onClick={() => setOpenNotice(null)}
          >
            关闭
          </Button>
        </div>
      ) : null}

      <BusinessTableFrame
        title="销售单列表"
        description="服务端分页形态预览；当前使用本地演示数据。"
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  value={search}
                  onChange={(event) => setSearch(event.target.value)}
                  placeholder="单号、客户、合同、负责人"
                  aria-label="搜索销售单"
                />
              </InputGroup>
            }
            filters={
              <>
                <ToggleGroup
                  value={[natureFilter]}
                  onValueChange={(values) => {
                    const next = values[0] as NatureFilter | undefined
                    setNatureFilter(next ?? "all")
                  }}
                  variant="outline"
                  size="sm"
                  spacing={0}
                >
                  <ToggleGroupItem value="all">全部</ToggleGroupItem>
                  <ToggleGroupItem value="physical_service">
                    实物与服务
                  </ToggleGroupItem>
                  <ToggleGroupItem value="card_voucher">卡券</ToggleGroupItem>
                </ToggleGroup>
                <Button type="button" variant="outline" size="sm">
                  <FilterIcon data-icon="inline-start" aria-hidden="true" />
                  高级筛选
                </Button>
              </>
            }
            actions={
              <span className="text-xs text-muted-foreground">
                共 {filtered.length.toLocaleString("zh-CN")} 条
              </span>
            }
          />
        }
        table={
          <DataTable
            data={pageRows}
            columns={columns}
            getRowId={(row) => row.id}
            rowCount={filtered.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="comfortable"
            onRowPreview={(row) => setPreviewId(row.id)}
            onRowOpen={(row) => setPreviewId(row.id)}
          />
        }
      />

      <QuickPreviewSheet
        open={previewOrder != null}
        onOpenChange={(open) => {
          if (!open) setPreviewId(null)
        }}
        size="detail"
        title={previewOrder?.customerName ?? "销售单预览"}
        identity={
          previewOrder ? (
            <span className="num">
              {previewOrder.documentNumber} · v{previewOrder.version} ·{" "}
              {NATURE_LABEL[previewOrder.nature]}
            </span>
          ) : null
        }
        summary={
          previewOrder ? (
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="preview"
                {...previewOrder.primaryStatus}
              />
              <Badge
                variant={
                  previewOrder.ownerSystem === "erp" ? "info" : "secondary"
                }
              >
                {OWNER_LABEL[previewOrder.ownerSystem]}
              </Badge>
              <span className="text-xs text-muted-foreground">
                半屏详情 · 左右分栏
              </span>
            </div>
          ) : null
        }
        footer={
          previewOrder ? (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => setPreviewId(null)}
              >
                关闭
              </Button>
              <Button
                type="button"
                variant="outline"
                onClick={() => openPaper(previewOrder.id)}
              >
                <PrinterIcon data-icon="inline-start" aria-hidden="true" />
                纸质预览
              </Button>
              <Button
                type="button"
                onClick={() => openCenter(previewOrder.documentNumber)}
              >
                打开中心
              </Button>
            </>
          ) : null
        }
      >
        {previewOrder ? (
          <SalesOrderPreviewPanel order={previewOrder} />
        ) : null}
      </QuickPreviewSheet>

      <SalesOrderPaperDialog
        order={paperOrder}
        open={paperOrder != null}
        onOpenChange={(open) => {
          if (!open) setPaperId(null)
        }}
      />
    </div>
  )
}

function MetricCard({
  label,
  value,
  hint,
}: {
  label: string
  value: number
  hint?: string
}) {
  return (
    <div className="bg-card p-4">
      <div className="text-sm text-muted-foreground">{label}</div>
      <div className="num mt-1 text-2xl font-semibold tracking-tight text-foreground">
        {value.toLocaleString("zh-CN")}
      </div>
      {hint ? (
        <div className="mt-1 text-xs text-muted-foreground">{hint}</div>
      ) : null}
    </div>
  )
}
