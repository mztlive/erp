"use client"

import type { Dispatch, SetStateAction } from "react"
import { BusinessFailureState, QuickPreviewSheet } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    backgroundJobDomainLabel,
    backgroundJobIdentity,
    formatJobDateTime,
    isJobActive,
    JOB_STATUS_LABELS,
} from "../labels"
import type {
    useBackgroundJobDetailQuery,
    useBackgroundJobItemsQuery,
} from "../queries"
import { BACKGROUND_JOBS_ID_PREFIX as ID_PREFIX } from "../lib/constants"
import { BackgroundJobItemResults } from "./background-job-item-results"
import { DataTransferTaskResult } from "./data-transfer-task-result"
import { DirectImportResult } from "./direct-import-result"
import { DomainJobResult } from "./domain-job-result"
import { canCancelJob } from "../labels"

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
    const previewJob = detailQuery.isError ? null : (detailQuery.data ?? null)
    const identity = previewJob ? backgroundJobIdentity(previewJob) : null
    return (
        <QuickPreviewSheet
            idPrefix={`${ID_PREFIX}-preview-sheet`}
            open={previewId != null}
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
            size="preview"
            title={identity?.title ?? "后台任务"}
            identity={
                identity ? (
                    <span className="num break-all">
                        任务号：{identity.number}
                    </span>
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
                            {backgroundJobDomainLabel(
                                previewJob.domain_job_type,
                                previewJob.job_type,
                            )}
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
                        {previewJob && canCancelJob(previewJob) ? (
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
            {detailQuery.isPending && !previewJob ? (
                <p className="text-sm text-muted-foreground">
                    正在加载任务详情…
                </p>
            ) : previewJob ? (
                <div className="space-y-6 text-sm">
                    {previewJob.domain_job_type === "DATA_TRANSFER" ? (
                        previewJob.requested_by === currentUserId ? (
                            <DataTransferTaskResult
                                key={previewJob.id}
                                job={previewJob}
                            />
                        ) : (
                            <p className="text-muted-foreground">
                                此任务由其他用户发起。业务结果、文件下载与继续办理仅供发起人使用。
                            </p>
                        )
                    ) : previewJob.domain_job_type === "PRODUCT_IMPORT" ||
                      previewJob.domain_job_type === "SUPPLIER_IMPORT" ? (
                        <DirectImportResult
                            job={previewJob}
                            currentUserId={currentUserId}
                        />
                    ) : (
                        <DomainJobResult job={previewJob} />
                    )}
                    {previewJob.error_summary && (
                        <p role="alert" className="text-destructive">
                            {previewJob.error_summary}
                        </p>
                    )}
                    {previewJob.domain_job_type === "DATA_TRANSFER" &&
                        previewJob.job_type === "import" &&
                        isJobActive(
                            previewJob.status,
                            previewJob.finished_at,
                        ) &&
                        !canCancelJob(previewJob) && (
                            <p className="text-muted-foreground">
                                此任务已开始处理，不能中途取消，请等待执行结果。
                            </p>
                        )}
                    <section className="space-y-3">
                        <h3 className="font-medium">任务资料</h3>
                        <DescriptionList columns="one">
                            <DescriptionItem>
                                <DescriptionTerm>发起人</DescriptionTerm>
                                <DescriptionDetails>
                                    {previewJob.requested_by === currentUserId
                                        ? "我"
                                        : "其他用户"}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>创建时间</DescriptionTerm>
                                <DescriptionDetails className="num">
                                    {formatJobDateTime(previewJob.created_at)}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>开始时间</DescriptionTerm>
                                <DescriptionDetails className="num">
                                    {formatJobDateTime(previewJob.started_at)}
                                </DescriptionDetails>
                            </DescriptionItem>
                            <DescriptionItem>
                                <DescriptionTerm>结束时间</DescriptionTerm>
                                <DescriptionDetails className="num">
                                    {formatJobDateTime(previewJob.finished_at)}
                                </DescriptionDetails>
                            </DescriptionItem>
                        </DescriptionList>
                    </section>
                    {previewJob.domain_job_type !== "DATA_TRANSFER" && (
                        <BackgroundJobItemResults
                            previewJob={previewJob}
                            currentUserId={currentUserId}
                            itemsPage={itemsPage}
                            onPageChange={onItemsPageChange}
                            itemsQuery={itemsQuery}
                        />
                    )}
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
    )
}
