"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
    QuickPreviewSheet,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceFilterField,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { FixedOptionRadioFilter } from "@/components/business/fixed-option-radio-filter"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge } from "@/components/ui/status-badge"
import { getErrorMessage } from "@/lib/api/errors"
import type { ExportJob, ExportJobStatus } from "@/features/export-tasks/api"
import {
    EXPORT_JOB_STATUS_LABELS,
    exportDomainLabel,
    exportProgressStatus,
    formatExportDateTime,
    isExportJobActive,
} from "@/features/export-tasks/labels"
import {
    useCancelExportJobMutation,
    useExportJobDetailQuery,
    useExportJobItemsQuery,
    useExportJobsQuery,
} from "@/features/export-tasks/queries"

const STATUS_FILTER_OPTIONS = [
    { value: "all", label: "全部" },
    { value: "active", label: "进行中" },
    { value: "pending", label: "等待执行" },
    { value: "running", label: "执行中" },
    { value: "partially_succeeded", label: "部分成功" },
    { value: "succeeded", label: "已完成" },
    { value: "failed", label: "执行失败" },
    { value: "cancelled", label: "已取消" },
] as const

type StatusFilter = (typeof STATUS_FILTER_OPTIONS)[number]["value"]

const DOMAIN_FILTER_OPTIONS = [
    { value: "", label: "全部类型" },
    { value: "SALES_ORDER_EXPORT", label: "销售单导出" },
    { value: "INVENTORY_LEDGER_EXPORT", label: "库存台账导出" },
    {
        value: "supplier_fulfillment_order_export",
        label: "供应商订单导出",
    },
    { value: "CONTRACT_EXPORT", label: "合同导出" },
] as const

export function ExportTasksWorkspace() {
    const [searchDraft, setSearchDraft] = React.useState("")
    const [appliedJobNo, setAppliedJobNo] = React.useState("")
    const [statusDraft, setStatusDraft] = React.useState<StatusFilter>("all")
    const [appliedStatus, setAppliedStatus] =
        React.useState<StatusFilter>("all")
    const [domainDraft, setDomainDraft] = React.useState<string>("")
    const [appliedDomain, setAppliedDomain] = React.useState<string>("")
    const [page, setPage] = React.useState(1)
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [cancelId, setCancelId] = React.useState<string | null>(null)
    const [cancelError, setCancelError] = React.useState<string | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    const searchInputRef = React.useRef<HTMLInputElement>(null)

    const listParams = React.useMemo(
        () => ({
            page,
            page_size: 20,
            job_no: appliedJobNo || undefined,
            status:
                appliedStatus === "all"
                    ? undefined
                    : (appliedStatus as ExportJobStatus | "active"),
            domain_job_type: appliedDomain || undefined,
        }),
        [page, appliedJobNo, appliedStatus, appliedDomain],
    )
    const jobsQuery = useExportJobsQuery(listParams)
    const detailQuery = useExportJobDetailQuery(previewId)
    const itemsQuery = useExportJobItemsQuery(previewId)
    const cancelMutation = useCancelExportJobMutation()

    const rows = React.useMemo(
        () => jobsQuery.data?.items ?? [],
        [jobsQuery.data?.items],
    )
    const previewJob = detailQuery.data ?? null

    const applyFilters = React.useCallback(() => {
        setAppliedJobNo(searchDraft.trim())
        setAppliedStatus(statusDraft)
        setAppliedDomain(domainDraft)
        setPage(1)
    }, [searchDraft, statusDraft, domainDraft])

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setAppliedJobNo("")
        setStatusDraft("all")
        setAppliedStatus("all")
        setDomainDraft("")
        setAppliedDomain("")
        setPage(1)
    }, [])

    const hasActiveFilters =
        appliedJobNo !== "" || appliedStatus !== "all" || appliedDomain !== ""

    const openPreview = React.useCallback((job: ExportJob) => {
        lastFocusedRowId.current = job.id
        setPreviewId(job.id)
    }, [])

    const closePreview = React.useCallback(() => {
        setPreviewId(null)
        if (lastFocusedRowId.current) {
            const el = document.querySelector(
                `[data-row-id="${CSS.escape(lastFocusedRowId.current)}"]`,
            )
            if (el instanceof HTMLElement) el.focus()
        }
    }, [])

    const confirmCancel = React.useCallback(async () => {
        if (!cancelId || !detailQuery.data) return
        setCancelError(null)
        try {
            await cancelMutation.mutateAsync({
                id: cancelId,
                version: detailQuery.data.version,
            })
            setCancelId(null)
        } catch (error) {
            setCancelError(getErrorMessage(error, "取消失败，请重试。"))
        }
    }, [cancelId, detailQuery.data, cancelMutation])

    const columns = React.useMemo<ColumnDef<ExportJob>[]>(
        () => [
            {
                id: "job",
                header: "导出任务",
                meta: { label: "导出任务" },
                cell: ({ row }) => (
                    <div className="min-w-0">
                        <p className="truncate font-medium">
                            {exportDomainLabel(row.original.domain_job_type)}
                        </p>
                        <p className="num mt-0.5 truncate text-xs text-muted-foreground">
                            {row.original.job_no}
                        </p>
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态" },
                cell: ({ row }) => (
                    <StatusBadge
                        tone={
                            row.original.status === "succeeded"
                                ? "success"
                                : row.original.status === "failed"
                                  ? "destructive"
                                  : row.original.status === "cancelled"
                                    ? "neutral"
                                    : "info"
                        }
                        label={EXPORT_JOB_STATUS_LABELS[row.original.status]}
                    />
                ),
            },
            {
                id: "progress",
                header: "进度",
                meta: { label: "进度", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {row.original.processed_count} /{" "}
                        {row.original.total_count}
                    </span>
                ),
            },
            {
                id: "created",
                header: "创建时间",
                meta: { label: "创建时间", numeric: true },
                cell: ({ row }) => (
                    <span className="num text-xs text-muted-foreground">
                        {formatExportDateTime(row.original.created_at)}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", role: "preview" },
                enableSorting: false,
                cell: ({ row }) => (
                    <Button
                        id={`governance-exports-row-${row.original.id}-preview`}
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => {
                            lastFocusedRowId.current = row.original.id
                            openPreview(row.original)
                        }}
                    >
                        查看
                    </Button>
                ),
            },
        ],
        [openPreview],
    )

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="治理"
                title="导出任务"
                description="集中查看各业务的导出进度、结果与有效期，取消尚未完成的任务。"
            />
            <ListWorkSurface
                ariaLabel="导出任务"
                toolbar={
                    <ListWorkspaceFilterBar
                        idPrefix="governance-exports-toolbar"
                        formAriaLabel="导出任务查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id="governance-exports-toolbar-search-input"
                                searchInputRef={searchInputRef}
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="按任务号搜索"
                                aria-label="按任务号搜索导出任务"
                            />
                        }
                        commonFilters={
                            <>
                                <FixedOptionRadioFilter
                                    idPrefix="governance-exports-toolbar-status"
                                    label="状态"
                                    variant="quiet"
                                    value={statusDraft}
                                    onValueChange={setStatusDraft}
                                    options={STATUS_FILTER_OPTIONS}
                                />
                                <ListWorkspaceFilterField
                                    htmlFor="governance-exports-toolbar-domain"
                                    label="任务类型"
                                >
                                    <select
                                        id="governance-exports-toolbar-domain"
                                        className="h-9 w-full rounded-md border bg-background px-2 text-[13px] sm:w-52"
                                        value={domainDraft}
                                        onChange={(event) =>
                                            setDomainDraft(event.target.value)
                                        }
                                        aria-label="任务类型"
                                    >
                                        {DOMAIN_FILTER_OPTIONS.map((option) => (
                                            <option
                                                key={option.value || "all"}
                                                value={option.value}
                                            >
                                                {option.label}
                                            </option>
                                        ))}
                                    </select>
                                </ListWorkspaceFilterField>
                            </>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: jobsQuery.isFetching,
                            failed: jobsQuery.isError,
                            resultCount: jobsQuery.data?.total,
                            noun: "个任务",
                        })}
                        chips={[
                            ...(appliedJobNo
                                ? [
                                      {
                                          key: "job_no",
                                          label: `任务号：${appliedJobNo}`,
                                      },
                                  ]
                                : []),
                            ...(appliedStatus !== "all"
                                ? [
                                      {
                                          key: "status",
                                          label: `状态：${
                                              STATUS_FILTER_OPTIONS.find(
                                                  (option) =>
                                                      option.value ===
                                                      appliedStatus,
                                              )?.label ?? appliedStatus
                                          }`,
                                      },
                                  ]
                                : []),
                            ...(appliedDomain
                                ? [
                                      {
                                          key: "domain",
                                          label: `类型：${exportDomainLabel(appliedDomain)}`,
                                      },
                                  ]
                                : []),
                        ]}
                        onClearChip={(key) => {
                            if (key === "job_no") {
                                setSearchDraft("")
                                setAppliedJobNo("")
                            }
                            if (key === "status") {
                                setStatusDraft("all")
                                setAppliedStatus("all")
                            }
                            if (key === "domain") {
                                setDomainDraft("")
                                setAppliedDomain("")
                            }
                            setPage(1)
                        }}
                        onClearAll={clearFilters}
                    />
                }
                table={
                    <DataTable
                        id="governance-exports-table"
                        data={rows}
                        columns={columns}
                        getRowId={(row) => row.id}
                        rowCount={jobsQuery.data?.total ?? 0}
                        pagination={{
                            pageIndex: page - 1,
                            pageSize: 20,
                        }}
                        onPaginationChange={(next) => {
                            setPage(next.pageIndex + 1)
                        }}
                        loading={jobsQuery.isFetching}
                        layout="flush"
                        onRowPreview={openPreview}
                        highlightedRowId={previewId ?? undefined}
                        errorState={
                            jobsQuery.isError ? (
                                <BusinessFailureState
                                    error={jobsQuery.error}
                                    action={
                                        <Button
                                            id="governance-exports-retry"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            onClick={() =>
                                                void jobsQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            ) : undefined
                        }
                        emptyState={
                            !jobsQuery.isError && rows.length === 0 ? (
                                <BusinessEmptyState
                                    kind={
                                        hasActiveFilters ? "filter" : "no-data"
                                    }
                                    className={listWorkspaceEmptyStateClassName}
                                    title={
                                        hasActiveFilters
                                            ? "当前筛选无结果"
                                            : "还没有导出任务"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有任务符合当前筛选条件，可清除筛选后重试。"
                                            : "在销售单、库存台账等页面发起导出后，进度会集中显示在这里。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                id="governance-exports-empty-clear-filters"
                                                type="button"
                                                variant="secondary"
                                                size="sm"
                                                className="rounded-lg shadow-none"
                                                onClick={clearFilters}
                                            >
                                                清除筛选
                                            </Button>
                                        ) : undefined
                                    }
                                />
                            ) : undefined
                        }
                    />
                }
            />
            <QuickPreviewSheet
                idPrefix="governance-exports-preview-sheet"
                open={previewJob != null}
                onOpenChange={(open) => {
                    if (!open) closePreview()
                }}
                size="preview"
                contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px]"
                title={
                    previewJob
                        ? exportDomainLabel(previewJob.domain_job_type)
                        : "导出任务"
                }
                description={
                    previewJob ? `任务号 ${previewJob.job_no}` : undefined
                }
                identity={
                    previewJob ? (
                        <span className="num">任务号：{previewJob.job_no}</span>
                    ) : null
                }
                summary={
                    previewJob ? (
                        <div className="flex flex-wrap items-center gap-2">
                            <StatusBadge
                                tone={
                                    previewJob.status === "succeeded"
                                        ? "success"
                                        : previewJob.status === "failed"
                                          ? "destructive"
                                          : previewJob.status === "cancelled"
                                            ? "neutral"
                                            : "info"
                                }
                                label={
                                    EXPORT_JOB_STATUS_LABELS[previewJob.status]
                                }
                            />
                            <span className="text-xs text-muted-foreground">
                                {exportDomainLabel(previewJob.domain_job_type)}
                            </span>
                        </div>
                    ) : null
                }
                footer={
                    previewJob ? (
                        <>
                            <Button
                                id="governance-exports-preview-close"
                                type="button"
                                variant="outline"
                                onClick={closePreview}
                            >
                                关闭
                            </Button>
                            {previewJob &&
                            isExportJobActive(previewJob.status) ? (
                                <Button
                                    id="governance-exports-preview-cancel"
                                    type="button"
                                    variant="destructive"
                                    disabled={cancelMutation.isPending}
                                    onClick={() => setCancelId(previewJob.id)}
                                >
                                    {cancelMutation.isPending
                                        ? "取消中…"
                                        : "取消任务"}
                                </Button>
                            ) : null}
                        </>
                    ) : null
                }
            >
                {previewJob ? (
                    <div className="space-y-6 text-sm">
                        <BackgroundJobProgress
                            mode="partialAllowed"
                            status={exportProgressStatus(previewJob.status)}
                            total={previewJob.total_count}
                            completed={previewJob.processed_count}
                            succeeded={previewJob.success_count}
                            skipped={previewJob.skipped_count}
                            failed={previewJob.failed_count}
                            label={exportDomainLabel(
                                previewJob.domain_job_type,
                            )}
                            description={
                                previewJob.error_summary
                                    ? previewJob.error_summary
                                    : "导出按选择快照执行，已完成的部分不受未完成部分影响。"
                            }
                        />
                        <section className="space-y-3">
                            <h3 className="font-medium">任务资料</h3>
                            <DescriptionList columns="two">
                                <DescriptionItem>
                                    <DescriptionTerm>发起人</DescriptionTerm>
                                    <DescriptionDetails>
                                        {previewJob.requested_by || "—"}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>创建时间</DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatExportDateTime(
                                            previewJob.created_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>开始时间</DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatExportDateTime(
                                            previewJob.started_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>结束时间</DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatExportDateTime(
                                            previewJob.finished_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>
                                        结果有效期至
                                    </DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatExportDateTime(
                                            previewJob.result_expires_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>目标总数</DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {previewJob.total_count}
                                    </DescriptionDetails>
                                </DescriptionItem>
                            </DescriptionList>
                        </section>
                        <section className="space-y-3">
                            <h3 className="font-medium">逐项结果</h3>
                            {itemsQuery.isPending ? (
                                <p className="text-muted-foreground">
                                    正在加载逐项结果…
                                </p>
                            ) : (itemsQuery.data?.items.length ?? 0) === 0 ? (
                                <p className="text-muted-foreground">
                                    暂无逐项结果。
                                </p>
                            ) : (
                                <ul className="space-y-2">
                                    {itemsQuery.data?.items.map((item) => (
                                        <li
                                            key={item.id}
                                            className="rounded-lg border p-3"
                                        >
                                            <div className="flex items-center justify-between gap-2">
                                                <span className="num text-xs text-muted-foreground">
                                                    第 {item.item_no} 项
                                                </span>
                                                <span className="text-xs">
                                                    {item.status ?? "待执行"}
                                                </span>
                                            </div>
                                            <p className="mt-1 text-[13px]">
                                                {item.result_summary ||
                                                    item.object_type ||
                                                    "排队执行中"}
                                            </p>
                                        </li>
                                    ))}
                                </ul>
                            )}
                        </section>
                    </div>
                ) : null}
            </QuickPreviewSheet>
            <AlertDialog
                open={cancelId != null}
                onOpenChange={(open) => {
                    if (!open) {
                        setCancelId(null)
                        setCancelError(null)
                    }
                }}
            >
                <AlertDialogContent size="sm">
                    <AlertDialogHeader>
                        <AlertDialogTitle>取消导出任务</AlertDialogTitle>
                        <AlertDialogDescription>
                            取消后尚未开始的部分不再执行，已经完成的部分不受影响。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    {cancelError ? (
                        <p className="text-sm text-destructive" role="alert">
                            {cancelError}
                        </p>
                    ) : null}
                    <AlertDialogFooter>
                        <AlertDialogCancel id="governance-exports-cancel-back">
                            返回
                        </AlertDialogCancel>
                        <AlertDialogAction
                            id="governance-exports-cancel-confirm"
                            disabled={cancelMutation.isPending}
                            onClick={() => {
                                void confirmCancel()
                            }}
                        >
                            {cancelMutation.isPending ? "取消中…" : "确认取消"}
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </PageScaffold>
    )
}
