"use client"

import * as React from "react"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DataTable,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkSurface,
    ListWorkspaceHeader,
    listWorkspaceEmptyStateClassName,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api/errors"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { BackgroundJobView, CancelAllBackgroundJobsResult } from "../api"
import { isBackgroundJobAdmin, isJobActive } from "../labels"
import {
    useBackgroundJobDetailQuery,
    useBackgroundJobItemsQuery,
    useBackgroundJobsQuery,
    useCancelAllBackgroundJobsMutation,
    useCancelBackgroundJobMutation,
} from "../queries"
import { useBackgroundJobsFilters } from "../hooks/use-background-jobs-filters"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"
import { BackgroundJobsToolbar } from "./background-jobs-toolbar"
import { backgroundJobColumns } from "./background-job-columns"
import { BackgroundJobPreviewSheet } from "./background-job-preview-sheet"
import {
    BackgroundJobCancelDialog,
    BackgroundJobsStopAllDialog,
} from "./background-job-cancel-dialogs"

/** 后台任务工作台统一编排查询、预览焦点与取消命令。 */
export function BackgroundJobsWorkspace() {
    const profileQuery = useAccountProfileQuery()
    const isAdmin = isBackgroundJobAdmin(profileQuery.data?.role_ids)
    const currentUserId = profileQuery.data?.userid
    const filters = useBackgroundJobsFilters(isAdmin, currentUserId)
    const { page, setPage, hasActiveFilters, clearAll: clearFilters } = filters
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

    const jobsQuery = useBackgroundJobsQuery(filters.listParams)
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

    const columns = React.useMemo(
        () => backgroundJobColumns(openPreview),
        [openPreview],
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
                    <BackgroundJobsToolbar
                        filters={filters}
                        isAdmin={isAdmin}
                        jobsQuery={jobsQuery}
                        searchInputRef={searchInputRef}
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
            <BackgroundJobPreviewSheet
                previewId={previewId}
                currentUserId={currentUserId}
                detailQuery={detailQuery}
                itemsQuery={itemsQuery}
                itemsPage={itemsPage}
                onItemsPageChange={setItemsPage}
                cancelPending={cancelMutation.isPending}
                onClose={closePreview}
                onCancel={setCancelId}
            />
            <BackgroundJobCancelDialog
                open={cancelId != null}
                onOpenChange={(open) => {
                    if (!open) {
                        setCancelId(null)
                        setCancelError(null)
                    }
                }}
                error={cancelError}
                pending={cancelMutation.isPending}
                onConfirm={confirmCancel}
            />
            <BackgroundJobsStopAllDialog
                open={stopAllOpen}
                onOpenChange={(open) => {
                    if (!open) {
                        setStopAllOpen(false)
                        setStopAllError(null)
                    }
                }}
                error={stopAllError}
                result={stopAllResult}
                pending={cancelAllMutation.isPending}
                onConfirm={confirmStopAll}
            />
        </PageScaffold>
    )
}
