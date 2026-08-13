"use client"

import * as React from "react"
import {
    ArrowLeftIcon,
    DownloadIcon,
    RefreshCwIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BackgroundJobProgress,
    BusinessEmptyState,
    BusinessFailureState,
    CostCoverageNotice,
    DocumentHeader,
    FormalActionConfirmDialog,
    ImportStageIndicator,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { formatHistoryBackfillDay as formatDay } from "@/features/history-backfill/lib/format"
import { HistoryBackfillFact as Fact } from "@/features/history-backfill/components/history-backfill-fact"
import {
    CostSection,
    ItemFilters,
    ItemsTable,
    OverviewSection,
    ReportSection,
} from "@/features/history-backfill/components/job-detail-sections"
import { HistoryBackfillResultBanner as FormalResultBanner } from "@/features/history-backfill/components/history-backfill-result-banner"
import {
    buildStageStates,
    mapJobProgressStatus,
    newRequestId,
} from "@/features/history-backfill/lib/presentation"
import {
    useHistoryBackfillCommandMutation,
    useHistoryBackfillDetailQuery,
} from "@/features/history-backfill/hooks/queries"
import type {
    CostBasis,
    HistoryBackfillCommandResult,
    ItemResult,
    JobSection,
} from "@/features/history-backfill/types"
import {
    ENVIRONMENT_LABEL,
    PIPELINE_STAGE_LABEL,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
} from "@/features/history-backfill/types"
import type { HistoryBackfillUrlState } from "@/features/history-backfill/lib/url-state"
import { formatDateTime } from "@/lib/datetime"

const SECTION_TABS: { id: JobSection; label: string }[] = [
    { id: "overview", label: "概览" },
    { id: "facts", label: "记录结果" },
    { id: "dedupe", label: "去重" },
    { id: "unattributed", label: "未归集" },
    { id: "cost", label: "成本口径" },
    { id: "failures", label: "失败诊断" },
    { id: "report", label: "审计报告" },
]

function JobDetailView({
    jobId,
    urlState,
    patchUrl,
    onBack,
}: {
    jobId: string
    urlState: HistoryBackfillUrlState
    patchUrl: (patch: Partial<HistoryBackfillUrlState>) => void
    onBack: () => void
    onOpenJob: (id: string) => void
}) {
    const [actionResult, setActionResult] =
        React.useState<HistoryBackfillCommandResult | null>(null)
    const [startOpen, setStartOpen] = React.useState(false)
    const [resumeOpen, setResumeOpen] = React.useState(false)
    const [reattributeOpen, setReattributeOpen] = React.useState(false)
    const [reattributeItemId, setReattributeItemId] = React.useState<
        string | null
    >(null)
    const [confirmReportOpen, setConfirmReportOpen] = React.useState(false)
    const [downloadNote, setDownloadNote] = React.useState<string | null>(null)
    const commandMutation = useHistoryBackfillCommandMutation()

    const section = urlState.section
    const results: ItemResult[] | undefined =
        section === "dedupe"
            ? ["DEDUPLICATED"]
            : section === "unattributed"
              ? ["UNATTRIBUTED"]
              : section === "failures"
                ? ["FAILED"]
                : urlState.result
                  ? [urlState.result]
                  : undefined
    const factTypes = urlState.factType ? [urlState.factType] : undefined
    const costBases = urlState.costBasis ? [urlState.costBasis] : undefined

    const detailQuery = useHistoryBackfillDetailQuery({
        jobId,
        results,
        factTypes,
        costBases,
        q: urlState.q,
        page: Math.max(1, urlState.page),
        pageSize: 20,
        section,
    })

    const view = detailQuery.data
    const job = view?.job

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <div className="h-10 w-48 animate-pulse rounded-lg bg-muted" />
                <div className="h-24 animate-pulse rounded-lg bg-muted" />
                <div className="h-40 animate-pulse rounded-lg bg-muted" />
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="任务加载失败"
                    error={detailQuery.error}
                    onRetry={() => void detailQuery.refetch()}
                />
            </PageScaffold>
        )
    }

    if (!job) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="无法打开该任务"
                    description="任务可能已结束或链接失效；也可返回任务列表重新选择。"
                    className="rounded-lg border-0 bg-transparent shadow-none ring-0"
                    action={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={onBack}
                            >
                                返回列表
                            </Button>
                        </div>
                    }
                />
            </PageScaffold>
        )
    }

    // 收窄后固定引用，供 async 闭包安全使用
    const currentJob = job

    const stageStates = buildStageStates(currentJob.pipelineStage)
    const stageLabels = {
        upload: PIPELINE_STAGE_LABEL.SCOPE,
        mapping: PIPELINE_STAGE_LABEL.VALIDATE_SOURCE,
        validation: PIPELINE_STAGE_LABEL.INGEST,
        preview: PIPELINE_STAGE_LABEL.ATTRIBUTE,
        submission: PIPELINE_STAGE_LABEL.REPORT,
        result: PIPELINE_STAGE_LABEL.DONE,
    }
    const progressStatus = mapJobProgressStatus(currentJob.processingStatus)
    const noneRow = currentJob.costBasis.find((c) => c.basis === "NONE")
    const canStart = currentJob.allowedActions.includes("START")
    const canResume = currentJob.allowedActions.includes("RESUME")
    const canValidate = currentJob.allowedActions.includes("VALIDATE_SOURCE")
    const canConfirmReport =
        currentJob.allowedActions.includes("CONFIRM_REPORT")
    const startBlockers = currentJob.actionBlockers.filter(
        (b) => b.action === "START",
    )
    const report = view?.report

    const primaryProcessing = {
        label: PROCESSING_STATUS_LABEL[currentJob.processingStatus],
        tone: PROCESSING_STATUS_TONE[currentJob.processingStatus],
    }

    const filteredItems = view?.items ?? []
    const sectionItems = filteredItems

    const dominantBasis: CostBasis =
        ([...currentJob.costBasis].sort((a, b) => b.count - a.count)[0]
            ?.basis as CostBasis) ?? "NONE"

    const coverageState =
        currentJob.coveragePercent >= 99
            ? "complete"
            : currentJob.coveragePercent <= 0
              ? "none"
              : "partial"

    async function runCommand(
        action:
            | "VALIDATE_SOURCE"
            | "START"
            | "RESUME"
            | "REATTRIBUTE"
            | "CONFIRM_REPORT",
        itemIds?: string[],
    ) {
        const operationId = newRequestId("op")
        const idempotencyKey =
            action === "RESUME"
                ? `${currentJob.idempotencyNamespace}:resume:${currentJob.lockVersion}`
                : newRequestId(`idem_${action.toLowerCase()}`)
        const result = await commandMutation.mutateAsync({
            action,
            jobId: currentJob.id,
            expectedLockVersion: currentJob.lockVersion,
            rangeStart: currentJob.rangeStart,
            rangeEnd: currentJob.rangeEnd,
            operationId,
            idempotencyKey,
            itemIds,
            reportVersion: report?.reportVersion,
        })
        setActionResult(result)
        if (action === "START") setStartOpen(false)
        if (action === "RESUME") setResumeOpen(false)
        if (action === "REATTRIBUTE") setReattributeOpen(false)
        if (action === "CONFIRM_REPORT") setConfirmReportOpen(false)
    }

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    {
                        id: "gov",
                        label: "治理",
                        href: "/governance/history-backfill",
                    },
                    {
                        id: "hb",
                        label: "历史消费回填",
                        href: "/governance/history-backfill",
                    },
                    {
                        id: "job",
                        label: currentJob.jobNo,
                        current: true,
                    },
                ]}
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={onBack}
                        >
                            <ArrowLeftIcon className="size-4" />
                            返回任务列表
                        </Button>
                        <Button
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="text-muted-foreground"
                            disabled={detailQuery.isFetching}
                            onClick={() => void detailQuery.refetch()}
                        >
                            <RefreshCwIcon
                                className={
                                    detailQuery.isFetching ? "animate-spin" : ""
                                }
                                aria-hidden
                            />
                            刷新
                        </Button>
                    </div>
                }
            />

            <DocumentHeader
                density="compact"
                title={currentJob.mallName}
                documentNumber={currentJob.jobNo}
                primaryStatus={primaryProcessing}
                version={`版本 ${currentJob.lockVersion}`}
                meta={
                    <span className="text-muted-foreground">
                        {ENVIRONMENT_LABEL[currentJob.environment]} · 范围起点{" "}
                        {formatDay(currentJob.rangeStart)} 至 截止时点{" "}
                        {formatDay(currentJob.rangeEnd)}
                    </span>
                }
                statuses={[
                    {
                        id: "report",
                        label: "报告确认",
                        status: {
                            label: REPORT_REVIEW_STATUS_LABEL[
                                currentJob.reportReviewStatus
                            ],
                            tone: REPORT_REVIEW_STATUS_TONE[
                                currentJob.reportReviewStatus
                            ],
                        },
                    },
                    {
                        id: "mall",
                        label: "商城",
                        status: {
                            label: `${currentJob.mallName} · ${ENVIRONMENT_LABEL[currentJob.environment]}`,
                            tone:
                                currentJob.environment === "production"
                                    ? "destructive"
                                    : "info",
                        },
                    },
                    {
                        id: "downstream",
                        label: "后续流程",
                        status: {
                            label: currentJob.formalDownstreamUnlocked
                                ? "已可用"
                                : "保持关闭",
                            tone: currentJob.formalDownstreamUnlocked
                                ? "success"
                                : "warning",
                        },
                    },
                ]}
                secondaryActions={
                    <>
                        {canValidate ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={commandMutation.isPending}
                                onClick={() =>
                                    void runCommand("VALIDATE_SOURCE")
                                }
                            >
                                校验来源
                            </Button>
                        ) : null}
                        {canStart ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => setStartOpen(true)}
                            >
                                开始回填
                            </Button>
                        ) : null}
                        {canResume ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => setResumeOpen(true)}
                            >
                                续跑原任务
                            </Button>
                        ) : null}
                        {canConfirmReport ? (
                            <Button
                                type="button"
                                size="sm"
                                onClick={() => setConfirmReportOpen(true)}
                            >
                                确认报告
                            </Button>
                        ) : null}
                        {report ? (
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={() => {
                                    setDownloadNote(
                                        `示例：报告文件生成中 · ${report.downloadLabel} · v${report.reportVersion}`,
                                    )
                                }}
                            >
                                <DownloadIcon className="size-4" />
                                下载报告（示例）
                            </Button>
                        ) : null}
                    </>
                }
            />

            {currentJob.processingStatus === "COMPLETED" &&
            currentJob.reportReviewStatus !== "CONFIRMED" ? (
                <Alert>
                    <TriangleAlertIcon />
                    <AlertTitle>
                        技术处理完成 ≠ 报告已确认 / 全历史业务完成
                    </AlertTitle>
                    <AlertDescription>
                        技术处理完成仅表示处理完成。当前报告确认状态为「
                        {
                            REPORT_REVIEW_STATUS_LABEL[
                                currentJob.reportReviewStatus
                            ]
                        }
                        」。后续流程门禁：
                        {currentJob.formalDownstreamUnlocked
                            ? "已可用"
                            : "保持关闭"}
                        。
                    </AlertDescription>
                </Alert>
            ) : null}

            {!currentJob.coverageComplete ? (
                <Alert variant="destructive">
                    <AlertTitle>全历史覆盖不足 · 阻断执行</AlertTitle>
                    <AlertDescription>
                        必须覆盖起点=
                        {formatDay(currentJob.requiredHistoryStart)}，
                        来源覆盖起点=
                        {currentJob.sourceCoverageStart
                            ? formatDay(currentJob.sourceCoverageStart)
                            : "—"}
                        。不得把较晚时间改成范围起点后宣称全历史完成。
                        <ul className="mt-2 list-disc pl-4">
                            {currentJob.coverageGaps.map((g) => (
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

            {downloadNote ? (
                <Alert>
                    <DownloadIcon />
                    <AlertTitle>下载结果</AlertTitle>
                    <AlertDescription>{downloadNote}</AlertDescription>
                </Alert>
            ) : null}

            <ImportStageIndicator
                stages={stageStates}
                stageLabels={stageLabels}
                aria-label="回填处理阶段"
            />

            <BackgroundJobProgress
                mode="partialAllowed"
                status={progressStatus}
                total={currentJob.progress.totalCount}
                completed={currentJob.progress.processedCount}
                succeeded={currentJob.progress.insertedCount}
                skipped={currentJob.progress.deduplicatedCount}
                failed={currentJob.progress.failedCount}
                label={`后台回填进度 · ${currentJob.jobNo}`}
                description={
                    <>
                        后台执行，不伪装同步完成。已处理{" "}
                        {currentJob.progress.processedCount.toLocaleString(
                            "zh-CN",
                        )}{" "}
                        · 新增{" "}
                        {currentJob.progress.insertedCount.toLocaleString(
                            "zh-CN",
                        )}{" "}
                        · 去重{" "}
                        {currentJob.progress.deduplicatedCount.toLocaleString(
                            "zh-CN",
                        )}{" "}
                        · 待归集{" "}
                        {currentJob.progress.unattributedCount.toLocaleString(
                            "zh-CN",
                        )}{" "}
                        · 失败{" "}
                        {currentJob.progress.failedCount.toLocaleString(
                            "zh-CN",
                        )}
                        {currentJob.progress.heartbeatAt
                            ? ` · 最近更新于 ${formatDateTime(currentJob.progress.heartbeatAt, "dateStyle")}`
                            : ""}
                    </>
                }
            />

            <CostCoverageNotice
                basis={dominantBasis}
                coveragePercent={currentJob.coveragePercent}
                coverageLabel={currentJob.coverageRate ?? "—"}
                coverageState={coverageState}
                breakdown={{
                    ACTUAL:
                        currentJob.costBasis.find((c) => c.basis === "ACTUAL")
                            ?.consumptionAmountGross ?? "—",
                    STANDARD:
                        currentJob.costBasis.find((c) => c.basis === "STANDARD")
                            ?.consumptionAmountGross ?? "—",
                    NONE:
                        noneRow && noneRow.count > 0
                            ? `${noneRow.consumptionAmountGross} · 成本空（非 0）`
                            : "—",
                }}
                profitBasis="回填成本按逐笔记录：商城成本记录 → 消费时点供给版本 → 未覆盖；禁止使用当前供给价"
                notice={
                    <span>
                        未覆盖时成本字段为空而非
                        0，仅进入消费金额与覆盖率分母。时点标准成本
                        必须命中消费发生时点版本。
                    </span>
                }
            />

            <div
                className={`${surfacePanelClassName} grid gap-3 p-4 sm:grid-cols-2 lg:grid-cols-4`}
            >
                <Fact label="发起人" value={currentJob.requestedBy} />
                <Fact
                    label="发起时间"
                    value={formatDateTime(currentJob.requestedAt, "dateStyle")}
                />
                <Fact
                    label="来源更新时间"
                    value={formatDateTime(currentJob.sourceAsOf, "dateStyle")}
                />
                <Fact label="范围说明" value={currentJob.scopeNote} />
                <Fact label="履约说明" value={currentJob.legacyManualNote} />
            </div>

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

            {section === "overview" ? (
                <OverviewSection job={currentJob} />
            ) : null}

            {section === "facts" ||
            section === "dedupe" ||
            section === "unattributed" ||
            section === "failures" ? (
                <ItemsTable
                    items={sectionItems}
                    section={section}
                    loading={detailQuery.isFetching}
                    totalCount={view?.totalItems ?? sectionItems.length}
                    page={Math.max(1, urlState.page)}
                    onPageChange={(nextPage) => patchUrl({ page: nextPage })}
                    onReattribute={(itemId) => {
                        setReattributeItemId(itemId)
                        setReattributeOpen(true)
                    }}
                />
            ) : null}

            {section === "cost" ? (
                <CostSection job={currentJob} items={filteredItems} />
            ) : null}

            {section === "report" ? (
                <ReportSection
                    job={currentJob}
                    report={report}
                    onDownload={() => {
                        if (!report) return
                        setDownloadNote(
                            `${report.downloadLabel} · Schema ${report.schemaVersion} · 规则 ${report.ruleVersion}`,
                        )
                    }}
                />
            ) : null}

            <FormalActionConfirmDialog
                open={startOpen}
                onOpenChange={setStartOpen}
                actionLabel="开始回填"
                title="确认开始历史回填"
                description="将锁定回填范围并创建后台任务，只补充缺失记录；回填起点前的支付不计入；范围创建后不可修改。"
                fromStatus={{
                    label: PROCESSING_STATUS_LABEL[currentJob.processingStatus],
                    tone: PROCESSING_STATUS_TONE[currentJob.processingStatus],
                }}
                toStatus={{ label: "运行中", tone: "info" }}
                lockedFields={[
                    `范围起点 = 必须覆盖起点 = ${formatDay(currentJob.requiredHistoryStart)}`,
                    `截止时点 = ${formatDay(currentJob.rangeEnd)}`,
                    `商城 ${currentJob.mallName} · ${ENVIRONMENT_LABEL[currentJob.environment]}`,
                ]}
                effects={[
                    "后台执行五类关键记录回填",
                    "与实时记录按业务记录键去重",
                    "成本按实际、时点标准、未覆盖三种口径评估",
                ]}
                irreversibleEffects={[
                    "已成功写入的业务记录不因失败或续跑回滚",
                    "范围冻结后不可修改",
                ]}
                pending={commandMutation.isPending}
                onConfirm={() => runCommand("START")}
            />

            <FormalActionConfirmDialog
                open={resumeOpen}
                onOpenChange={setResumeOpen}
                actionLabel="续跑原任务"
                title="确认续跑失败/中断任务"
                description="沿原任务、原范围与原任务标识续跑，不新建重叠业务批次。"
                fromStatus={{
                    label: PROCESSING_STATUS_LABEL[currentJob.processingStatus],
                    tone: PROCESSING_STATUS_TONE[currentJob.processingStatus],
                }}
                toStatus={{ label: "运行中", tone: "info" }}
                lockedFields={[
                    `任务 ${currentJob.jobNo}`,
                    `范围起点 ${formatDay(currentJob.rangeStart)} 至 截止时点 ${formatDay(currentJob.rangeEnd)}`,
                    "沿用原任务提交记录",
                    `已成功 ${currentJob.progress.insertedCount} · 待处理剩余项`,
                ]}
                effects={["逐项仍使用相同业务记录键", "已成功记录保持不变"]}
                irreversibleEffects={["不删除已入库记录"]}
                pending={commandMutation.isPending}
                onConfirm={() => runCommand("RESUME")}
            />

            <FormalActionConfirmDialog
                open={confirmReportOpen}
                onOpenChange={setConfirmReportOpen}
                actionLabel="确认报告"
                title="确认技术报告并解锁后续流程"
                description="仅更新报告确认状态；不改写已入库记录或处理状态。"
                fromStatus={{
                    label: REPORT_REVIEW_STATUS_LABEL[
                        currentJob.reportReviewStatus
                    ],
                    tone: REPORT_REVIEW_STATUS_TONE[
                        currentJob.reportReviewStatus
                    ],
                }}
                toStatus={{ label: "已确认", tone: "success" }}
                effects={[
                    "技术报告标记为已确认",
                    "覆盖完整时解锁后续流程",
                    "不改写已入库记录",
                ]}
                irreversibleEffects={["报告确认状态进入处理审计"]}
                pending={commandMutation.isPending}
                onConfirm={() => runCommand("CONFIRM_REPORT")}
            />

            <FormalActionConfirmDialog
                open={reattributeOpen}
                onOpenChange={setReattributeOpen}
                actionLabel="重新归集"
                title="确认逐项重新归集"
                description="引用原业务记录重新归集并追加成本评估；不复制业务记录、不改写原消费。"
                fromStatus={{ label: "待归集", tone: "warning" }}
                toStatus={{ label: "已提交重新归集", tone: "success" }}
                effects={["按原业务记录键重新归集", "追加成本评估"]}
                irreversibleEffects={["归集结果进入处理审计"]}
                pending={commandMutation.isPending}
                onConfirm={() =>
                    runCommand(
                        "REATTRIBUTE",
                        reattributeItemId ? [reattributeItemId] : undefined,
                    )
                }
            />
        </PageScaffold>
    )
}

export { JobDetailView }
