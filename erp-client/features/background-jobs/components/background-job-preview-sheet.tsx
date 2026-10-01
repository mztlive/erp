"use client"

import type { Dispatch, SetStateAction } from "react"
import {
    BackgroundJobProgress,
    BusinessFailureState,
    QuickPreviewSheet,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge } from "@/components/ui/status-badge"
import { ScrollArea } from "@/components/ui/scroll-area"
import {
    backgroundJobDomainLabel,
    formatJobDateTime,
    isJobActive,
    JOB_STATUS_LABELS,
    jobProgressStatus,
} from "../labels"
import type {
    useBackgroundJobDetailQuery,
    useBackgroundJobItemsQuery,
} from "../queries"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"
import { BackgroundJobItemResults } from "./background-job-item-results"

type BackgroundJobPreviewSheetProps = {
    previewId: string | null
    currentUserId: string | undefined
    detailQuery: ReturnType<typeof useBackgroundJobDetailQuery>
    itemsQuery: ReturnType<typeof useBackgroundJobItemsQuery>
    itemsPage: number
    onItemsPageChange: Dispatch<SetStateAction<number>>
    cancelPending: boolean
    onClose: () => void
    onCancel: (jobId: string) => void
}

/** 任务预览保持受控，查询订阅与焦点恢复由工作台负责。 */
export function BackgroundJobPreviewSheet({
    previewId,
    currentUserId,
    detailQuery,
    itemsQuery,
    itemsPage,
    onItemsPageChange,
    cancelPending,
    onClose,
    onCancel,
}: BackgroundJobPreviewSheetProps) {
    const previewJob = detailQuery.data ?? null
    const jobLabel = (job: NonNullable<typeof previewJob>) =>
        backgroundJobDomainLabel(job.domain_job_type, job.job_type)
    return (
        <QuickPreviewSheet
            idPrefix={`${ID_PREFIX}-preview-sheet`}
            open={previewId != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            size="detail"
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
                            onClick={onClose}
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
                                disabled={cancelPending}
                                onClick={() => onCancel(previewJob.id)}
                            >
                                {cancelPending ? "取消中…" : "取消任务"}
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
        >
            <ScrollArea className="min-h-0 flex-1">
                <div className="px-7 py-6">
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
                                        <DescriptionTerm>
                                            发起人
                                        </DescriptionTerm>
                                        <DescriptionDetails>
                                            {previewJob.requested_by || "—"}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            创建时间
                                        </DescriptionTerm>
                                        <DescriptionDetails className="num">
                                            {formatJobDateTime(
                                                previewJob.created_at,
                                            )}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            开始时间
                                        </DescriptionTerm>
                                        <DescriptionDetails className="num">
                                            {formatJobDateTime(
                                                previewJob.started_at,
                                            )}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                    <DescriptionItem>
                                        <DescriptionTerm>
                                            结束时间
                                        </DescriptionTerm>
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
                                        <DescriptionTerm>
                                            目标总数
                                        </DescriptionTerm>
                                        <DescriptionDetails className="num">
                                            {previewJob.total_count}
                                        </DescriptionDetails>
                                    </DescriptionItem>
                                </DescriptionList>
                            </section>
                            <BackgroundJobItemResults
                                previewJob={previewJob}
                                currentUserId={currentUserId}
                                itemsPage={itemsPage}
                                onPageChange={onItemsPageChange}
                                itemsQuery={itemsQuery}
                            />
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
                </div>
            </ScrollArea>
        </QuickPreviewSheet>
    )
}
