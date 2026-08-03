"use client"

import * as React from "react"
import Link from "next/link"
import {
  DownloadIcon,
  FileUpIcon,
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
  OptionCombobox,
  PageActions,
  PageHeader,
  QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import { ContractPreviewPanel } from "@/features/contracts/contract-preview-panel"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import {
  computeContractMetrics,
  contractMetricLabel,
  filterContracts,
  type ContractMetricFilter,
  type ContractStatusFilter,
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

export function ContractsListPage({
  initialSearch = "",
  initialCustomerId = "",
}: {
  initialSearch?: string
  initialCustomerId?: string
}) {
  const contractsQuery = useContractsQuery()
  const allRows = React.useMemo(
    () => contractsQuery.data ?? [],
    [contractsQuery.data]
  )

  const [search, setSearch] = React.useState(initialSearch)
  const [metricKey, setMetricKey] =
    React.useState<ContractMetricFilter>("all")
  const [statusFilter, setStatusFilter] =
    React.useState<ContractStatusFilter>("all")
  const [pagination, setPagination] = React.useState<PaginationState>({
    pageIndex: 0,
    pageSize: 20,
  })
  const [previewId, setPreviewId] = React.useState<string | null>(null)
  const [paperId, setPaperId] = React.useState<string | null>(null)
  const [uploadOpen, setUploadOpen] = React.useState(Boolean(initialCustomerId))
  const [exportJob, setExportJob] = React.useState<ContractExportJob | null>(
    null
  )
  const [actionResult, setActionResult] = React.useState<{
    status: "succeeded" | "blocked"
    title: string
    description: string
    reference: string
    facts?: Array<{ label: string; value: string }>
    nextHref?: string
  } | null>(null)

  const exportMutation = useCreateContractExportJobMutation()

  const resetPagination = React.useCallback(() => {
    setPagination((previous) =>
      previous.pageIndex === 0 ? previous : { ...previous, pageIndex: 0 }
    )
  }, [])

  const filtered = React.useMemo(() => {
    let rows = filterContracts(allRows, {
      search,
      metricKey,
      statusFilter,
    })
    if (initialCustomerId) {
      rows = rows.filter((r) => r.customer.customerId === initialCustomerId)
    }
    return rows
  }, [allRows, initialCustomerId, metricKey, search, statusFilter])

  const pageRows = React.useMemo(() => {
    const start = pagination.pageIndex * pagination.pageSize
    return filtered.slice(start, start + pagination.pageSize)
  }, [filtered, pagination.pageIndex, pagination.pageSize])

  const metrics = React.useMemo(
    () => computeContractMetrics(allRows),
    [allRows]
  )

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
        reference: result.reference,
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
      `指标=${contractMetricLabel(metricKey)}`,
      statusFilter === "all" ? "状态=全部" : `状态=${statusFilter}`,
      search.trim() ? `搜索=${search.trim()}` : "搜索=空",
      initialCustomerId ? `客户=${initialCustomerId}` : null,
    ].filter(Boolean)
    return parts.join(" · ")
  }, [initialCustomerId, metricKey, search, statusFilter])

  const handleExport = React.useCallback(async () => {
    if (filtered.length === 0) return
    const job = await exportMutation.mutateAsync({
      rowCount: filtered.length,
      filterSnapshotLabel,
    })
    setExportJob(job)
    setActionResult({
      status: "succeeded",
      title: "导出任务已创建",
      description:
        "导出内容按当前筛选生成；下载时将重新校验权限。",
      reference: job.jobId,
      facts: [
        { label: "筛选结果", value: job.filterSnapshotLabel },
        { label: "行数", value: String(job.rowCount) },
        { label: "权限版本", value: job.permissionVersion },
        { label: "下载标签", value: job.downloadLabel },
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
              aria-label={`预览 ${row.original.contractNo}`}
              onClick={(event) => {
                event.stopPropagation()
                setPreviewId(row.original.contractId)
              }}
            >
              {row.original.contractNo}
            </Button>
            <div className="truncate text-xs text-muted-foreground">
              {row.original.customer.displayName}
            </div>
          </div>
        ),
      },
      {
        id: "customer",
        accessorFn: (row) => row.customer.displayName,
        header: "客户",
        meta: { label: "客户", width: "default" },
        cell: ({ row }) => (
          <div className="min-w-0">
            <div className="truncate text-sm">
              {row.original.customer.displayName}
            </div>
            <div className="num text-xs text-muted-foreground">
              {row.original.customer.customerNo}
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
            {row.original.ownerLabel}
          </span>
        ),
      },
      {
        id: "actions",
        header: "操作",
        meta: { label: "操作", width: "default", align: "end" },
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

  if (contractsQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
        <PageHeader title="合同" description="正在加载列表数据…" />
      </div>
    )
  }

  if (contractsQuery.isError) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:p-4">
        <PageHeader title="合同" description="列表数据加载失败。" />
        <Button type="button" onClick={() => void contractsQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:gap-3.5 md:p-4">
      <PageHeader
        title="合同"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "contracts", label: "合同", current: true },
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
                mobileVisibility: "hide",
                disabled: filtered.length === 0 || exportMutation.isPending,
                onClick: () => {
                  void handleExport()
                },
              },
              {
                actionKey: "upload",
                label: "上传合同 PDF",
                icon: FileUpIcon,
                mobileVisibility: "hide",
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
          reference={actionResult.reference}
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
        <BackgroundJobProgress
          mode="all-or-nothing"
          status={exportJob.status}
          total={exportJob.rowCount}
          completed={exportJob.rowCount}
          succeeded={exportJob.rowCount}
          label="合同导出任务"
          description={
            <>
              筛选结果：{exportJob.filterSnapshotLabel}
              。任务号 <span className="num">{exportJob.jobId}</span>
              ，结果保留 7 天；下载时将重新校验权限。
            </>
          }
          action={
            <Button type="button" size="sm" variant="outline" disabled>
              下载（演示·已就绪）
            </Button>
          }
        />
      ) : null}

      <MetricStrip columns={4} aria-label="合同快速筛选">
        <MetricFilterItem
          label="全部合同"
          value={metrics.all}
          detail="当前业务范围"
          active={metricKey === "all"}
          onClick={() => {
            setMetricKey("all")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="有效"
          value={metrics.effective}
          detail="可关联建单"
          active={metricKey === "effective"}
          onClick={() => {
            setMetricKey("effective")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="30 天内到期"
          value={metrics.expiring_30d}
          detail="将到期提醒"
          active={metricKey === "expiring_30d"}
          onClick={() => {
            setMetricKey("expiring_30d")
            resetPagination()
          }}
        />
        <MetricFilterItem
          label="已到期"
          value={metrics.expired}
          detail="历史可追溯"
          active={metricKey === "expired"}
          onClick={() => {
            setMetricKey("expired")
            resetPagination()
          }}
        />
      </MetricStrip>

      <BusinessTableFrame
        title="合同列表"
        description={
          metricKey === "all" && statusFilter === "all" && !search
            ? "按将到期优先排序展示当前业务范围内的合同。"
            : `当前筛选：${contractMetricLabel(metricKey)}${
                statusFilter !== "all" ? ` · ${statusFilter}` : ""
              }${search ? ` · “${search}”` : ""}`
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
                  placeholder="合同号、客户、结算主体"
                  aria-label="搜索合同"
                />
              </InputGroup>
            }
            filters={
              <>
                <ToggleGroup
                  value={[statusFilter]}
                  onValueChange={(values) => {
                    const next = (values[0] as ContractStatusFilter | undefined) ?? "all"
                    setStatusFilter(next)
                    resetPagination()
                  }}
                  variant="outline"
                  size="sm"
                  spacing={0}
                >
                  <ToggleGroupItem value="all">全部状态</ToggleGroupItem>
                  <ToggleGroupItem value="EFFECTIVE">生效</ToggleGroupItem>
                  <ToggleGroupItem value="EXPIRED">到期</ToggleGroupItem>
                  <ToggleGroupItem value="TERMINATED">终止</ToggleGroupItem>
                </ToggleGroup>
                <OptionCombobox
                  className="w-[9.5rem]"
                  value={metricKey}
                  aria-label="有效期视图"
                  onValueChange={(v) => {
                    setMetricKey((v ?? "all") as ContractMetricFilter)
                    resetPagination()
                  }}
                  options={[
                    { value: "all", label: "有效期：全部" },
                    { value: "expiring_30d", label: "30 天内到期" },
                    { value: "expired", label: "已到期" },
                    { value: "terminated", label: "已终止" },
                  ]}
                  size="sm"
                  allowClear={false}
                  placeholder="有效期视图"
                />
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
            getRowId={(row) => row.contractId}
            rowCount={filtered.length}
            pagination={pagination}
            onPaginationChange={setPagination}
            layout="flush"
            density="compact"
            defaultColumnPinning={{
              left: ["contractNo"],
              right: ["actions"],
            }}
            onRowPreview={(row) => setPreviewId(row.contractId)}
            onRowOpen={(row) => setPreviewId(row.contractId)}
          />
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
        initialCustomerId={initialCustomerId}
        onSuccess={handleUploadSuccess}
      />
    </div>
  )
}
