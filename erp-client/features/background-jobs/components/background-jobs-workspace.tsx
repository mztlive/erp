"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    OptionCombobox,
    PageScaffold,
    QuickPreviewSheet,
    TableRowActions,
} from "@/components/business"
import {
    ListSearchField,
    ListWorkSurface,
    ListWorkspaceFilterBar,
    ListWorkspaceInlineFilter,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceFilterStatusText,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
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
import { useAccountProfileQuery } from "@/features/auth/queries"
import type {
    BackgroundJobView,
    CancelAllBackgroundJobsResult,
    JobStatus,
} from "@/features/background-jobs/api"
import {
    backgroundJobDomainLabel,
    formatJobDateTime,
    isBackgroundJobAdmin,
    isJobActive,
    JOB_DOMAIN_FILTER_OPTIONS,
    JOB_STATUS_LABELS,
    JOB_TYPE_FILTER_OPTIONS,
    JOB_TYPE_LABELS,
    jobProgressStatus,
} from "@/features/background-jobs/labels"
import {
    useBackgroundJobDetailQuery,
    useBackgroundJobItemsQuery,
    useBackgroundJobsQuery,
    useCancelAllBackgroundJobsMutation,
    useCancelBackgroundJobMutation,
} from "@/features/background-jobs/queries"

import { SupplierImportFailures } from "./supplier-import-failures"

const ID_PREFIX = "governance-background-jobs"

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

const SCOPE_FILTER_OPTIONS = [
    { value: "all", label: "全部任务" },
    { value: "mine", label: "只看我的" },
] as const

type ScopeFilter = (typeof SCOPE_FILTER_OPTIONS)[number]["value"]

/** 后台任务页：集中查看导入、导出等任务的进度与结果。 */
export function BackgroundJobsWorkspace() {
    const profileQuery = useAccountProfileQuery()
    const isAdmin = isBackgroundJobAdmin(profileQuery.data?.role_ids)
    const currentUserId = profileQuery.data?.userid
    const [searchDraft, setSearchDraft] = React.useState("")
    const [appliedJobNo, setAppliedJobNo] = React.useState("")
    const [statusDraft, setStatusDraft] = React.useState<StatusFilter>("all")
    const [appliedStatus, setAppliedStatus] =
        React.useState<StatusFilter>("all")
    const [jobTypeDraft, setJobTypeDraft] = React.useState<string>("")
    const [appliedJobType, setAppliedJobType] = React.useState<string>("")
    const [domainDraft, setDomainDraft] = React.useState<string>("")
    const [appliedDomain, setAppliedDomain] = React.useState<string>("")
    const [scopeDraft, setScopeDraft] = React.useState<ScopeFilter>("all")
    const [appliedScope, setAppliedScope] = React.useState<ScopeFilter>("all")
    const [page, setPage] = React.useState(1)
    const [itemsPage, setItemsPage] = React.useState(1)
    const [previewId, setPreviewId] = React.useState<string | null>(null)
    const [cancelId, setCancelId] = React.useState<string | null>(null)
    const [cancelError, setCancelError] = React.useState<string | null>(null)
    const [stopAllOpen, setStopAllOpen] = React.useState(false)
    const [stopAllError, setStopAllError] = React.useState<string | null>(null)
    const [stopAllResult, setStopAllResult] =
        React.useState<CancelAllBackgroundJobsResult | null>(null)
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
                    : (appliedStatus as JobStatus | "active"),
            job_type: appliedJobType || undefined,
            domain_job_type: appliedDomain || undefined,
            requested_by:
                isAdmin && appliedScope === "mine" ? currentUserId : undefined,
        }),
        [
            page,
            appliedJobNo,
            appliedStatus,
            appliedJobType,
            appliedDomain,
            isAdmin,
            appliedScope,
            currentUserId,
        ],
    )
    const jobsQuery = useBackgroundJobsQuery(listParams)
    const detailQuery = useBackgroundJobDetailQuery(previewId)
    const itemsQuery = useBackgroundJobItemsQuery(
        previewId,
        itemsPage,
        Boolean(
            detailQuery.data &&
            detailQuery.data.finished_at === null &&
            isJobActive(detailQuery.data.status),
        ),
    )
    const cancelMutation = useCancelBackgroundJobMutation()
    const cancelAllMutation = useCancelAllBackgroundJobsMutation()

    const rows = React.useMemo(
        () => jobsQuery.data?.items ?? [],
        [jobsQuery.data?.items],
    )
    const previewJob = detailQuery.data ?? null

    const applyFilters = React.useCallback(() => {
        setAppliedJobNo(searchDraft.trim())
        setAppliedStatus(statusDraft)
        setAppliedJobType(jobTypeDraft)
        setAppliedDomain(domainDraft)
        setAppliedScope(scopeDraft)
        setPage(1)
    }, [searchDraft, statusDraft, jobTypeDraft, domainDraft, scopeDraft])

    const clearFilters = React.useCallback(() => {
        setSearchDraft("")
        setAppliedJobNo("")
        setStatusDraft("all")
        setAppliedStatus("all")
        setJobTypeDraft("")
        setAppliedJobType("")
        setDomainDraft("")
        setAppliedDomain("")
        setScopeDraft("all")
        setAppliedScope("all")
        setPage(1)
    }, [])

    const hasActiveFilters =
        appliedJobNo !== "" ||
        appliedStatus !== "all" ||
        appliedJobType !== "" ||
        appliedDomain !== "" ||
        (isAdmin && appliedScope !== "all")

    const hasPendingChanges =
        searchDraft.trim() !== appliedJobNo ||
        statusDraft !== appliedStatus ||
        jobTypeDraft !== appliedJobType ||
        domainDraft !== appliedDomain ||
        (isAdmin && scopeDraft !== appliedScope)

    const jobLabel = React.useCallback(
        (job: BackgroundJobView) =>
            backgroundJobDomainLabel(job.domain_job_type, job.job_type),
        [],
    )

    const openPreview = React.useCallback((job: BackgroundJobView) => {
        setItemsPage(1)
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

    const openStopAll = React.useCallback(() => {
        setStopAllError(null)
        setStopAllResult(null)
        setStopAllOpen(true)
    }, [])

    const confirmStopAll = React.useCallback(async () => {
        setStopAllError(null)
        try {
            const result = await cancelAllMutation.mutateAsync(undefined)
            setStopAllResult(result)
        } catch (error) {
            setStopAllError(
                getErrorMessage(error, "停止全部任务失败，请重试。"),
            )
        }
    }, [cancelAllMutation])

    const columns = React.useMemo<ColumnDef<BackgroundJobView>[]>(
        () => [
            {
                id: "job",
                header: "任务",
                meta: { label: "任务" },
                cell: ({ row }) => (
                    <div className="min-w-0">
                        <p className="truncate font-medium">
                            {jobLabel(row.original)}
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
                        label={JOB_STATUS_LABELS[row.original.status]}
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
                        {formatJobDateTime(row.original.created_at)}
                    </span>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", role: "preview" },
                enableSorting: false,
                cell: ({ row }) => (
                    <TableRowActions
                        moreId={`${ID_PREFIX}-row-${row.original.id}-more`}
                        moreLabel={`${jobLabel(row.original)} 更多操作`}
                        actions={[
                            {
                                id: `${ID_PREFIX}-row-${row.original.id}-preview`,
                                label: "查看",
                                onClick: () => {
                                    lastFocusedRowId.current = row.original.id
                                    openPreview(row.original)
                                },
                            },
                        ]}
                    />
                ),
            },
        ],
        [jobLabel, openPreview],
    )

    return (
        <PageScaffold density="compact" className={styles.page}>
            <ListWorkspaceHeader
                eyebrow="治理"
                title="后台任务"
                description="集中查看商品导入、业务导出等后台任务的进度与结果，取消尚未完成的任务。"
            >
                {isAdmin ? (
                    <Button
                        id={`${ID_PREFIX}-stop-all`}
                        type="button"
                        variant="destructive"
                        onClick={openStopAll}
                    >
                        停止并取消所有任务
                    </Button>
                ) : null}
            </ListWorkspaceHeader>
            <ListWorkSurface
                ariaLabel="后台任务"
                toolbar={
                    <ListWorkspaceFilterBar
                        density="compact"
                        idPrefix={`${ID_PREFIX}-toolbar`}
                        formAriaLabel="后台任务查询"
                        onSubmit={applyFilters}
                        search={
                            <ListSearchField
                                id={`${ID_PREFIX}-toolbar-search-input`}
                                searchInputRef={searchInputRef}
                                value={searchDraft}
                                onChange={setSearchDraft}
                                placeholder="按任务号搜索"
                                aria-label="按任务号搜索后台任务"
                            />
                        }
                        commonFilters={
                            <div className="grid w-full min-w-0 gap-3 sm:grid-cols-2 xl:flex xl:flex-wrap xl:items-center xl:gap-x-6">
                                <ListWorkspaceInlineFilter
                                    className="lg:not-first:border-l-0 lg:not-first:pl-0 xl:not-first:border-l xl:not-first:pl-6 max-xl:[&>label]:w-14"
                                    htmlFor={`${ID_PREFIX}-toolbar-status`}
                                    label="状态"
                                >
                                    <OptionCombobox
                                        id={`${ID_PREFIX}-toolbar-status`}
                                        className="w-full xl:w-36"
                                        value={statusDraft}
                                        onValueChange={(value) =>
                                            setStatusDraft(
                                                (value ??
                                                    "all") as StatusFilter,
                                            )
                                        }
                                        options={STATUS_FILTER_OPTIONS}
                                        aria-label="状态"
                                        placeholder="全部状态"
                                        allowClear={false}
                                    />
                                </ListWorkspaceInlineFilter>
                                <ListWorkspaceInlineFilter
                                    className="lg:not-first:border-l-0 lg:not-first:pl-0 xl:not-first:border-l xl:not-first:pl-6 max-xl:[&>label]:w-14"
                                    htmlFor={`${ID_PREFIX}-toolbar-job-type`}
                                    label="任务类型"
                                >
                                    <OptionCombobox
                                        id={`${ID_PREFIX}-toolbar-job-type`}
                                        className="w-full xl:w-36"
                                        value={jobTypeDraft}
                                        onValueChange={(value) =>
                                            setJobTypeDraft(value ?? "")
                                        }
                                        options={JOB_TYPE_FILTER_OPTIONS}
                                        aria-label="任务类型"
                                        placeholder="全部类型"
                                        allowClear={false}
                                    />
                                </ListWorkspaceInlineFilter>
                                <ListWorkspaceInlineFilter
                                    className="lg:not-first:border-l-0 lg:not-first:pl-0 xl:not-first:border-l xl:not-first:pl-6 max-xl:[&>label]:w-14"
                                    htmlFor={`${ID_PREFIX}-toolbar-domain`}
                                    label="业务类型"
                                >
                                    <OptionCombobox
                                        id={`${ID_PREFIX}-toolbar-domain`}
                                        className="w-full xl:w-48"
                                        value={domainDraft}
                                        onValueChange={(value) =>
                                            setDomainDraft(value ?? "")
                                        }
                                        options={JOB_DOMAIN_FILTER_OPTIONS}
                                        aria-label="业务类型"
                                        placeholder="全部业务"
                                        allowClear={false}
                                    />
                                </ListWorkspaceInlineFilter>
                                {isAdmin ? (
                                    <ListWorkspaceInlineFilter
                                        className="lg:not-first:border-l-0 lg:not-first:pl-0 xl:not-first:border-l xl:not-first:pl-6 max-xl:[&>label]:w-14"
                                        htmlFor={`${ID_PREFIX}-toolbar-scope`}
                                        label="可见范围"
                                    >
                                        <OptionCombobox
                                            id={`${ID_PREFIX}-toolbar-scope`}
                                            className="w-full xl:w-36"
                                            value={scopeDraft}
                                            onValueChange={(value) =>
                                                setScopeDraft(
                                                    (value ??
                                                        "all") as ScopeFilter,
                                                )
                                            }
                                            options={SCOPE_FILTER_OPTIONS}
                                            aria-label="可见范围"
                                            placeholder="全部任务"
                                            allowClear={false}
                                        />
                                    </ListWorkspaceInlineFilter>
                                ) : null}
                            </div>
                        }
                        resultStatus={listWorkspaceFilterStatusText({
                            loading: jobsQuery.isPending,
                            failed: jobsQuery.isError,
                            resultCount: jobsQuery.data?.total,
                            noun: "个任务",
                        })}
                        chips={[
                            ...(isAdmin && appliedScope === "mine"
                                ? [
                                      {
                                          key: "scope",
                                          label: "范围：只看我的",
                                      },
                                  ]
                                : []),
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
                            ...(appliedJobType
                                ? [
                                      {
                                          key: "job_type",
                                          label: `任务类型：${
                                              JOB_TYPE_LABELS[appliedJobType] ??
                                              appliedJobType
                                          }`,
                                      },
                                  ]
                                : []),
                            ...(appliedDomain
                                ? [
                                      {
                                          key: "domain",
                                          label: `业务类型：${backgroundJobDomainLabel(appliedDomain, null)}`,
                                      },
                                  ]
                                : []),
                        ]}
                        onClearChip={(key) => {
                            if (key === "scope") {
                                setScopeDraft("all")
                                setAppliedScope("all")
                            }
                            if (key === "job_no") {
                                setSearchDraft("")
                                setAppliedJobNo("")
                            }
                            if (key === "status") {
                                setStatusDraft("all")
                                setAppliedStatus("all")
                            }
                            if (key === "job_type") {
                                setJobTypeDraft("")
                                setAppliedJobType("")
                            }
                            if (key === "domain") {
                                setDomainDraft("")
                                setAppliedDomain("")
                            }
                            setPage(1)
                        }}
                        onClearAll={clearFilters}
                        hasPendingChanges={hasPendingChanges}
                        idleHint={
                            isAdmin ? undefined : "仅显示我创建的后台任务"
                        }
                    />
                }
                table={
                    <DataTable
                        id={`${ID_PREFIX}-table`}
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
                        loading={jobsQuery.isPending}
                        showRefreshingBanner={false}
                        layout="flush"
                        onRowPreview={openPreview}
                        highlightedRowId={previewId ?? undefined}
                        errorState={
                            jobsQuery.isError ? (
                                <BusinessFailureState
                                    error={jobsQuery.error}
                                    action={
                                        <Button
                                            id={`${ID_PREFIX}-retry`}
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
                                            : "还没有后台任务"
                                    }
                                    description={
                                        hasActiveFilters
                                            ? "没有任务符合当前筛选条件，可清除筛选后重试。"
                                            : "在商品列表导入报价表，或在销售单、库存台账等页面发起导出后，进度会集中显示在这里。"
                                    }
                                    action={
                                        hasActiveFilters ? (
                                            <Button
                                                id={`${ID_PREFIX}-empty-clear-filters`}
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
                idPrefix={`${ID_PREFIX}-preview-sheet`}
                open={previewId != null}
                onOpenChange={(open) => {
                    if (!open) closePreview()
                }}
                size="preview"
                contentClassName="data-[side=right]:sm:w-[460px] data-[side=right]:sm:max-w-[460px]"
                title={previewJob ? jobLabel(previewJob) : "后台任务"}
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
                                label={JOB_STATUS_LABELS[previewJob.status]}
                            />
                            <span className="text-xs text-muted-foreground">
                                {jobLabel(previewJob)}
                            </span>
                        </div>
                    ) : null
                }
                footer={
                    previewId ? (
                        <>
                            <Button
                                id={`${ID_PREFIX}-preview-close`}
                                type="button"
                                variant="outline"
                                onClick={closePreview}
                            >
                                关闭
                            </Button>
                            {previewJob &&
                            isJobActive(
                                previewJob.status,
                                previewJob.finished_at,
                            ) ? (
                                <Button
                                    id={`${ID_PREFIX}-preview-cancel`}
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
                {detailQuery.isPending && !previewJob ? (
                    <p className="text-sm text-muted-foreground">
                        正在加载任务详情…
                    </p>
                ) : previewJob ? (
                    <div className="space-y-6 text-sm">
                        <BackgroundJobProgress
                            mode="partialAllowed"
                            status={jobProgressStatus(previewJob.status)}
                            total={previewJob.total_count}
                            completed={previewJob.processed_count}
                            succeeded={previewJob.success_count}
                            skipped={previewJob.skipped_count}
                            failed={previewJob.failed_count}
                            label={jobLabel(previewJob)}
                            description={
                                previewJob.error_summary
                                    ? previewJob.error_summary
                                    : "任务在后台逐项执行，已完成的部分不受未完成部分影响。"
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
                                        {formatJobDateTime(
                                            previewJob.created_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>开始时间</DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatJobDateTime(
                                            previewJob.started_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>结束时间</DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatJobDateTime(
                                            previewJob.finished_at,
                                        )}
                                    </DescriptionDetails>
                                </DescriptionItem>
                                <DescriptionItem>
                                    <DescriptionTerm>
                                        结果有效期至
                                    </DescriptionTerm>
                                    <DescriptionDetails className="num">
                                        {formatJobDateTime(
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
                            {(itemsQuery.data?.total ?? 0) > 100 && (
                                <div className="mt-3 flex items-center justify-end gap-3">
                                    <Button
                                        id={`${ID_PREFIX}-items-prev`}
                                        variant="outline"
                                        disabled={itemsPage === 1}
                                        onClick={() =>
                                            setItemsPage((page) => page - 1)
                                        }
                                    >
                                        上一页
                                    </Button>
                                    <span className="text-sm">
                                        第 {itemsPage} 页，共{" "}
                                        {Math.ceil(
                                            (itemsQuery.data?.total ?? 0) / 100,
                                        )}{" "}
                                        页
                                    </span>
                                    <Button
                                        id={`${ID_PREFIX}-items-next`}
                                        variant="outline"
                                        disabled={
                                            itemsPage * 100 >=
                                            (itemsQuery.data?.total ?? 0)
                                        }
                                        onClick={() =>
                                            setItemsPage((page) => page + 1)
                                        }
                                    >
                                        下一页
                                    </Button>
                                </div>
                            )}
                            {previewJob.domain_job_type === "SUPPLIER_IMPORT" &&
                                previewJob.finished_at !== null &&
                                previewJob.requested_by === currentUserId &&
                                (previewJob.failed_count > 0 ||
                                    previewJob.processed_count <
                                        previewJob.total_count) && (
                                    <div className="mt-4">
                                        <SupplierImportFailures
                                            key={previewJob.id}
                                            jobId={previewJob.id}
                                        />
                                    </div>
                                )}
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
                                                    {item.source_row_no
                                                        ? `Excel 第 ${item.source_row_no} 行`
                                                        : `第 ${item.item_no} 项`}
                                                </span>
                                                <span className="text-xs">
                                                    {item.result_code ===
                                                    "outcome_unknown"
                                                        ? "结果待确认"
                                                        : ({
                                                              success: "成功",
                                                              skipped: "跳过",
                                                              failed: "执行失败",
                                                          }[
                                                              item.status ?? ""
                                                          ] ?? "待执行")}
                                                </span>
                                            </div>
                                            {item.object_type ===
                                                "supplier_import_row" && (
                                                <p className="mt-1 font-medium">
                                                    {item.object_id}
                                                </p>
                                            )}
                                            <p className="mt-1 text-[13px]">
                                                {item.result_summary ||
                                                    "排队执行中"}
                                            </p>
                                        </li>
                                    ))}
                                </ul>
                            )}
                            {itemsQuery.isError && (
                                <BusinessFailureState
                                    error={itemsQuery.error}
                                    action={
                                        <Button
                                            id={`${ID_PREFIX}-items-retry`}
                                            variant="outline"
                                            onClick={() =>
                                                void itemsQuery.refetch()
                                            }
                                        >
                                            重试
                                        </Button>
                                    }
                                />
                            )}
                        </section>
                    </div>
                ) : detailQuery.isError ? (
                    <BusinessFailureState
                        error={detailQuery.error}
                        action={
                            <Button
                                id={`${ID_PREFIX}-preview-retry`}
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                        }
                    />
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
                        <AlertDialogTitle>取消任务</AlertDialogTitle>
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
                        <AlertDialogCancel id={`${ID_PREFIX}-cancel-back`}>
                            返回
                        </AlertDialogCancel>
                        <AlertDialogAction
                            id={`${ID_PREFIX}-cancel-confirm`}
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
            <AlertDialog
                open={stopAllOpen}
                onOpenChange={(open) => {
                    if (!open) {
                        setStopAllOpen(false)
                        setStopAllError(null)
                    }
                }}
            >
                <AlertDialogContent size="sm">
                    <AlertDialogHeader>
                        <AlertDialogTitle>停止并取消所有任务</AlertDialogTitle>
                        <AlertDialogDescription>
                            将停止并取消当前全部未完成的后台任务，包括他人创建的任务。已经完成的部分不受影响，没有进行中的任务时不做任何处理。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    {stopAllResult ? (
                        <p
                            className="text-sm text-muted-foreground"
                            role="status"
                        >
                            已取消 {stopAllResult.cancelled_count} 个任务
                            {stopAllResult.skipped_count > 0
                                ? `，跳过 ${stopAllResult.skipped_count} 个已结束任务`
                                : ""}
                            {stopAllResult.failed_count > 0
                                ? `，${stopAllResult.failed_count} 个取消失败，可重试`
                                : ""}
                            。
                        </p>
                    ) : null}
                    {stopAllError ? (
                        <p className="text-sm text-destructive" role="alert">
                            {stopAllError}
                        </p>
                    ) : null}
                    <AlertDialogFooter>
                        {stopAllResult ? (
                            <AlertDialogCancel
                                id={`${ID_PREFIX}-stop-all-close`}
                            >
                                关闭
                            </AlertDialogCancel>
                        ) : (
                            <>
                                <AlertDialogCancel
                                    id={`${ID_PREFIX}-stop-all-back`}
                                >
                                    返回
                                </AlertDialogCancel>
                                <AlertDialogAction
                                    id={`${ID_PREFIX}-stop-all-confirm`}
                                    disabled={cancelAllMutation.isPending}
                                    onClick={() => {
                                        void confirmStopAll()
                                    }}
                                >
                                    {cancelAllMutation.isPending
                                        ? "停止中…"
                                        : "确认停止"}
                                </AlertDialogAction>
                            </>
                        )}
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </PageScaffold>
    )
}
