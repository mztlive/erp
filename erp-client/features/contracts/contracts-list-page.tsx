"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import {
  DownloadIcon,
  FileUpIcon,
  PrinterIcon,
  SearchIcon,
} from "lucide-react"
import type {
  ColumnDef,
  PaginationState,
  SortingState,
} from "@tanstack/react-table"

import {
  BusinessEmptyState,
  BusinessFailureState,
  BusinessStatusBadge,
  BusinessTableFrame,
  DataFreshness,
  DataTable,
  FilterChip,
  FormalActionResult,
  ListToolbar,
  MetricFilterItem,
  MetricStrip,
  PageActions,
  PageHeader,
  PageScaffold,
  QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import { ContractPreviewPanel } from "@/features/contracts/contract-preview-panel"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import {
  computeContractMetrics,
  contractMetricLabel,
  filterContracts,
  type ContractMetricFilter,
} from "@/features/contracts/filter-contracts"
import {
  useContractCenterQuery,
  useContractsQuery,
  useCreateContractExportJobMutation,
} from "@/features/contracts/queries"
import type {
  ContractExportJob,
  ContractListRow,
  UploadContractPdfResult,
} from "@/features/contracts/types"
import { contractOwnerLabel } from "@/features/contracts/types"
import { createUrlStateCodec } from "@/lib/url-state"

/** URL 契约：q（旧 search 别名只读兼容）/metric/page/pageSize/sort/dir/customerId。 */
const CONTRACT_METRIC_VALUES: ContractMetricFilter[] = [
  "all",
  "effective",
  "expiring_30d",
  "expired",
  "terminated",
]

const CONTRACTS_URL_FIELDS = [
  { key: "q", type: "string", trim: true, aliases: ["search"] as const },
  {
    key: "metric",
    type: "enum",
    values: CONTRACT_METRIC_VALUES,
    defaultValue: "all",
  },
  { key: "page", type: "number", defaultValue: 1 },
  { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
  { key: "sort", type: "string" },
  { key: "dir", type: "enum", values: ["asc", "desc"] as const },
  { key: "customerId", type: "string" },
] as const

type ContractsUrlState = {
  q?: string
  metric: ContractMetricFilter
  page: number
  pageSize: number
  sort?: string
  dir?: "asc" | "desc"
  customerId?: string
}

const contractsUrlCodec = createUrlStateCodec<ContractsUrlState>(
  CONTRACTS_URL_FIELDS
)

/** 表头排序列 → 全量排序键（对整表排序后再分页，杜绝「当前页伪排序」）。 */
function sortRows(
  rows: readonly ContractListRow[],
  sorting: SortingState
): ContractListRow[] {
  const sorted = [...rows]
  if (sorting.length === 0) {
    // 默认：将到期优先，再按有效期止升序（与列表描述文案一致）。
    return sorted.sort((a, b) => {
      if (a.expiringWithin30Days !== b.expiringWithin30Days) {
        return a.expiringWithin30Days ? -1 : 1
      }
      return a.validTo.localeCompare(b.validTo)
    })
  }
  const { id, desc } = sorting[0]
  const dir = desc ? -1 : 1
  return sorted.sort((a, b) => {
    let cmp = 0
    switch (id) {
      case "contractNo":
        cmp = a.contractNo.localeCompare(b.contractNo)
        break
      case "customer":
        cmp = a.customer.displayName.localeCompare(b.customer.displayName)
        break
      case "settlement":
        cmp = a.settlementParty.displayName.localeCompare(
          b.settlementParty.displayName
        )
        break
      case "validity":
        cmp = a.validTo.localeCompare(b.validTo)
        break
      case "revision":
        cmp = a.revisionNo - b.revisionNo
        break
      case "sales":
        cmp = a.salesOrderCount - b.salesOrderCount
        break
      case "owner":
        cmp = a.ownerLabel.localeCompare(b.ownerLabel)
        break
    }
    return cmp * dir
  })
}

export function ContractsListPage() {
  const router = useRouter()
  const pathname = usePathname()
  const searchParams = useSearchParams()
  const contractsQuery = useContractsQuery()
  const allRows = React.useMemo(
    () => contractsQuery.data ?? [],
    [contractsQuery.data]
  )

  const url = React.useMemo(() => contractsUrlCodec.parse(searchParams), [
    searchParams,
  ])
  const { q, metric, page, pageSize, sort, dir, customerId } = url

  const [searchDraft, setSearchDraft] = React.useState(q ?? "")
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [paperId, setPaperId] = React.useState<string | null>(null)
  const [uploadOpen, setUploadOpen] = React.useState(Boolean(customerId))
  const [exportJob, setExportJob] = React.useState<ContractExportJob | null>(
    null
  )
  const [actionResult, setActionResult] = React.useState<{
    status: "succeeded" | "blocked"
    title: string
    description: string
    facts?: Array<{ label: string; value: string }>
    nextHref?: string
  } | null>(null)

  const exportMutation = useCreateContractExportJobMutation()

  /** URL-first：筛选/分页/排序全部写 URL，浏览器后退与刷新一致。 */
  const pushUrl = React.useCallback(
    (patch: Partial<ContractsUrlState>) => {
      const next = { ...url, ...patch }
      router.replace(`${pathname}${contractsUrlCodec.build(next)}`, {
        scroll: false,
      })
    },
    [pathname, router, url]
  )

  React.useEffect(() => {
    setSearchDraft(q ?? "")
  }, [q])

  // 防抖即时搜索（300ms）+ Enter 兜底；写 URL 并回第 1 页。
  React.useEffect(() => {
    const handle = globalThis.setTimeout(() => {
      if (searchDraft.trim() === (q ?? "")) return
      pushUrl({ q: searchDraft.trim() || undefined, page: 1 })
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
        return
      }
      if (event.key === "/" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault()
        document
          .querySelector<HTMLInputElement>('[data-slot="contracts-search"]')
          ?.focus()
      }
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [])

  const filtered = React.useMemo(() => {
    let rows = filterContracts(allRows, {
      search: q ?? "",
      metricKey: metric,
      statusFilter: "all",
    })
    if (customerId) {
      rows = rows.filter((r) => r.customer.customerId === customerId)
    }
    return rows
  }, [allRows, customerId, metric, q])

  const sorting = React.useMemo<SortingState>(
    () => (sort ? [{ id: sort, desc: dir === "desc" }] : []),
    [dir, sort]
  )

  const sorted = React.useMemo(
    () => sortRows(filtered, sorting),
    [filtered, sorting]
  )

  const pagination = React.useMemo<PaginationState>(
    () => ({ pageIndex: Math.max(0, page - 1), pageSize }),
    [page, pageSize]
  )

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return sorted.slice(start, start + pagination.pageSize)
  }, [pagination.pageIndex, pagination.pageSize, sorted])

  const metrics = React.useMemo(
    () => computeContractMetrics(allRows),
    [allRows]
  )

  /** 客户锁定来自 URL customerId：界面给出可移除 chip 与清除入口。 */
  const lockedCustomer = React.useMemo(() => {
    if (!customerId) return null
    return (
      allRows.find((r) => r.customer.customerId === customerId)?.customer ??
      null
    )
  }, [allRows, customerId])

  const previewRow = React.useMemo(
    () => allRows.find((item) => item.contractId === previewId) ?? null,
    [allRows, previewId]
  )

  const previewDetailQuery = useContractCenterQuery(previewId ?? "")
  const paperDetailQuery = useContractCenterQuery(paperId ?? "")

  const handleUploadSuccess = React.useCallback(
    (result: UploadContractPdfResult) => {
      setActionResult({
        status: "succeeded",
        title: "合同 PDF 已归档",
        description: "已形成可追溯的合同版本，可直接选择用于新建销售单。",
        facts: [
          { label: "合同号", value: result.contractNo },
          { label: "修订", value: `v${result.revisionNo}` },
          { label: "文件", value: result.fileName },
          {
            label: "上传时间",
            value: result.uploadedAt.slice(0, 19).replace("T", " "),
          },
          { label: "下一步", value: "查看详情核对或新建销售单" },
        ],
        nextHref: `/sales/contracts/${result.contractId}`,
      })
    },
    []
  )

  const filterSnapshotLabel = React.useMemo(() => {
    const parts = [
      `指标=${contractMetricLabel(metric)}`,
      (q ?? "").trim() ? `搜索=${(q ?? "").trim()}` : "搜索=空",
      lockedCustomer ? `客户=${lockedCustomer.displayName}` : null,
    ].filter(Boolean)
    return parts.join(" · ")
  }, [lockedCustomer, metric, q])

  const handleExport = React.useCallback(async () => {
    if (filtered.length === 0) return
    const job = await exportMutation.mutateAsync({
      rowCount: filtered.length,
      filterSnapshotLabel,
    })
    setExportJob(job)
    setActionResult({
      status: "succeeded",
      title: "导出完成",
      description: "已生成 CSV 文件，内容按当前筛选生成；下载时将重新校验权限。",
      facts: [
        { label: "筛选结果", value: job.filterSnapshotLabel },
        { label: "行数", value: String(job.rowCount) },
        { label: "文件", value: job.downloadLabel },
      ],
    })
  }, [exportMutation, filterSnapshotLabel, filtered.length])

  const columns = React.useMemo<ColumnDef<ContractListRow>[]>(
    () => [
      {
        id: "contractNo",
        accessorKey: "contractNo",
        header: "合同编号",
        meta: { label: "合同编号", width: "reference" },
        cell: ({ row }) => (
          <div className="min-w-0">
            <Button
              type="button"
              variant="link"
              size="xs"
              className="num px-0"
              aria-label={`打开合同 ${row.original.contractNo}`}
              render={
                <Link href={`/sales/contracts/${row.original.contractId}`} />
              }
            >
              {row.original.contractNo}
            </Button>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.customer.displayName}
              {" · "}
              <span className="num">{row.original.customer.customerNo}</span>
            </div>
          </div>
        ),
      },
      {
        id: "settlement",
        accessorFn: (row) => row.settlementParty.displayName,
        header: "结算主体",
        meta: { label: "结算主体", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm">
            {row.original.settlementParty.displayName}
          </span>
        ),
      },
      {
        id: "validity",
        header: "有效期",
        meta: { label: "有效期", width: "default", numeric: true },
        cell: ({ row }) => (
          <div className="num text-sm">
            <div>
              {row.original.validFrom} ~ {row.original.validTo}
            </div>
            {row.original.expiringWithin30Days ? (
              <div className="text-xs text-warning-foreground">将到期</div>
            ) : null}
          </div>
        ),
      },
      {
        id: "status",
        header: "状态",
        meta: { label: "状态", width: "status" },
        enableSorting: false,
        cell: ({ row }) => (
          <BusinessStatusBadge
            context="list"
            label={row.original.statusLabel}
            tone={row.original.statusTone}
          />
        ),
      },
      {
        id: "revision",
        header: "版本",
        meta: { label: "版本", width: "status", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">v{row.original.revisionNo}</span>
        ),
      },
      {
        id: "sales",
        header: "销售单",
        meta: { label: "关联销售单", width: "status", numeric: true },
        cell: ({ row }) => (
          <span className="num text-sm">
            {row.original.salesOrderCount}
            {row.original.activeSalesOrderCount > 0 ? (
              <span className="text-muted-foreground">
                {" "}
                · 进行中 {row.original.activeSalesOrderCount}
              </span>
            ) : null}
          </span>
        ),
      },
      {
        id: "owner",
        accessorKey: "ownerLabel",
        header: "负责人",
        meta: { label: "负责人", width: "default" },
        cell: ({ row }) => (
          <span className="text-sm text-muted-foreground">
            {contractOwnerLabel(row.original.ownerLabel)}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
        enableSorting: false,
        cell: ({ row }) => {
          const canPrint = row.original.allowedActions.includes("PRINT")
          const printBlocker = row.original.actionBlockers.find(
            (b) => b.action === "PRINT"
          )
          return (
            <div
              className="flex justify-end gap-1"
              onClick={(event) => event.stopPropagation()}
              onKeyDown={(event) => event.stopPropagation()}
            >
              <Button
                type="button"
                variant="ghost"
                size="xs"
                onClick={() => setPreviewId(row.original.contractId)}
              >
                预览
              </Button>
              <Button
                type="button"
                variant="outline"
                size="xs"
                render={
                  <Link href={`/sales/contracts/${row.original.contractId}`} />
                }
              >
                打开
              </Button>
              <Button
                type="button"
                variant="outline"
                size="xs"
                disabled={!canPrint}
                title={
                  !canPrint
                    ? (printBlocker?.message ?? "当前不可打印")
                    : "纸质预览"
                }
                onClick={() => {
                  if (canPrint) setPaperId(row.original.contractId)
                }}
              >
                打印
              </Button>
            </div>
          )
        },
      },
    ],
    []
  )

  const handlePaginationChange = React.useCallback(
    (next: PaginationState) => {
      pushUrl({ page: next.pageIndex + 1, pageSize: next.pageSize })
    },
    [pushUrl]
  )

  const handleSearchCommit = React.useCallback(
    (value: string) => {
      setSearchDraft(value)
      pushUrl({ q: value.trim() || undefined, page: 1 })
    },
    [pushUrl]
  )

  const handleMetricChange = React.useCallback(
    (next: ContractMetricFilter) => {
      pushUrl({ metric: next, page: 1 })
    },
    [pushUrl]
  )

  const handleSortingChange = React.useCallback(
    (next: SortingState) => {
      const head = next[0]
      pushUrl({
        sort: head?.id,
        dir: head ? (head.desc ? "desc" : "asc") : undefined,
        page: 1,
      })
    },
    [pushUrl]
  )

  const handleClearCustomerLock = React.useCallback(() => {
    pushUrl({ customerId: undefined })
  }, [pushUrl])

  /** P4：清 q + 全部筛选参数（含 customerId 锁定）+ 分页回 1；保留排序。 */
  const clearAllFilters = React.useCallback(() => {
    setSearchDraft("")
    pushUrl({
      q: undefined,
      metric: "all",
      customerId: undefined,
      page: 1,
    })
  }, [pushUrl])

  const isFiltered =
    (q ?? "").trim() !== "" || metric !== "all" || Boolean(customerId)

  return (
    <PageScaffold density="compact">
      <PageHeader
        title="合同"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "contracts", label: "合同", current: true },
        ]}
        metadata={
          <DataFreshness
            updatedAt={
              contractsQuery.isError
                ? "查询失败"
                : contractsQuery.data
                  ? new Date(contractsQuery.dataUpdatedAt).toLocaleTimeString(
                      "zh-CN",
                      { hour: "2-digit", minute: "2-digit" }
                    )
                  : "正在查询"
            }
            dateTime={
              contractsQuery.data
                ? new Date(contractsQuery.dataUpdatedAt).toISOString()
                : undefined
            }
            state={
              contractsQuery.isError
                ? "failed"
                : contractsQuery.isFetching
                  ? "syncing"
                  : "fresh"
            }
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
                disabled: filtered.length === 0 || exportMutation.isPending,
                onClick: () => {
                  void handleExport()
                },
              },
              {
                actionKey: "upload",
                label: "上传合同 PDF",
                icon: FileUpIcon,
                onClick: () => setUploadOpen(true),
              },
            ]}
          />
        }
      />

      {actionResult ? (
        <FormalActionResult
          status={actionResult.status}
          title={actionResult.title}
          description={actionResult.description}
          facts={actionResult.facts}
          actions={
            actionResult.nextHref ? (
              <Button
                type="button"
                size="sm"
                render={<Link href={actionResult.nextHref} />}
              >
                查看详情
              </Button>
            ) : null
          }
        />
      ) : null}

      {exportJob ? (
        <FormalActionResult
          status="succeeded"
          title="合同导出完成"
          description={`共 ${exportJob.rowCount} 条，内容按当前筛选生成；下载时将重新校验权限。`}
          facts={[
            { label: "文件", value: exportJob.downloadLabel },
            { label: "行数", value: String(exportJob.rowCount) },
          ]}
        />
      ) : null}

      <MetricStrip columns={5} aria-label="合同快速筛选">
        <MetricFilterItem
          label="全部合同"
          value={metrics.all}
          detail="当前业务范围"
          active={metric === "all"}
          onClick={() => handleMetricChange("all")}
        />
        <MetricFilterItem
          label="有效"
          value={metrics.effective}
          detail="可关联建单"
          active={metric === "effective"}
          onClick={() => handleMetricChange("effective")}
        />
        <MetricFilterItem
          label="30 天内到期"
          value={metrics.expiring_30d}
          detail="将到期提醒"
          active={metric === "expiring_30d"}
          onClick={() => handleMetricChange("expiring_30d")}
        />
        <MetricFilterItem
          label="已到期"
          value={metrics.expired}
          detail="历史可追溯"
          active={metric === "expired"}
          onClick={() => handleMetricChange("expired")}
        />
        <MetricFilterItem
          label="已终止"
          value={metrics.terminated}
          detail="不再履行"
          active={metric === "terminated"}
          onClick={() => handleMetricChange("terminated")}
        />
      </MetricStrip>

      <BusinessTableFrame
        title="合同列表"
        description={
          metric === "all" && !(q ?? "").trim()
            ? "按将到期优先排序展示当前业务范围内的合同。"
            : `当前筛选：${contractMetricLabel(metric)}${
                (q ?? "").trim() ? ` · “${(q ?? "").trim()}”` : ""
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
                  data-slot="contracts-search"
                  value={searchDraft}
                  onChange={(event) => {
                    setSearchDraft(event.target.value)
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") {
                      handleSearchCommit(searchDraft)
                    }
                  }}
                  placeholder="合同号、客户、结算主体、负责人"
                  aria-label="搜索合同"
                />
              </InputGroup>
            }
            secondary={
              lockedCustomer ? (
                <FilterChip
                  label={`客户：${lockedCustomer.displayName}`}
                  onClear={handleClearCustomerLock}
                  clearLabel="清除客户锁定"
                />
              ) : undefined
            }
            actions={
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span aria-live="polite">
                  共 {sorted.length.toLocaleString("zh-CN")} 条
                </span>
                {isFiltered ? (
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    onClick={clearAllFilters}
                  >
                    清除筛选
                  </Button>
                ) : null}
              </div>
            }
          />
        }
        table={
          contractsQuery.isError ? (
            <BusinessFailureState
              kind="system"
              title="合同列表加载失败"
              description="暂时拿不到合同数据，请重试；失败不影响已保存的合同。"
              onRetry={() => {
                void contractsQuery.refetch()
              }}
            />
          ) : pageRows.length === 0 && !contractsQuery.isPending ? (
            <BusinessEmptyState
              kind={isFiltered ? "filter" : "no-data"}
              title={isFiltered ? undefined : "还没有合同"}
              description={
                isFiltered
                  ? "换一个关键词或清除筛选后再试。"
                  : "上传第一份合同 PDF，即可用于新建销售单。"
              }
              action={
                isFiltered ? (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={clearAllFilters}
                  >
                    清除筛选
                  </Button>
                ) : (
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => setUploadOpen(true)}
                  >
                    <FileUpIcon data-icon="inline-start" aria-hidden="true" />
                    上传合同 PDF
                  </Button>
                )
              }
            />
          ) : (
            <DataTable
              data={pageRows}
              columns={columns}
              getRowId={(row) => row.contractId}
              rowCount={sorted.length}
              sorting={sorting}
              onSortingChange={handleSortingChange}
              pagination={pagination}
              onPaginationChange={handlePaginationChange}
              loading={contractsQuery.isPending}
              layout="flush"
              density="compact"
              defaultColumnPinning={{
                left: ["contractNo"],
                right: ["actions"],
              }}
              onRowPreview={(row) => setPreviewId(row.contractId)}
              onRowOpen={(row) => setPreviewId(row.contractId)}
            />
          )
        }
      />

      <QuickPreviewSheet
        open={previewRow != null}
        onOpenChange={(open) => {
          if (!open) setPreviewId(null)
        }}
        size="detail"
        title={previewRow?.customer.displayName ?? "合同预览"}
        identity={
          previewRow ? (
            <span className="num">
              {previewRow.contractNo} · v{previewRow.revisionNo}
            </span>
          ) : null
        }
        summary={
          previewRow ? (
            <div className="flex flex-wrap items-center gap-2">
              <BusinessStatusBadge
                context="preview"
                label={previewRow.statusLabel}
                tone={previewRow.statusTone}
              />
              {previewRow.expiringWithin30Days ? (
                <Badge variant="warning">将到期</Badge>
              ) : null}
              <span className="text-xs text-muted-foreground">
                关联销售 {previewRow.salesOrderCount} 张
              </span>
            </div>
          ) : null
        }
        footer={
          previewRow ? (
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
                disabled={!previewRow.allowedActions.includes("PRINT")}
                title={
                  previewRow.actionBlockers.find((b) => b.action === "PRINT")
                    ?.message
                }
                onClick={() => setPaperId(previewRow.contractId)}
              >
                <PrinterIcon data-icon="inline-start" aria-hidden="true" />
                纸质预览
              </Button>
              <Button
                type="button"
                render={
                  <Link href={`/sales/contracts/${previewRow.contractId}`} />
                }
              >
                查看详情
              </Button>
            </>
          ) : null
        }
      >
        {previewRow ? (
          <ContractPreviewPanel
            row={previewRow}
            detail={previewDetailQuery.data}
            detailLoading={previewDetailQuery.isPending}
          />
        ) : null}
      </QuickPreviewSheet>

      <ContractPaperDialog
        contract={paperDetailQuery.data ?? null}
        open={paperId != null && paperDetailQuery.data != null}
        onOpenChange={(open) => {
          if (!open) setPaperId(null)
        }}
      />

      <ContractUploadDialog
        open={uploadOpen}
        onOpenChange={setUploadOpen}
        initialCustomerId={customerId ?? ""}
        onSuccess={handleUploadSuccess}
      />
    </PageScaffold>
  )
}
