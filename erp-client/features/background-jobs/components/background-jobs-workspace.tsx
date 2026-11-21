"use client"

import * as React from "react"
import {
    ListDataTable,
    ListWorkspacePage,
} from "@/components/business/list-workspace"
import { getErrorMessage } from "@/lib/api/errors"
import { restoreListRowFocus } from "@/lib/list-row-focus"
import { hasPermission } from "@/lib/permissions"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type { BackgroundJobView, CancelAllBackgroundJobsResult } from "../api"
import {
    backgroundJobIdentity,
    isBackgroundJobAdmin,
    isJobActive,
} from "../labels"
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
    const permissions = profileQuery.data?.permissions
    const canReadDetail = hasPermission(permissions, "background_job:detail")
    const canReadItems = hasPermission(permissions, "background_job_item:list")
    const canCancel = hasPermission(permissions, "background_job:cancel")
    const canStopAll =
        isAdmin && hasPermission(permissions, "background_job:cancel_all")
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
    useSearchHotkey(searchInputRef)

    const jobsQuery = useBackgroundJobsQuery(filters.listParams)
    const authorizedPreviewId = canReadDetail ? previewId : null
    const detailQuery = useBackgroundJobDetailQuery(authorizedPreviewId)
    const itemsQuery = useBackgroundJobItemsQuery(
        canReadItems &&
            detailQuery.data &&
            detailQuery.data.domain_job_type !== "PRODUCT_IMPORT"
            ? authorizedPreviewId
            : null,
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
        restoreListRowFocus(lastFocusedRowId.current)
    }, [])

    const confirmCancel = React.useCallback(async () => {
        if (!canCancel || !cancelId || !detailQuery.data) return
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
    }, [canCancel, cancelId, detailQuery.data, cancelMutation])

    const openStopAll = React.useCallback(() => {
        setStopAllError(null)
        setStopAllResult(null)
        setStopAllOpen(true)
    }, [])

    const confirmStopAll = React.useCallback(async () => {
        if (!canStopAll) return
        setStopAllError(null)
        try {
            const result = await cancelAllMutation.mutateAsync(undefined)
            setStopAllResult(result)
        } catch (error) {
            setStopAllError(
                getErrorMessage(error, "停止全部任务失败，请重试。"),
            )
        }
    }, [canStopAll, cancelAllMutation])

    const columns = React.useMemo(
        () => backgroundJobColumns(canReadDetail ? openPreview : undefined),
        [canReadDetail, openPreview],
    )

    return (
        <ListWorkspacePage
            title="后台任务"
            actions={
                canStopAll
                    ? [
                          {
                              id: `${ID_PREFIX}-stop-all`,
                              actionKey: "stop-all",
                              type: "button",
                              label: "停止并取消所有任务",
                              tier: "quiet",
                              className:
                                  "text-destructive hover:bg-destructive-soft hover:text-destructive",
                              onClick: openStopAll,
                          },
                      ]
                    : undefined
            }
            loading={jobsQuery.isPending && !jobsQuery.isFetched}
            surface={{
                ariaLabel: "后台任务",
                tableActions: isAdmin ? undefined : (
                    <span className="hidden text-xs text-muted-foreground xl:inline">
                        仅显示我创建的后台任务
                    </span>
                ),
                toolbar: (
                    <BackgroundJobsToolbar
                        filters={filters}
                        isAdmin={isAdmin}
                        jobsQuery={jobsQuery}
                        searchInputRef={searchInputRef}
                    />
                ),
                tableClassName:
                    "max-sm:[&_[data-column-id=job]]:min-w-table-column-default-min",
                table: (
                    <ListDataTable
                        id={`${ID_PREFIX}-table`}
                        entityName="后台任务"
                        data={rows}
                        columns={columns}
                        defaultColumnOrder={[
                            "job",
                            "status",
                            "progress",
                            "created",
                            "actions",
                        ]}
                        defaultColumnPinning={{
                            left: ["job"],
                            right: ["actions"],
                        }}
                        getRowId={(row) => row.id}
                        rowLabel={(row) => {
                            const { title, number } = backgroundJobIdentity(row)
                            return `${title}（${number}）`
                        }}
                        rowCount={jobsQuery.data?.total ?? 0}
                        pagination={{
                            pageIndex: page - 1,
                            pageSize: filters.listParams.page_size,
                        }}
                        pageSizeOptions={[filters.listParams.page_size]}
                        onPaginationChange={(next) => {
                            setPage(next.pageIndex + 1)
                        }}
                        loading={jobsQuery.isFetching}
                        showRefreshingBanner={jobsQuery.isPlaceholderData}
                        isError={
                            jobsQuery.isError ||
                            (!jobsQuery.isPending && !jobsQuery.data)
                        }
                        error={jobsQuery.error}
                        onRetry={() => void jobsQuery.refetch()}
                        retryButtonId={`${ID_PREFIX}-retry`}
                        hasActiveFilters={hasActiveFilters}
                        onClearFilters={clearFilters}
                        clearFiltersButtonId={`${ID_PREFIX}-empty-clear-filters`}
                        emptyCopy={{
                            filterDescription:
                                "没有任务符合当前筛选条件，可清除筛选后重试。",
                            emptyDescription:
                                "在商品列表导入报价表，或在销售单、库存台账等页面发起导出后，进度会集中显示在这里。",
                        }}
                        onRowPreview={canReadDetail ? openPreview : undefined}
                        onRowOpen={canReadDetail ? openPreview : undefined}
                        highlightedRowId={authorizedPreviewId ?? undefined}
                    />
                ),
            }}
        >
            <BackgroundJobPreviewSheet
                previewId={authorizedPreviewId}
                canCancel={canCancel}
                canReadItems={canReadItems}
                currentUserId={currentUserId}
                identityError={profileQuery.error}
                onRetryIdentity={() => void profileQuery.refetch()}
                detailQuery={detailQuery}
                itemsQuery={itemsQuery}
                itemsPage={itemsPage}
                onItemsPageChange={setItemsPage}
                cancelPending={cancelMutation.isPending}
                onClose={closePreview}
                onCancel={setCancelId}
            />
            <BackgroundJobCancelDialog
                open={canCancel && canReadDetail && cancelId != null}
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
                open={canStopAll && stopAllOpen}
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
        </ListWorkspacePage>
    )
}
