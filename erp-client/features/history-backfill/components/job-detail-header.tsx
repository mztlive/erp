"use client"

import { ArrowLeftIcon, DownloadIcon, RefreshCwIcon } from "lucide-react"

import { DocumentHeader, PageHeader } from "@/components/business"
import { Button } from "@/components/ui/button"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import type {
    HistoryBackfillJobCore,
    HistoryBackfillReportView,
} from "@/features/history-backfill/types"
import {
    ENVIRONMENT_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
} from "@/features/history-backfill/types"

export function JobDetailHeader({
    job,
    report,
    isFetching,
    pending,
    canValidate,
    canStart,
    canResume,
    canConfirmReport,
    onBack,
    onRefresh,
    onValidate,
    onStart,
    onResume,
    onConfirmReport,
    onDownloadReport,
}: {
    job: HistoryBackfillJobCore
    report?: HistoryBackfillReportView
    isFetching: boolean
    pending: boolean
    canValidate: boolean
    canStart: boolean
    canResume: boolean
    canConfirmReport: boolean
    onBack: () => void
    onRefresh: () => void
    onValidate: () => void
    onStart: () => void
    onResume: () => void
    onConfirmReport: () => void
    onDownloadReport: () => void
}) {
    const primaryProcessing = {
        label: PROCESSING_STATUS_LABEL[job.processingStatus],
        tone: PROCESSING_STATUS_TONE[job.processingStatus],
    }

    return (
        <>
            <PageHeader
                variant="object-chrome"
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="operations-history-backfill-detail-back"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon className="size-4" />
                            返回任务列表
                        </Button>
                        <Button
                            id="operations-history-backfill-detail-refresh"
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-muted-foreground"
                            disabled={isFetching}
                            onClick={onRefresh}
                        >
                            <RefreshCwIcon
                                className={isFetching ? "animate-spin" : ""}
                                aria-hidden
                            />
                            刷新
                        </Button>
                    </div>
                }
            />

            <DocumentHeader
                density="compact"
                title={job.mallName}
                documentNumber={job.jobNo}
                primaryStatus={primaryProcessing}
                version={`版本 ${job.lockVersion}`}
                meta={
                    <span className="text-muted-foreground">
                        {ENVIRONMENT_LABEL[job.environment]} · 范围起点{" "}
                        {formatDay(job.rangeStart)} 至 截止时点{" "}
                        {formatDay(job.rangeEnd)}
                    </span>
                }
                statuses={[
                    {
                        id: "report",
                        label: "报告确认",
                        status: {
                            label: REPORT_REVIEW_STATUS_LABEL[
                                job.reportReviewStatus
                            ],
                            tone: REPORT_REVIEW_STATUS_TONE[
                                job.reportReviewStatus
                            ],
                        },
                    },
                    {
                        id: "mall",
                        label: "商城",
                        status: {
                            label: `${job.mallName} · ${ENVIRONMENT_LABEL[job.environment]}`,
                            tone:
                                job.environment === "production"
                                    ? "destructive"
                                    : "info",
                        },
                    },
                    {
                        id: "downstream",
                        label: "后续流程",
                        status: {
                            label: job.formalDownstreamUnlocked
                                ? "已可用"
                                : "保持关闭",
                            tone: job.formalDownstreamUnlocked
                                ? "success"
                                : "warning",
                        },
                    },
                ]}
                secondaryActions={
                    <>
                        {canValidate ? (
                            <Button
                                id="operations-history-backfill-detail-validate-source"
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={pending}
                                onClick={() => void onValidate()}
                            >
                                校验来源
                            </Button>
                        ) : null}
                        {canStart ? (
                            <Button
                                id="operations-history-backfill-detail-start"
                                type="button"
                                size="sm"
                                disabled={pending}
                                onClick={onStart}
                            >
                                开始回填
                            </Button>
                        ) : null}
                        {canResume ? (
                            <Button
                                id="operations-history-backfill-detail-resume"
                                type="button"
                                size="sm"
                                disabled={pending}
                                onClick={onResume}
                            >
                                续跑原任务
                            </Button>
                        ) : null}
                        {canConfirmReport ? (
                            <Button
                                id="operations-history-backfill-detail-confirm-report"
                                type="button"
                                size="sm"
                                disabled={pending}
                                onClick={onConfirmReport}
                            >
                                确认报告
                            </Button>
                        ) : null}
                        {report ? (
                            <Button
                                id="operations-history-backfill-detail-header-download-report"
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onDownloadReport}
                            >
                                <DownloadIcon className="size-4" />
                                下载报告（示例）
                            </Button>
                        ) : null}
                    </>
                }
            />
        </>
    )
}
