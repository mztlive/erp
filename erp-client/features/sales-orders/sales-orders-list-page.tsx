"use client"

import * as React from "react"
import Link from "next/link"
import {
  DownloadIcon,
  FilterIcon,
  PrinterIcon,
  SearchIcon,
} from "lucide-react"
import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
  BackgroundJobProgress,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FormalActionResult,
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
  NATURE_LABEL,
  ORIGIN_LABEL,
  OWNER_LABEL,
} from "@/mock/sales-orders"
import { SalesOrderPaperDialog } from "@/features/sales-orders/sales-order-paper-dialog"
import { SalesOrderPreviewPanel } from "@/features/sales-orders/sales-order-preview-panel"
import {
  computeSalesOrderMetrics,
  filterSalesOrders,
  salesOrderSummaryLabels,
  type SalesOrderNatureFilter,
  type SalesOrderOriginFilter,
  type SalesOrderOwnerFilter,
  type SalesOrderStatusFilter,
  type SalesOrderSummaryFilter,
} from "@/features/sales-orders/filter-orders"
import {
  useCreateSalesOrderExportJobMutation,
  useSalesOrdersQuery,
} from "@/features/sales-orders/queries"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import { PERMISSION_VERSION } from "@/features/sales-orders/api"

type NatureFilter = SalesOrderNatureFilter
type SummaryFilter = SalesOrderSummaryFilter
type OwnerFilter = SalesOrderOwnerFilter
type OriginFilter = SalesOrderOriginFilter
type StatusFilter = SalesOrderStatusFilter

const EMPTY_SALES_ORDERS: readonly SalesOrderListItem[] = []

export function SalesOrdersListPage({
  initialSearch = "",
  initialNature = "all",
}: {
  initialSearch?: string
  initialNature?: NatureFilter
}) {
  const ordersQuery = useSalesOrdersQuery()
  const exportMutation = useCreateSalesOrderExportJobMutation()
  const allOrders = ordersQuery.data?.rows ?? EMPTY_SALES_ORDERS
  const [search, setSearch] = React.useState(initialSearch)
  const [natureFilter, setNatureFilter] =
    React.useState<NatureFilter>(initialNature)
  const [summaryFilter, setSummaryFilter] =
    React.useState<SummaryFilter>("all")
  const [ownerFilter, setOwnerFilter] = React.useState<OwnerFilter>("all")
  const [originFilter, setOriginFilter] = React.useState<OriginFilter>("all")
  const [statusFilter, setStatusFilter] = React.useState<StatusFilter>("all")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [paperId, setPaperId] = React.useState<string | null>(null)
  const [exportJob, setExportJob] = React.useState<{
    jobId: string
    rowCount: number
    permissionVersion: string
    downloadLabel: string
  } | null>(null)

  const resetPagination = React.useCallback(() => {
    setPagination((previous) =>
      previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
    )
  }, [])

  const filtered = React.useMemo(
    () =>
      filterSalesOrders(allOrders, {
        search,
        natureFilter,
        summaryFilter,
        ownerFilter,
        originFilter,
        statusFilter,
      }),
    [
      allOrders,
      natureFilter,
      ownerFilter,
      originFilter,
      search,
      statusFilter,
      summaryFilter,
    ]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return filtered.slice(start, start + pagination.pageSize)
  }, [filtered, pagination.pageIndex, pagination.pageSize])

  const previewOrder = React.useMemo(
    () => allOrders.find((item) => item.id === previewId) ?? null,
    [allOrders, previewId]
  )

  const paperOrder = React.useMemo(
    () => allOrders.find((item) => item.id === paperId) ?? null,
    [allOrders, paperId]
  )

  const metrics = React.useMemo(
    () => computeSalesOrderMetrics(allOrders),
    [allOrders]
  )

  const openPaper = React.useCallback((id: string) => {
    setPaperId(id)
  }, [])

  /** 诚实客户端导出：当前筛选结果 + 权限版本审计标签，非服务端后台全量。 */
  const exportCsv = React.useCallback(async () => {
    const job = await exportMutation.mutateAsync({
      rowCount: filtered.length,
    })
    setExportJob({
      jobId: job.jobId,
      rowCount: job.rowCount,
      permissionVersion: job.permissionVersion,
      downloadLabel: job.downloadLabel,
    })

    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const rows = filtered.map((order) =>
      [
        order.documentNumber,
        order.customerName,
        order.contractNumber,
        NATURE_LABEL[order.nature],
        order.primaryStatus.label,
        ORIGIN_LABEL[order.originSystem],
        OWNER_LABEL[order.ownerSystem],
        order.amountGross,
        order.ownerName,
        order.submittedAt,
      ]
        .map((value) => quote(String(value)))
        .join(",")
    )
    const csv = [
      `# permissionVersion=${job.permissionVersion}; source=client-filtered; audit=${job.jobId}`,
      "销售单号,客户,合同,业务性质,状态,创建来源,当前主责,成交金额（含税）,负责人,提交时间",
      ...rows,
    ].join("\n")
    const url = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = `销售单列表_${job.jobId}.csv`
    anchor.click()
    URL.revokeObjectURL(url)
  }, [exportMutation, filtered])

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
                <BusinessStatusBadge
                  context="list"
                  {...row.original.primaryStatus}
                />
              </div>
              <div className="truncate text-xs text-muted-foreground">
                {row.original.customerName}
              </div>
            </div>
          </div>
        ),
      },
      {
        id: "nature",
        header: "业务性质",
        meta: { label: "业务性质", width: "status" },
        cell: ({ row }) => (
          <Badge variant="secondary">
            {NATURE_LABEL[row.original.nature]}
          </Badge>
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
        id: "originSystem",
        header: "创建来源",
        meta: { label: "创建来源", width: "status" },
        cell: ({ row }) => (
          <Badge variant="outline">
            {ORIGIN_LABEL[row.original.originSystem]}
          </Badge>
        ),
      },
      {
        id: "ownerSystem",
        header: "当前主责",
        meta: { label: "当前主责", width: "status" },
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
              render={<Link href={`/sales/orders/${row.original.id}`} />}
            >
              打开
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

  const advancedActive =
    ownerFilter !== "all" ||
    originFilter !== "all" ||
    statusFilter !== "all"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
      <PageHeader
        title="销售单"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "orders", label: "销售单", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={
              ordersQuery.isError
                ? "查询失败"
                : ordersQuery.data
                  ? "刚刚"
                  : "正在查询"
            }
            dateTime={ordersQuery.data?.queriedAt}
            state={
              ordersQuery.isError
                ? "failed"
                : ordersQuery.isFetching
                  ? "syncing"
                  : ordersQuery.data
                    ? "fresh"
                    : "unknown"
            }
            label={`列表 · 权限 ${PERMISSION_VERSION}`}
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
                disabled: filtered.length === 0 || exportMutation.isPending,
                onClick: () => {
                  void exportCsv()
                },
              },
            ]}
          />
        }
      />

      {exportJob ? (
        <div className="space-y-2">
          <FormalActionResult
            status="succeeded"
            title="导出任务已完成（客户端筛选快照）"
            description={`共 ${exportJob.rowCount} 行，受当前筛选与权限版本约束；目标页打开后应重新查询金额和状态。非服务端全量后台导出。`}
            reference={exportJob.jobId}
            facts={[
              {
                label: "权限版本",
                value: exportJob.permissionVersion,
              },
              {
                label: "文件",
                value: exportJob.downloadLabel,
              },
            ]}
          />
          <BackgroundJobProgress
            mode="all-or-nothing"
            status="succeeded"
            label="导出作业"
            description={`审计标签 ${exportJob.jobId} · 客户端筛选快照`}
            total={exportJob.rowCount}
            completed={exportJob.rowCount}
            succeeded={exportJob.rowCount}
          />
        </div>
      ) : null}

      <MetricStrip columns={5} aria-label="销售单快速筛选">
        <MetricFilterItem
          label="待处理"
          value={metrics.pending}
          detail="确认 / 审批 / 驳回"
          active={summaryFilter === "pending"}
          onClick={() => {
            setSummaryFilter("pending")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="进行中"
          value={metrics.inProgress}
          detail="履约中 / 已生效"
          active={summaryFilter === "inProgress"}
          onClick={() => {
            setSummaryFilter("inProgress")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="待收款"
          value={metrics.pendingCollection}
          detail="未收 / 部分 / 待复核"
          active={summaryFilter === "pendingCollection"}
          onClick={() => {
            setSummaryFilter("pendingCollection")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="履约异常"
          value={metrics.fulfillmentException}
          detail="部分履约等"
          active={summaryFilter === "fulfillmentException"}
          onClick={() => {
            setSummaryFilter("fulfillmentException")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="商城协同"
          value={metrics.mallCollab}
          detail="主责或票款复核"
          active={summaryFilter === "mallCollab"}
          onClick={() => {
            setSummaryFilter("mallCollab")
            resetPagination()
          }}
        />
      </MetricStrip>

      <BusinessTableFrame
        title="销售单列表"
        description={
          summaryFilter === "all" && !advancedActive
            ? "按提交时间查看当前业务范围内的销售单；业务性质与主责分列。"
            : `当前筛选：${salesOrderSummaryLabels(summaryFilter)}${
                advancedActive
                  ? ` · ${originFilter === "all" ? "全部来源" : ORIGIN_LABEL[originFilter]} · ${ownerFilter === "all" ? "全部主责" : OWNER_LABEL[ownerFilter]} · ${statusFilter === "all" ? "全部状态" : statusFilter}`
                  : ""
              }`
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
                    render={
                      <Button type="button" variant="outline" size="sm" />
                    }
                  >
                    <FilterIcon data-icon="inline-start" aria-hidden="true" />
                    高级筛选
                    {advancedActive ? (
                      <Badge variant="info">已启用</Badge>
                    ) : null}
                  </PopoverTrigger>
                  <PopoverContent align="end" className="w-80">
                    <div>
                      <div className="font-medium">高级筛选</div>
                      <p className="mt-1 text-xs text-muted-foreground">
                        创建来源与当前主责分列筛选；主状态使用服务端枚举文案。
                      </p>
                    </div>
                    <label className="grid gap-1.5 text-sm">
                      <span>创建来源</span>
                      <NativeSelect
                        className="w-full"
                        value={originFilter}
                        onChange={(event) => {
                          setOriginFilter(event.target.value as OriginFilter)
                          resetPagination()
                        }}
                      >
                        <NativeSelectOption value="all">
                          全部来源
                        </NativeSelectOption>
                        <NativeSelectOption value="erp">
                          创建于 ERP
                        </NativeSelectOption>
                        <NativeSelectOption value="mall">
                          创建于商城
                        </NativeSelectOption>
                      </NativeSelect>
                    </label>
                    <label className="grid gap-1.5 text-sm">
                      <span>当前主责</span>
                      <NativeSelect
                        className="w-full"
                        value={ownerFilter}
                        onChange={(event) => {
                          setOwnerFilter(event.target.value as OwnerFilter)
                          resetPagination()
                        }}
                      >
                        <NativeSelectOption value="all">
                          全部主责
                        </NativeSelectOption>
                        <NativeSelectOption value="erp">
                          主责 ERP
                        </NativeSelectOption>
                        <NativeSelectOption value="mall">
                          主责商城
                        </NativeSelectOption>
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
                        <NativeSelectOption value="all">
                          全部状态
                        </NativeSelectOption>
                        <NativeSelectOption value="待二次确认">
                          待二次确认
                        </NativeSelectOption>
                        <NativeSelectOption value="待销售处理">
                          待销售处理
                        </NativeSelectOption>
                        <NativeSelectOption value="待销售领导审批">
                          待销售领导审批
                        </NativeSelectOption>
                        <NativeSelectOption value="待运营审批">
                          待运营审批
                        </NativeSelectOption>
                        <NativeSelectOption value="履约中">
                          履约中
                        </NativeSelectOption>
                        <NativeSelectOption value="已生效">
                          已生效
                        </NativeSelectOption>
                        <NativeSelectOption value="已关闭">
                          已关闭
                        </NativeSelectOption>
                        <NativeSelectOption value="草稿">草稿</NativeSelectOption>
                        <NativeSelectOption value="已作废">
                          已作废
                        </NativeSelectOption>
                      </NativeSelect>
                    </label>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      disabled={!advancedActive}
                      onClick={() => {
                        setOwnerFilter("all")
                        setOriginFilter("all")
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
              <Badge variant="outline">
                {ORIGIN_LABEL[previewOrder.originSystem]}
              </Badge>
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
                render={<Link href={`/sales/orders/${previewOrder.id}`} />}
              >
                打开对象中心
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
