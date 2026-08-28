"use client"

import * as React from "react"
import { TriangleAlertIcon } from "lucide-react"

import { PageScaffold } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { toast } from "@/components/ui/toast"
import { HistoryBackfillResultBanner as FormalResultBanner } from "@/features/history-backfill/components/history-backfill-result-banner"
import { JobCommandDialogs } from "@/features/history-backfill/components/job-command-dialogs"
import { JobDetailHeader } from "@/features/history-backfill/components/job-detail-header"
import { JobProgressPanel } from "@/features/history-backfill/components/job-progress-panel"
import {
    CostSection,
    ItemFilters,
    ItemsTable,
    OverviewSection,
    ReportSection,
} from "@/features/history-backfill/components/job-detail-sections"
import {
    useBackfillCommand,
    type BackfillCommandAction,
} from "@/features/history-backfill/hooks/use-backfill-command"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import type {
    HistoryBackfillDetailView,
    HistoryBackfillReportView,
    JobSection,
} from "@/features/history-backfill/types"
import { REPORT_REVIEW_STATUS_LABEL } from "@/features/history-backfill/types"

const SECTION_TABS: { id: JobSection; label: string }[] = [
    { id: "overview", label: "概览" },
    { id: "facts", label: "记录结果" },
    { id: "dedupe", label: "去重" },
    { id: "unattributed", label: "未归集" },
    { id: "cost", label: "成本口径" },
    { id: "failures", label: "失败诊断" },
    { id: "report", label: "审计报告" },
]

export function JobDetailContent({
    view,
    isFetching,
    urlState,
    patchUrl,
    onBack,
    onRefresh,
}: {
    view: HistoryBackfillDetailView
    isFetching: boolean
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    onBack: () => void
    onRefresh: () => void
}) {
    const [startOpen, setStartOpen] = React.useState(false)
    const [resumeOpen, setResumeOpen] = React.useState(false)
    const [reattributeOpen, setReattributeOpen] = React.useState(false)
    const [reattributeItemId, setReattributeItemId] = React.useState<
        string | null
    >(null)
    const [confirmReportOpen, setConfirmReportOpen] = React.useState(false)
    const job = view.job
    const report: HistoryBackfillReportView | undefined = view.report

    const { actionResult, isPending, runCommand } = useBackfillCommand(
        job,
        report,
    )

    const section = urlState.section

    const canStart = job.allowedActions.includes("START")
    const canResume = job.allowedActions.includes("RESUME")
    const canValidate = job.allowedActions.includes("VALIDATE_SOURCE")
    const canConfirmReport = job.allowedActions.includes("CONFIRM_REPORT")
    const startBlockers = job.actionBlockers.filter(
        (b) => b.action === "START",
    )

    const filteredItems = view.items ?? []
    const sectionItems = filteredItems

    const runAndClose = React.useCallback(
        async (action: BackfillCommandAction, itemIds?: string[]) => {
            await runCommand(action, itemIds)
            if (action === "START") setStartOpen(false)
            if (action === "RESUME") setResumeOpen(false)
            if (action === "REATTRIBUTE") setReattributeOpen(false)
            if (action === "CONFIRM_REPORT") setConfirmReportOpen(false)
        },
        [runCommand],
    )

    return (
        <PageScaffold>
            <JobDetailHeader
                job={job}
                report={report}
                isFetching={isFetching}
                pending={isPending}
                canValidate={canValidate}
                canStart={canStart}
                canResume={canResume}
                canConfirmReport={canConfirmReport}
                onBack={onBack}
                onRefresh={onRefresh}
                onValidate={() => void runCommand("VALIDATE_SOURCE")}
                onStart={() => setStartOpen(true)}
                onResume={() => setResumeOpen(true)}
                onConfirmReport={() => setConfirmReportOpen(true)}
                onDownloadReport={() => {
                    if (!report) return
                    toast.add({
                        title: "报告文件生成中",
                        description: `${report.downloadLabel} · v${report.reportVersion}`,
                        type: "info",
                        timeout: 4000,
                    })
                }}
            />

            {job.processingStatus === "COMPLETED" &&
            job.reportReviewStatus !== "CONFIRMED" ? (
                <Alert>
                    <TriangleAlertIcon />
                    <AlertTitle>
                        技术处理完成 ≠ 报告已确认 / 全历史业务完成
                    </AlertTitle>
                    <AlertDescription>
                        技术处理完成仅表示处理完成。当前报告确认状态为「
                        {REPORT_REVIEW_STATUS_LABEL[job.reportReviewStatus]}
                        」。后续流程门禁：
                        {job.formalDownstreamUnlocked ? "已可用" : "保持关闭"}。
                    </AlertDescription>
                </Alert>
            ) : null}

            {!job.coverageComplete ? (
                <Alert variant="destructive">
                    <AlertTitle>全历史覆盖不足 · 阻断执行</AlertTitle>
                    <AlertDescription>
                        必须覆盖起点=
                        {formatDay(job.requiredHistoryStart)}，
                        来源覆盖起点=
                        {job.sourceCoverageStart
                            ? formatDay(job.sourceCoverageStart)
                            : "—"}
                        。不得把较晚时间改成范围起点后宣称全历史完成。
                        <ul className="mt-2 list-disc pl-4">
                            {job.coverageGaps.map((g) => (
                                <li key={`${g.from}-${g.to}`}>
                                    {formatDay(g.from)} → {formatDay(g.to)} ·{" "}
                                    {g.reasonLabel}
                                </li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}

            <FormalResultBanner result={actionResult} />

            <JobProgressPanel job={job} />

            {startBlockers.length > 0 && !canStart ? (
                <Alert>
                    <AlertTitle>处理动作阻断</AlertTitle>
                    <AlertDescription>
                        <ul className="list-disc pl-4">
                            {startBlockers.map((b) => (
                                <li key={b.code}>{b.message}</li>
                            ))}
                        </ul>
                    </AlertDescription>
                </Alert>
            ) : null}

            <Tabs
                value={section}
                onValueChange={(v) => {
                    if (v == null) return
                    patchUrl({ section: v as JobSection, page: 1 })
                }}
            >
                <TabsList className="flex h-auto flex-wrap">
                    {SECTION_TABS.map((t) => (
                        <TabsTrigger key={t.id} value={t.id}>
                            {t.label}
                        </TabsTrigger>
                    ))}
                </TabsList>
            </Tabs>

            {(section === "facts" ||
                section === "dedupe" ||
                section === "unattributed" ||
                section === "failures") && (
                <ItemFilters
                    urlState={urlState}
                    patchUrl={patchUrl}
                    section={section}
                />
            )}

            {section === "overview" ? <OverviewSection job={job} /> : null}

            {section === "facts" ||
            section === "dedupe" ||
            section === "unattributed" ||
            section === "failures" ? (
                <ItemsTable
                    items={sectionItems}
                    section={section}
                    loading={isFetching}
                    totalCount={view.totalItems ?? sectionItems.length}
                    page={Math.max(1, urlState.page)}
                    onPageChange={(nextPage) => patchUrl({ page: nextPage })}
                    onReattribute={(itemId) => {
                        setReattributeItemId(itemId)
                        setReattributeOpen(true)
                    }}
                />
            ) : null}

            {section === "cost" ? (
                <CostSection job={job} items={filteredItems} />
            ) : null}

            {section === "report" ? (
                <ReportSection
                    job={job}
                    report={report}
                    onDownload={() => {
                        if (!report) return
                        toast.add({
                            title: "报告下载信息",
                            description: `${report.downloadLabel} · 结构版本 ${report.schemaVersion} · 规则 ${report.ruleVersion}`,
                            type: "info",
                            timeout: 4000,
                        })
                    }}
                />
            ) : null}

            <JobCommandDialogs
                job={job}
                pending={isPending}
                startOpen={startOpen}
                onStartOpenChange={setStartOpen}
                resumeOpen={resumeOpen}
                onResumeOpenChange={setResumeOpen}
                confirmReportOpen={confirmReportOpen}
                onConfirmReportOpenChange={setConfirmReportOpen}
                reattributeOpen={reattributeOpen}
                onReattributeOpenChange={setReattributeOpen}
                reattributeItemId={reattributeItemId}
                onRun={runAndClose}
            />
        </PageScaffold>
    )
}
