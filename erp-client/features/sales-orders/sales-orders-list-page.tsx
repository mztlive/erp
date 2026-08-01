"use client"

import * as React from "react"
import {
  DownloadIcon,
  FilterIcon,
  PrinterIcon,
  SearchIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
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
  NativeSelect,
  NativeSelectOption,
} from "@/components/ui/native-select"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  MOCK_SALES_ORDERS,
  NATURE_LABEL,
  OWNER_LABEL,
} from "@/mock/sales-orders"
import { SalesOrderPaperDialog } from "@/features/sales-orders/sales-order-paper-dialog"
import { SalesOrderPreviewPanel } from "@/features/sales-orders/sales-order-preview-panel"
import type { SalesOrderListItem } from "@/features/sales-orders/types"

type NatureFilter = "all" | "physical_service" | "card_voucher"
type SummaryFilter = "all" | "pendingConfirm" | "inFulfillment" | "cardVoucher"
type OwnerFilter = "all" | SalesOrderListItem["ownerSystem"]
type StatusFilter = "all" | "待二次确认" | "履约中" | "已关闭"

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

export function SalesOrdersListPage({ initialSearch = "" }: { initialSearch?: string }) {
  const [search, setSearch] = React.useState(initialSearch)
  const [natureFilter, setNatureFilter] =
    React.useState<NatureFilter>("all")
  const [summaryFilter, setSummaryFilter] =
    React.useState<SummaryFilter>("all")
  const [ownerFilter, setOwnerFilter] = React.useState<OwnerFilter>("all")
  const [statusFilter, setStatusFilter] = React.useState<StatusFilter>("all")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [paperId, setPaperId] = React.useState<string | null>(null)

  const resetPagination = React.useCallback(() => {
    setPagination((previous) =>
      previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
    )
  }, [])

  const filtered = React.useMemo(() => {
    return MOCK_SALES_ORDERS.filter((order) => {
      if (natureFilter !== "all" && order.nature !== natureFilter) {
        return false
      }
      if (ownerFilter !== "all" && order.ownerSystem !== ownerFilter) {
        return false
      }
      if (statusFilter !== "all" && order.primaryStatus.label !== statusFilter) {
        return false
      }
      if (
        summaryFilter === "pendingConfirm" &&
        order.primaryStatus.label !== "待二次确认"
      ) {
        return false
      }
      if (
        summaryFilter === "inFulfillment" &&
        order.primaryStatus.label !== "履约中"
      ) {
        return false
      }
      if (summaryFilter === "cardVoucher" && order.nature !== "card_voucher") {
        return false
      }
      return matchesSearch(order, search)
    })
  }, [natureFilter, ownerFilter, search, statusFilter, summaryFilter])

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

  const openPaper = React.useCallback((id: string) => {
    setPaperId(id)
  }, [])

  const exportCsv = React.useCallback(() => {
    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const rows = filtered.map((order) =>
      [
        order.documentNumber,
        order.customerName,
        order.contractNumber,
        NATURE_LABEL[order.nature],
        order.primaryStatus.label,
        OWNER_LABEL[order.ownerSystem],
        order.amountGross,
        order.ownerName,
        order.submittedAt,
      ]
        .map((value) => quote(String(value)))
        .join(",")
    )
    const csv = [
      "销售单号,客户,合同,业务性质,状态,主责系统,成交金额（含税）,负责人,提交时间",
      ...rows,
    ].join("\n")
    const url = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = "销售单列表.csv"
    anchor.click()
    URL.revokeObjectURL(url)
  }, [filtered])

  const columns = React.useMemo<ColumnDef<SalesOrderListItem>[]>(
    () => [
      {
        id: "document",
        accessorKey: "documentNumber",
        header: "销售单",
        meta: { label: "销售单", width: "reference" },
        cell: ({ row }) => (
          <div className="flex min-w-0 items-center gap-2">
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <Button
                  type="button"
                  variant="link"
                  size="xs"
                  className="num px-0"
                  aria-label={`预览 ${row.original.documentNumber}`}
                  onClick={() => setPreviewId(row.original.id)}
                >
                  {row.original.documentNumber}
                </Button>
                <BusinessStatusBadge context="list" {...row.original.primaryStatus} />
              </div>
              <div className="truncate text-xs text-muted-foreground">
                {row.original.customerName}
              </div>
            </div>
            <Badge variant="secondary" className="shrink-0">
              {NATURE_LABEL[row.original.nature]}
            </Badge>
          </div>
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
            className="flex-nowrap gap-x-2 gap-y-0"
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
        meta: { label: "操作", width: "default", align: "end" },
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
              onClick={() => openPaper(row.original.id)}
            >
              打印
            </Button>
          </div>
        ),
      },
    ],
    [openPaper]
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        title="销售单"
        description="集中核对销售单状态、履约、票款与明细；单击任一行可在当前列表查看完整摘要。"
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
                disabled: filtered.length === 0,
                onClick: exportCsv,
              },
            ]}
          />
        }
      />

      <MetricStrip columns={4} aria-label="销售单快速筛选">
        <MetricFilterItem
          label="全部销售单"
          value={metrics.total}
          detail="当前业务范围"
          active={summaryFilter === "all"}
          onClick={() => {
            setSummaryFilter("all")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="待二次确认"
          value={metrics.pendingConfirm}
          detail="采购确认闸门"
          active={summaryFilter === "pendingConfirm"}
          onClick={() => {
            setSummaryFilter("pendingConfirm")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="履约中"
          value={metrics.inFulfillment}
          detail="含长期卡券"
          active={summaryFilter === "inFulfillment"}
          onClick={() => {
            setSummaryFilter("inFulfillment")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="卡券销售单"
          value={metrics.cardVoucher}
          detail="主责系统可能为商城"
          active={summaryFilter === "cardVoucher"}
          onClick={() => {
            setSummaryFilter("cardVoucher")
            resetPagination()
          }}
        />
      </MetricStrip>

      <BusinessTableFrame
        title="销售单列表"
        description={
          summaryFilter === "all" && ownerFilter === "all" && statusFilter === "all"
            ? "按提交时间查看当前业务范围内的销售单。"
            : `当前筛选：${summaryFilter === "all" ? "全部指标" : summaryFilter === "pendingConfirm" ? "待二次确认" : summaryFilter === "inFulfillment" ? "履约中" : "卡券销售单"} · ${ownerFilter === "all" ? "全部主责" : OWNER_LABEL[ownerFilter]} · ${statusFilter === "all" ? "全部状态" : statusFilter}`
        }
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  value={search}
                  onChange={(event) => {
                    setSearch(event.target.value)
                    resetPagination()
                  }}
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
                    resetPagination()
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
                <Popover>
                  <PopoverTrigger
                    render={<Button type="button" variant="outline" size="sm" />}
                  >
                    <FilterIcon data-icon="inline-start" aria-hidden="true" />
                    高级筛选
                    {ownerFilter !== "all" || statusFilter !== "all" ? (
                      <Badge variant="info">已启用</Badge>
                    ) : null}
                  </PopoverTrigger>
                  <PopoverContent align="end" className="w-80">
                    <div>
                      <div className="font-medium">高级筛选</div>
                      <p className="mt-1 text-xs text-muted-foreground">
                        组合主责系统与主状态缩小结果范围。
                      </p>
                    </div>
                    <label className="grid gap-1.5 text-sm">
                      <span>主责系统</span>
                      <NativeSelect
                        className="w-full"
                        value={ownerFilter}
                        onChange={(event) => {
                          setOwnerFilter(event.target.value as OwnerFilter)
                          resetPagination()
                        }}
                      >
                        <NativeSelectOption value="all">全部主责</NativeSelectOption>
                        <NativeSelectOption value="erp">主责 ERP</NativeSelectOption>
                        <NativeSelectOption value="mall">主责商城</NativeSelectOption>
                      </NativeSelect>
                    </label>
                    <label className="grid gap-1.5 text-sm">
                      <span>主状态</span>
                      <NativeSelect
                        className="w-full"
                        value={statusFilter}
                        onChange={(event) => {
                          setStatusFilter(event.target.value as StatusFilter)
                          resetPagination()
                        }}
                      >
                        <NativeSelectOption value="all">全部状态</NativeSelectOption>
                        <NativeSelectOption value="待二次确认">待二次确认</NativeSelectOption>
                        <NativeSelectOption value="履约中">履约中</NativeSelectOption>
                        <NativeSelectOption value="已关闭">已关闭</NativeSelectOption>
                      </NativeSelect>
                    </label>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      disabled={ownerFilter === "all" && statusFilter === "all"}
                      onClick={() => {
                        setOwnerFilter("all")
                        setStatusFilter("all")
                        resetPagination()
                      }}
                    >
                      清除高级筛选
                    </Button>
                  </PopoverContent>
                </Popover>
              </>
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
            getRowId={(row) => row.id}
            rowCount={filtered.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
            defaultColumnPinning={{ left: ["document"], right: ["actions"] }}
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
              <span className="text-xs text-muted-foreground">销售单详情</span>
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
