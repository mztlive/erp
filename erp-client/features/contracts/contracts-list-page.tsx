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
import { z } from "zod"

import {
  BackgroundJobProgress,
  BusinessStatusBadge,
  BusinessTableFrame,
  CustomerCombobox,
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
  SettlementPartyCombobox,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import {
  PAYMENT_TERM_OPTIONS,
  SETTLEMENT_PARTY_OPTIONS,
  paymentTermLabel,
} from "@/lib/business-options"
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
import { ContractPaperDialog } from "@/features/contracts/contract-paper-dialog"
import { ContractPreviewPanel } from "@/features/contracts/contract-preview-panel"
import {
  computeContractMetrics,
  contractMetricLabel,
  filterContracts,
  type ContractMetricFilter,
  type ContractStatusFilter,
} from "@/features/contracts/filter-contracts"
import { contractPdfError } from "@/features/contracts/pdf"
import {
  useContractCenterQuery,
  useContractsQuery,
  useCreateContractExportJobMutation,
  useUploadContractPdfMutation,
} from "@/features/contracts/queries"
import type {
  ContractExportJob,
  ContractListRow,
} from "@/features/contracts/types"
import {
  useCustomerCenterQuery,
  useCustomerDirectoryQuery,
} from "@/features/customers/queries"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"

const uploadSchema = z
  .object({
    pdfFile: z.custom<File | null>(),
    contractNo: z.string().trim().min(1, "请填写合同编号"),
    customerId: z.string().trim().min(1, "请选择客户"),
    customerName: z.string().trim().min(2, "请选择客户"),
    settlementPartyId: z.string().trim().min(1, "请选择结算主体"),
    settlementPartyName: z.string().trim().min(2, "请选择结算主体"),
    paymentTerms: z.string().trim().min(1, "请选择付款条件"),
    signedAt: z.string().min(1, "请填写签订日期"),
    validFrom: z.string().min(1, "请填写有效期起"),
    validTo: z.string().min(1, "请填写有效期止"),
  })
  .superRefine((value, context) => {
    const fileError = contractPdfError(value.pdfFile)
    if (fileError) {
      context.addIssue({ code: "custom", path: ["pdfFile"], message: fileError })
    }
    if (value.validFrom && value.validTo && value.validTo < value.validFrom) {
      context.addIssue({
        code: "custom",
        path: ["validTo"],
        message: "有效期止不能早于有效期起",
      })
    }
  })

export function ContractsListPage({
  initialSearch = "",
  initialCustomerId = "",
}: {
  initialSearch?: string
  initialCustomerId?: string
}) {
  const contractsQuery = useContractsQuery()
  const customerQuery = useCustomerCenterQuery(initialCustomerId)
  const customerDirectoryQuery = useCustomerDirectoryQuery({
    scope: "team",
    status: "active",
  })
  const allRows = React.useMemo(
    () => contractsQuery.data ?? [],
    [contractsQuery.data]
  )
  const seededCustomerRef = React.useRef(false)

  const customerComboboxItems = React.useMemo(
    () =>
      (customerDirectoryQuery.data?.items ?? []).map((c) => ({
        id: c.id,
        customerNo: c.customerNo,
        legalName: c.legalName,
        shortName: c.shortName,
        statusLabel: c.statusLabel.label,
        statusTone: c.statusLabel.tone,
        ownerName: c.ownerName,
      })),
    [customerDirectoryQuery.data?.items]
  )

  const settlementPartyItems = React.useMemo(() => {
    const fromRows = allRows.map((r) => ({
      partyId: r.settlementParty.partyId,
      displayName: r.settlementParty.displayName,
      statusLabel: "可选" as const,
      statusTone: "neutral" as const,
    }))
    const byId = new Map(
      [...SETTLEMENT_PARTY_OPTIONS, ...fromRows].map((p) => [p.partyId, p])
    )
    return [...byId.values()]
  }, [allRows])

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

  const uploadMutation = useUploadContractPdfMutation()
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

  const uploadForm = useAppForm({
    defaultValues: {
      pdfFile: null as File | null,
      contractNo: "",
      customerId: initialCustomerId,
      customerName: "",
      settlementPartyId: "",
      settlementPartyName: "",
      paymentTerms: "CONTRACT",
      signedAt: "2026-08-02",
      validFrom: "2026-08-02",
      validTo: "2027-08-01",
    },
    validators: { onChange: uploadSchema },
    onSubmit: async ({ value }) => {
      if (!value.pdfFile) return
      const result = await uploadMutation.mutateAsync({
        pdfFile: value.pdfFile,
        contractNo: value.contractNo.trim(),
        customerId:
          value.customerId.trim() || initialCustomerId || undefined,
        customerName: value.customerName.trim(),
        settlementPartyName: value.settlementPartyName.trim(),
        paymentTerms:
          paymentTermLabel(value.paymentTerms) || value.paymentTerms.trim(),
        signedAt: value.signedAt,
        validFrom: value.validFrom,
        validTo: value.validTo,
        idempotencyKey: `upload-${Date.now().toString(36)}`,
      })
      setUploadOpen(false)
      uploadForm.reset()
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
  })

  React.useEffect(() => {
    const customer = customerQuery.data
    if (!customer || seededCustomerRef.current) return
    seededCustomerRef.current = true
    uploadForm.setFieldValue("customerId", customer.customerId)
    uploadForm.setFieldValue(
      "customerName",
      customer.currentRevision.legalName
    )
  }, [customerQuery.data, uploadForm])

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

      {uploadMutation.isError ? (
        <FormalActionResult
          status="blocked"
          title="合同 PDF 未归档"
          description={
            uploadMutation.error instanceof Error &&
            uploadMutation.error.message === "CONTRACT_NO_EXISTS"
              ? "该合同编号已存在，请打开已有合同；如为新签署版本，应归档到原合同。"
              : uploadMutation.error instanceof Error &&
                  uploadMutation.error.message === "CONTRACT_VALIDITY_INVALID"
                ? "有效期止不能早于有效期起。"
                : uploadMutation.error instanceof Error
                  ? uploadMutation.error.message
                  : "上传失败，请使用原任务号重试。"
          }
          reference="CONTRACT-PDF-NOT-COMMITTED"
        />
      ) : null}

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

      <Dialog open={uploadOpen} onOpenChange={setUploadOpen}>
        <DialogContent className="flex max-h-[calc(100dvh-2rem)] flex-col gap-0 overflow-hidden p-0">
          <DialogHeader className="px-6 pt-6">
            <DialogTitle>上传合同 PDF</DialogTitle>
            <DialogDescription>
              系统不新建或编辑合同正文；上传已签署电子档并补充检索信息后，形成可引用的合同版本。
            </DialogDescription>
          </DialogHeader>
          <form
            className="flex min-h-0 flex-1 flex-col"
            onSubmit={(event) => {
              event.preventDefault()
              void uploadForm.handleSubmit()
            }}
          >
            <div className="grid min-h-0 flex-1 gap-3 overflow-y-auto px-6 py-4">
              <uploadForm.AppField
                name="pdfFile"
                children={(field) => <field.PdfUploadField label="合同电子档" />}
              />
              <uploadForm.AppField
                name="contractNo"
                children={(field) => <field.TextField label="合同编号" />}
              />
              <uploadForm.AppField
                name="customerId"
                children={(field) => {
                  const isInvalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                  const errors = toFieldErrors(field.state.meta.errors)
                  return (
                    <Field data-invalid={isInvalid || undefined}>
                      <FieldLabel htmlFor="upload-customerId">客户</FieldLabel>
                      <CustomerCombobox
                        value={field.state.value || undefined}
                        onValueChange={(id) => {
                          const next = id ?? ""
                          field.handleChange(next)
                          const customer = customerComboboxItems.find(
                            (c) => c.id === next
                          )
                          uploadForm.setFieldValue(
                            "customerName",
                            customer?.legalName ?? ""
                          )
                        }}
                        customers={customerComboboxItems}
                        loading={customerDirectoryQuery.isPending}
                        placeholder="搜索客户编号或名称"
                      />
                      {isInvalid ? <FieldError errors={errors} /> : null}
                    </Field>
                  )
                }}
              />
              <uploadForm.AppField
                name="settlementPartyId"
                children={(field) => {
                  const isInvalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                  const errors = toFieldErrors(field.state.meta.errors)
                  return (
                    <Field data-invalid={isInvalid || undefined}>
                      <FieldLabel htmlFor="upload-settlementPartyId">
                        结算主体
                      </FieldLabel>
                      <SettlementPartyCombobox
                        value={field.state.value || undefined}
                        onValueChange={(id) => {
                          const next = id ?? ""
                          field.handleChange(next)
                          const party = settlementPartyItems.find(
                            (p) => p.partyId === next
                          )
                          uploadForm.setFieldValue(
                            "settlementPartyName",
                            party?.displayName ?? ""
                          )
                        }}
                        parties={settlementPartyItems}
                        placeholder="搜索结算主体"
                      />
                      {isInvalid ? <FieldError errors={errors} /> : null}
                    </Field>
                  )
                }}
              />
              <uploadForm.AppField
                name="paymentTerms"
                children={(field) => (
                  <field.SelectField
                    label="付款条件"
                    options={PAYMENT_TERM_OPTIONS}
                    description="用于销售单快速带出；完整条款以 PDF 为准。"
                  />
                )}
              />
              <div className="grid gap-3 sm:grid-cols-2">
                <uploadForm.AppField
                  name="signedAt"
                  children={(field) => <field.DateField label="签订日期" />}
                />
                <uploadForm.AppField
                  name="validFrom"
                  children={(field) => <field.DateField label="有效期起" />}
                />
                <uploadForm.AppField
                  name="validTo"
                  children={(field) => <field.DateField label="有效期止" />}
                />
              </div>
            </div>
            <DialogFooter className="shrink-0 border-t px-6 py-4">
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <uploadForm.AppForm>
                <uploadForm.SubmitButton
                  label={uploadMutation.isPending ? "上传中…" : "上传并归档"}
                />
              </uploadForm.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
