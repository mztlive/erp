import { DownloadIcon, TriangleAlertIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { HistoryBackfillFact as Fact } from "@/features/history-backfill/components/history-backfill-fact"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import type {
    HistoryBackfillJobCore,
    HistoryBackfillReportView,
} from "@/features/history-backfill/types"
import {
    COST_BASIS_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
} from "@/features/history-backfill/types"
import { formatDateTime } from "@/lib/datetime"

export function ReportSection({
    job,
    report,
    onDownload,
}: {
    job: HistoryBackfillJobCore
    report?: HistoryBackfillReportView
    onDownload: () => void
}) {
    if (!report) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="技术报告尚未生成"
                description="处理状态达到部分完成或技术处理完成后可生成可审计报告。"
                className="rounded-lg border-0 bg-transparent shadow-none ring-0"
            />
        )
    }

    const unconfirmed = report.reviewLabel === "UNCONFIRMED"

    return (
        <div className="space-y-4">
            <Card className={surfacePanelClassName}>
                <CardHeader className="border-b border-grid">
                    <div className="flex flex-wrap items-start justify-between gap-2">
                        <div>
                            <CardTitle>审计报告</CardTitle>
                            <CardDescription>
                                v{report.reportVersion} ·{" "}
                                {formatDateTime(
                                    report.generatedAt,
                                    "dateStyle",
                                )}
                            </CardDescription>
                        </div>
                        <div className="flex flex-wrap gap-2">
                            <Badge
                                variant={unconfirmed ? "outline" : "default"}
                            >
                                {report.downloadLabel}
                            </Badge>
                            <BusinessStatusBadge
                                context="detail"
                                label={
                                    PROCESSING_STATUS_LABEL[
                                        report.processingStatus
                                    ]
                                }
                                tone={
                                    PROCESSING_STATUS_TONE[
                                        report.processingStatus
                                    ]
                                }
                            />
                            <BusinessStatusBadge
                                context="detail"
                                label={
                                    REPORT_REVIEW_STATUS_LABEL[
                                        report.reportReviewStatus
                                    ]
                                }
                                tone={
                                    REPORT_REVIEW_STATUS_TONE[
                                        report.reportReviewStatus
                                    ]
                                }
                            />
                        </div>
                    </div>
                </CardHeader>
                <CardContent className="space-y-4">
                    {unconfirmed ? (
                        <Alert>
                            <TriangleAlertIcon />
                            <AlertTitle>技术报告 · 未确认</AlertTitle>
                            <AlertDescription>
                                报告复核策略未配置或报告未确认时，下载固定标「技术报告
                                ·
                                未确认」。确认动作与下游门禁保持关闭；不得仅因技术完成解锁。
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {report.fullHistoryFinalComplete ? (
                        <Alert>
                            <AlertTitle>全历史回填最终完成</AlertTitle>
                            <AlertDescription>
                                技术处理完成、来源覆盖完整且报告已确认。
                            </AlertDescription>
                        </Alert>
                    ) : (
                        <Alert>
                            <AlertTitle>尚未全历史最终完成</AlertTitle>
                            <AlertDescription>
                                当前不可宣称「全历史回填最终完成」。后续流程：
                                {job.formalDownstreamUnlocked
                                    ? "已可用"
                                    : "保持关闭"}
                                。
                            </AlertDescription>
                        </Alert>
                    )}

                    <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                        <Fact
                            label="范围"
                            value={`${formatDay(report.rangeStart)} 至 ${formatDay(report.rangeEnd)}（截止时点当天除外）`}
                            mono
                        />
                        <Fact
                            label="启用日"
                            value={formatDay(report.cutoverAt)}
                            mono
                        />
                        <Fact
                            label="总笔数"
                            value={report.totalCount.toLocaleString("zh-CN")}
                        />
                        <Fact label="总金额" value={report.totalAmount} />
                        <Fact
                            label="去重"
                            value={report.deduplicatedCount.toLocaleString(
                                "zh-CN",
                            )}
                        />
                        <Fact
                            label="覆盖率"
                            value={report.coverageRate ?? "—"}
                        />
                        <Fact
                            label="报告格式"
                            value={report.schemaVersion}
                            mono
                        />
                        <Fact
                            label="规则版本"
                            value={report.ruleVersion}
                            mono
                        />
                        <Fact label="操作者" value={report.operatorLabel} />
                    </div>

                    <div className="grid gap-3 md:grid-cols-3">
                        {report.costBasis.map((c) => (
                            <div
                                key={c.basis}
                                className="rounded-xl border bg-muted/30 p-3 text-sm"
                            >
                                <div className="font-medium">
                                    {COST_BASIS_LABEL[c.basis]}
                                </div>
                                <div>{c.count.toLocaleString("zh-CN")} 笔</div>
                                <div>{c.consumptionAmountGross}</div>
                                <div className="text-muted-foreground">
                                    成本：
                                    {c.basis === "NONE"
                                        ? "空"
                                        : (c.costAmountNet ?? "—")}
                                </div>
                            </div>
                        ))}
                    </div>

                    <div className="grid gap-4 md:grid-cols-2">
                        <div>
                            <h4 className="mb-2 text-sm font-medium">
                                未归集清单摘要
                            </h4>
                            <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                                {report.unattributedSummaries.map((s) => (
                                    <li key={s}>{s}</li>
                                ))}
                            </ul>
                        </div>
                        <div>
                            <h4 className="mb-2 text-sm font-medium">
                                失败清单摘要
                            </h4>
                            <ul className="list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                                {report.failedSummaries.map((s) => (
                                    <li key={s}>{s}</li>
                                ))}
                            </ul>
                        </div>
                    </div>

                    <p className="text-xs text-muted-foreground">
                        {report.sensitiveRedactionNote}
                    </p>

                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="operations-history-backfill-detail-report-section-download-report"
                            type="button"
                            onClick={onDownload}
                        >
                            <DownloadIcon className="size-4" />
                            下载
                            {unconfirmed ? "技术报告 · 未确认" : "已确认报告"}
                            （示例）
                        </Button>
                        {job.reportReviewStatus === "POLICY_NOT_CONFIGURED" ? (
                            <Badge variant="outline">
                                确认动作不可用 · 复核策略未配置
                            </Badge>
                        ) : null}
                        {job.allowedActions.includes("CONFIRM_REPORT") ? (
                            <Badge variant="outline">
                                报告确认后解锁后续流程
                            </Badge>
                        ) : null}
                    </div>
                </CardContent>
            </Card>
        </div>
    )
}
