"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  DownloadIcon,
  FilterIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react"
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/react-table"

import {
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
  OptionCombobox,
  PageActions,
  PageHeader,
  PageScaffold,
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
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import {
  NATURE_LABEL,
  ORIGIN_LABEL,
} from "@/features/sales-orders/labels"
import { SalesOrderPaperDialog } from "@/features/sales-orders/sales-order-paper-dialog"
import {
  salesOrderStatusLabel,
  salesOrderSummaryLabels,
  SALES_ORDER_STATUS_OPTIONS,
} from "@/features/sales-orders/filter-orders"
import {
  fetchSalesOrders,
  type SalesOrdersListQuery,
} from "@/features/sales-orders/api"
import {
  useCreateSalesOrderExportJobMutation,
  useSalesOrdersQuery,
} from "@/features/sales-orders/queries"
import type { SalesOrderListItem } from "@/features/sales-orders/types"
import {
  buildSalesOrdersSearchParams,
  parseSalesOrdersSearchParams,
  type SalesOrdersUrlState,
} from "@/features/sales-orders/url-state"

const SORT_COLUMN_TO_FIELD: Record<
  string,
  NonNullable<SalesOrdersListQuery["sortBy"]>
> = {
  document: "documentNumber",
  contract: "contractNumber",
  amount: "amountGross",
  owner: "ownerName",
  submittedAt: "submittedAt",
}

const EMPTY_METRICS = {
  total: 0,
  pending: 0,
  inProgress: 0,
  pendingCollection: 0,
  fulfillmentException: 0,
  mallCollab: 0,
}

export function SalesOrdersListPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const url = React.useMemo(
    () => parseSalesOrdersSearchParams(searchParams),
    [searchParams]
  )

  const pushUrl = React.useCallback(
    (patch: Partial<SalesOrdersUrlState>) => {
      const next = { ...url, ...patch }
      const qs = buildSalesOrdersSearchParams(next)
      router.replace(`${pathname}${qs}`, { scroll: false })
    },
    [pathname, router, url]
  )

  const query = React.useMemo<SalesOrdersListQuery>(
    () => ({
      page: url.page,
      pageSize: url.pageSize,
      search: url.search,
      nature: url.nature,
      summary: url.summary,
      origin: url.origin,
      status: url.status,
      sortBy: url.sort ? SORT_COLUMN_TO_FIELD[url.sort] : undefined,
      sortDir: url.dir,
    }),
    [url]
  )

  const ordersQuery = useSalesOrdersQuery(query)
  const exportMutation = useCreateSalesOrderExportJobMutation()
  const items = React.useMemo(
    () => ordersQuery.data?.items ?? [],
    [ordersQuery.data?.items]
  )
  const total = ordersQuery.data?.total ?? 0
  const metrics = ordersQuery.data?.metrics ?? EMPTY_METRICS

  const [searchDraft, setSearchDraft] = React.useState(url.search ?? "")
  const [paperId, setPaperId] = React.useState<string | null>(null)
  const [exportJob, setExportJob] = React.useState<{
    jobId: string
    rowCount: number
    downloadLabel: string
    exportedAt: string
    fileName: string
  } | null>(null)
  const [focusedIndex, setFocusedIndex] = React.useState(0)
  const rowRefs = React.useRef<Map<string, HTMLElement>>(new Map())

  const openPaperPreview = React.useCallback((id: string) => {
    setPaperId(id)
  }, [])

  React.useEffect(() => {
    setSearchDraft(url.search ?? "")
  }, [url.search])

  React.useEffect(() => {
    setFocusedIndex(0)
  }, [
    url.nature,
    url.origin,
    url.page,
    url.search,
    url.status,
    url.summary,
    items.length,
  ])

  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchDraft.trim() === (url.search ?? "")) return
      pushUrl({ search: searchDraft.trim() || undefined, page: 1 })
    }, 300)
    return () => globalThis.clearTimeout(handle)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- pushUrl 以当前 URL 快照为准
  }, [searchDraft])

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
          .querySelector<HTMLInputElement>('[data-slot="so-list-search"]')
          ?.focus()
        return
      }

      if (items.length === 0) return

      if (event.key === "j" || event.key === "ArrowDown") {
        event.preventDefault()
        setFocusedIndex((i) => Math.min(items.length - 1, i + 1))
      } else if (event.key === "k" || event.key === "ArrowUp") {
        event.preventDefault()
        setFocusedIndex((i) => Math.max(0, i - 1))
      } else if (event.key === "Enter") {
        event.preventDefault()
        const row = items[focusedIndex]
        if (row) openPaperPreview(row.id)
      } else if (event.key === "Escape" && paperId) {
        event.preventDefault()
        const id = paperId
        setPaperId(null)
        requestAnimationFrame(() => {
          rowRefs.current.get(id)?.focus()
        })
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [focusedIndex, items, openPaperPreview, paperId])

  const pagination = React.useMemo<PaginationState>(
    () => ({
      pageIndex: Math.max(0, url.page - 1),
      pageSize: url.pageSize,
    }),
    [url.page, url.pageSize]
  )

  const handlePaginationChange = React.useCallback(
    (next: PaginationState) => {
      pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
    },
    [pushUrl]
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

  const paperOrder = React.useMemo(
    () => items.find((item) => item.id === paperId) ?? null,
    [items, paperId]
  )

  const exportCsv = React.useCallback(async () => {
    if (total === 0) return
    const job = await exportMutation.mutateAsync({ rowCount: total })
    const all = await fetchSalesOrders({ ...query, page: 1, pageSize: total })
    const now = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    const datePart = `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}`
    const timePart = `${pad(now.getHours())}${pad(now.getMinutes())}`
    const fileName = `销售单列表_${datePart}_${timePart}.csv`
    setExportJob({
      jobId: job.jobId,
      rowCount: job.rowCount,
      downloadLabel: fileName,
      exportedAt: now.toISOString(),
      fileName,
    })

    const quote = (value: string) => `"${value.replaceAll('"', '""')}"`
    const rows = all.items.map((order) =>
      [
        order.documentNumber,
        order.customerName,
        order.contractNumber,
        NATURE_LABEL[order.nature],
        order.primaryStatus.label,
        ORIGIN_LABEL[order.originSystem],
        order.amountGross,
        order.ownerName,
        order.submittedAt,
      ]
        .map((value) => quote(String(value)))
        .join(",")
    )
    const csv = [
      `# 导出时间 ${now.toLocaleString("zh-CN")}；仅包含当前筛选结果，金额以列表页最新数据为准。`,
      "销售单号,客户,合同,业务性质,状态,创建来源,成交金额（含税）,负责人,提交时间",
      ...rows,
    ].join("\n")
    const url = URL.createObjectURL(
      new Blob(["\uFEFF", csv], { type: "text/csv;charset=utf-8" })
    )
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    anchor.click()
    URL.revokeObjectURL(url)
  }, [exportMutation, query, total])

  const advancedActive =
    url.origin !== "all" || url.status !== "all"

  const filtersActive =
    Boolean(url.search) ||
    url.nature !== "all" ||
    url.summary !== "all" ||
    url.origin !== "all" ||
    url.status !== "all"

  const clearFilters = React.useCallback(() => {
    setSearchDraft("")
    pushUrl({
      search: undefined,
      nature: "all",
      summary: "all",
      origin: "all",
      status: "all",
      page: 1,
    })
  }, [pushUrl])

  const columns = React.useMemo<ColumnDef<SalesOrderListItem>[]>(
    () => [
      {
        id: "document",
        accessorKey: "documentNumber",
        header: "销售单",
        meta: { label: "销售单", width: "reference" },
        cell: ({ row }) => (
          <div
            className="flex min-w-0 items-center gap-2"
            ref={(el) => {
              if (el) rowRefs.current.set(row.original.id, el)
              else rowRefs.current.delete(row.original.id)
            }}
            tabIndex={
              items[focusedIndex]?.id === row.original.id ? 0 : -1
            }
            data-focused={
              items[focusedIndex]?.id === row.original.id
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
                  aria-label={`预览 ${row.original.documentNumber}`}
                  onClick={() => openPaperPreview(row.original.id)}
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
        enableSorting: false,
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
          <span className="num text-sm">{row.original.contractNumber}</span>
        ),
      },
      {
        id: "tracks",
        header: "进度",
        meta: { label: "多轨进度", width: "tracks" },
        enableSorting: false,
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
        enableSorting: false,
        cell: ({ row }) => (
          <div className="flex justify-end gap-1">
            <Button
              type="button"
              variant="outline"
              size="xs"
              render={<Link href={`/sales/orders/${row.original.id}`} />}
            >
              查看详情
            </Button>
          </div>
        ),
      },
    ],
    [focusedIndex, items, openPaperPreview]
  )

  return (
    <PageScaffold density="compact">
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
                  ? ordersQuery.data.queriedAt.slice(11, 16)
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
          />
        }
        actions={
          <PageActions
            actions={[
              {
                actionKey: "create",
                label: "新建销售单",
                icon: PlusIcon,
                render: <Link href="/sales/orders?mode=create" />,
              },
              {
                actionKey: "export",
                label: "导出",
                icon: DownloadIcon,
                variant: "outline",
                mobileVisibility: "hide",
                disabled: total === 0 || exportMutation.isPending,
                onClick: () => {
                  void exportCsv()
                },
              },
            ]}
          />
        }
      />

      {exportJob ? (
        <FormalActionResult
          status="succeeded"
          title="导出完成"
          description={`已生成 CSV 文件，共 ${exportJob.rowCount} 行，仅包含当前筛选结果；导出后金额与状态以列表页最新数据为准。`}
          facts={[
            {
              label: "文件",
              value: exportJob.fileName,
            },
            {
              label: "行数",
              value: String(exportJob.rowCount),
            },
            {
              label: "导出时间",
              value: new Date(exportJob.exportedAt).toLocaleString("zh-CN"),
            },
          ]}
        />
      ) : null}

      <MetricStrip columns={5} aria-label="销售单快速筛选">
        <MetricFilterItem
          label="待处理"
          value={metrics.pending}
          detail="确认 / 审批 / 驳回"
          active={url.summary === "pending"}
          onClick={() => {
            pushUrl({ summary: "pending", page: 1 })
          }}
        />
        <MetricFilterItem
          label="进行中"
          value={metrics.inProgress}
          detail="履约中 / 已生效"
          active={url.summary === "inProgress"}
          onClick={() => {
            pushUrl({ summary: "inProgress", page: 1 })
          }}
        />
        <MetricFilterItem
          label="待收款"
          value={metrics.pendingCollection}
          detail="未收 / 部分 / 待复核"
          active={url.summary === "pendingCollection"}
          onClick={() => {
            pushUrl({ summary: "pendingCollection", page: 1 })
          }}
        />
        <MetricFilterItem
          label="履约异常"
          value={metrics.fulfillmentException}
          detail="部分履约等"
          active={url.summary === "fulfillmentException"}
          onClick={() => {
            pushUrl({ summary: "fulfillmentException", page: 1 })
          }}
        />
        <MetricFilterItem
          label="商城协同"
          value={metrics.mallCollab}
          detail="商城开单或票款复核"
          active={url.summary === "mallCollab"}
          onClick={() => {
            pushUrl({ summary: "mallCollab", page: 1 })
          }}
        />
      </MetricStrip>

      <BusinessTableFrame
        title="销售单列表"
        description={
          url.summary === "all" &&
          url.origin === "all" &&
          url.status === "all" &&
          url.nature === "all" &&
          !url.search
            ? "按提交时间查看当前业务范围内的销售单；业务性质与创建来源展示。"
            : `当前筛选：${salesOrderSummaryLabels(url.summary)}${
                url.nature !== "all"
                  ? ` · ${NATURE_LABEL[url.nature]}`
                  : ""
              }${
                url.origin !== "all"
                  ? ` · ${ORIGIN_LABEL[url.origin]}`
                  : ""
              }${
                url.status !== "all"
                  ? ` · ${salesOrderStatusLabel(url.status)}`
                  : ""
              }${url.search ? ` · 关键词「${url.search}」` : ""}`
        }
        toolbar={
          <ListToolbar
            search={
              <InputGroup>
                <InputGroupAddon>
                  <SearchIcon aria-hidden="true" />
                </InputGroupAddon>
                <InputGroupInput
                  data-slot="so-list-search"
                  value={searchDraft}
                  onChange={(event) => {
                    setSearchDraft(event.target.value)
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      pushUrl({
                        search: searchDraft.trim() || undefined,
                        page: 1,
                      })
                    }
                  }}
                  placeholder="单号、客户、合同、负责人"
                  aria-label="搜索销售单"
                />
              </InputGroup>
            }
            filters={
              <>
                <ToggleGroup
                  value={[url.nature]}
                  onValueChange={(values) => {
                    const next = values[0] as
                      | SalesOrdersUrlState["nature"]
                      | undefined
                    pushUrl({ nature: next ?? "all", page: 1 })
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
                        创建来源与主状态筛选。
                      </p>
                    </div>
                    <label className="grid gap-1.5 text-sm">
                      <span>创建来源</span>
                      <OptionCombobox
                        value={url.origin}
                        onValueChange={(v) => {
                          pushUrl({
                            origin:
                              (v ?? "all") as SalesOrdersUrlState["origin"],
                            page: 1,
                          })
                        }}
                        options={[
                          { value: "all", label: "全部来源" },
                          { value: "erp", label: "创建于 ERP" },
                          { value: "mall", label: "创建于商城" },
                        ]}
                        allowClear={false}
                        aria-label="创建来源"
                        placeholder="创建来源"
                      />
                    </label>
                    <label className="grid gap-1.5 text-sm">
                      <span>主状态</span>
                      <OptionCombobox
                        value={url.status}
                        onValueChange={(v) => {
                          pushUrl({
                            status:
                              (v ?? "all") as SalesOrdersUrlState["status"],
                            page: 1,
                          })
                        }}
                        options={[
                          { value: "all", label: "全部状态" },
                          ...SALES_ORDER_STATUS_OPTIONS.map((option) => ({
                            value: option.value,
                            label: option.label,
                          })),
                        ]}
                        allowClear={false}
                        aria-label="主状态"
                        placeholder="主状态"
                      />
                    </label>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      disabled={!advancedActive}
                      onClick={() => {
                        pushUrl({ origin: "all", status: "all", page: 1 })
                      }}
                    >
                      清除高级筛选
                    </Button>
                  </PopoverContent>
                </Popover>
              </>
            }
            actions={
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span aria-live="polite">
                  共 {total.toLocaleString("zh-CN")} 条
                </span>
                <span className="hidden md:inline" aria-hidden="true">
                  ·
                </span>
                <span className="hidden md:inline">
                  / 聚焦搜索 · ↑↓ 选择行 · Enter 预览
                </span>
                {filtersActive ? (
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
          ordersQuery.isError ? (
            <BusinessFailureState
              kind="system"
              title="销售单列表加载失败"
              description="暂时拿不到销售单数据，请重试；失败不影响已保存的单据。"
              onRetry={() => {
                void ordersQuery.refetch()
              }}
            />
          ) : items.length === 0 ? (
            <div className="flex flex-col items-center gap-2 px-4 py-10 text-center">
              <p className="text-sm text-muted-foreground">
                {filtersActive ? "当前筛选没有结果" : "还没有销售单"}
              </p>
              <p className="text-xs text-muted-foreground">
                {filtersActive
                  ? "换一个关键词或清除筛选后再试。"
                  : "当前业务范围内还没有销售单，可新建第一张单。"}
              </p>
              <div className="flex flex-wrap justify-center gap-2 pt-1">
                {filtersActive ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={clearFilters}
                  >
                    清除筛选
                  </Button>
                ) : (
                  <Button
                    type="button"
                    size="sm"
                    render={<Link href="/sales/orders?mode=create" />}
                  >
                    <PlusIcon data-icon="inline-start" aria-hidden="true" />
                    新建销售单
                  </Button>
                )}
              </div>
            </div>
          ) : (
            <DataTable
              data={items}
              columns={columns}
              getRowId={(row) => row.id}
              rowCount={total}
              sorting={sorting}
              onSortingChange={handleSortingChange}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              loading={ordersQuery.isPending}
              layout="flush"
              density="compact"
              defaultColumnPinning={{ left: ["document"], right: ["actions"] }}
              onRowPreview={(row) => openPaperPreview(row.id)}
              onRowOpen={(row) => openPaperPreview(row.id)}
            />
          )
        }
      />

      <SalesOrderPaperDialog
        order={paperOrder}
        open={paperOrder != null}
        onOpenChange={(open) => {
          if (!open) setPaperId(null)
        }}
      />
    </PageScaffold>
  )
}
