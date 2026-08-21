"use client"

import type { ColumnDef, PaginationState } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    BusinessTableFrame,
    DataTable,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { MallSyncJobRow } from "@/features/mall-sync/types"
import { JOB_ERROR_CLASS_LABEL } from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"

type MallSyncJobsViewProps = {
    selectedJob: MallSyncJobRow | undefined
    totalJobs: number
    pageJobs: MallSyncJobRow[]
    jobColumns: ColumnDef<MallSyncJobRow>[]
    pagination: PaginationState
    onPaginationChange: (
        next: PaginationState | ((current: PaginationState) => PaginationState),
    ) => void
    retryPending: boolean
    onRetryJob: () => void
}

export function MallSyncJobsView({
    selectedJob,
    totalJobs,
    pageJobs,
    jobColumns,
    pagination,
    onPaginationChange,
    retryPending,
    onRetryJob,
}: MallSyncJobsViewProps) {
    return (
        <div className="grid gap-4 xl:grid-cols-[minmax(0,1.4fr)_minmax(18rem,1fr)]">
            <BusinessTableFrame
                title="同步任务"
                description="基线 / 增量 / 单号补拉。同步进度不因映射失败回退。"
                table={
                    <DataTable
                        data={pageJobs}
                        columns={jobColumns}
                        getRowId={(r) => r.jobId}
                        rowCount={totalJobs}
                        pagination={pagination}
                        onPaginationChange={onPaginationChange}
                        layout="flush"
                    />
                }
            />
            {selectedJob ? (
                <Card size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-grid">
                        <CardTitle className="text-base">
                            {selectedJob.jobNo}
                        </CardTitle>
                        <CardDescription>
                            {selectedJob.jobTypeLabel} ·{" "}
                            {selectedJob.triggeredBy}
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3 text-sm">
                        <BusinessStatusBadge
                            context="detail"
                            label={selectedJob.statusLabel}
                            tone={selectedJob.statusTone}
                        />
                        {selectedJob.impactSummary ? (
                            <p>{selectedJob.impactSummary}</p>
                        ) : null}
                        {selectedJob.errorClass ? (
                            <p className="text-muted-foreground">
                                错误分类：
                                {JOB_ERROR_CLASS_LABEL[
                                    selectedJob.errorClass
                                ] ?? selectedJob.errorClass}
                            </p>
                        ) : null}
                        <div className="grid grid-cols-2 gap-2 text-xs text-muted-foreground">
                            <span>
                                游标前{" "}
                                {selectedJob.cursorBefore
                                    ? formatDateTime(
                                          selectedJob.cursorBefore,
                                          "default",
                                      )
                                    : "—"}
                            </span>
                            <span>
                                游标后{" "}
                                {selectedJob.cursorAfter
                                    ? formatDateTime(
                                          selectedJob.cursorAfter,
                                          "default",
                                      )
                                    : "—"}
                            </span>
                        </div>
                        {selectedJob.allowedActions.includes(
                            "RETRY_FAILED_JOB",
                        ) ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="secondary"
                                disabled={retryPending}
                                onClick={onRetryJob}
                            >
                                重试失败任务
                            </Button>
                        ) : null}
                        {selectedJob.actionBlockers.map((b) => (
                            <p
                                key={b.code}
                                className="text-xs text-warning-soft-foreground"
                            >
                                {b.message}
                            </p>
                        ))}
                    </CardContent>
                </Card>
            ) : null}
        </div>
    )
}
