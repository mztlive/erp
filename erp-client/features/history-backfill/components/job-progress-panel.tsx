"use client"

import {
    BackgroundJobProgress,
    CostCoverageNotice,
    ImportStageIndicator,
    surfacePanelClassName,
} from "@/components/business"
import { HistoryBackfillFact as Fact } from "@/features/history-backfill/components/history-backfill-fact"
import {
    buildStageStates,
    mapJobProgressStatus,
} from "@/features/history-backfill/lib/presentation"
import type {
    CostBasis,
    HistoryBackfillJobCore,
} from "@/features/history-backfill/types"
import { PIPELINE_STAGE_LABEL } from "@/features/history-backfill/types"
import { formatDateTime } from "@/lib/datetime"

const STAGE_LABELS = {
    upload: PIPELINE_STAGE_LABEL.SCOPE,
    mapping: PIPELINE_STAGE_LABEL.VALIDATE_SOURCE,
    validation: PIPELINE_STAGE_LABEL.INGEST,
    preview: PIPELINE_STAGE_LABEL.ATTRIBUTE,
    submission: PIPELINE_STAGE_LABEL.REPORT,
    result: PIPELINE_STAGE_LABEL.DONE,
} as const

export function JobProgressPanel({ job }: { job: HistoryBackfillJobCore }) {
    const stageStates = buildStageStates(job.pipelineStage)
    const progressStatus = mapJobProgressStatus(job.processingStatus)
    const noneRow = job.costBasis.find((c) => c.basis === "NONE")

    const dominantBasis: CostBasis =
        ([...job.costBasis].sort((a, b) => b.count - a.count)[0]
            ?.basis as CostBasis) ?? "NONE"

    const coverageState =
        job.coveragePercent >= 99
            ? "complete"
            : job.coveragePercent <= 0
              ? "none"
              : "partial"

    return (
        <>
            <ImportStageIndicator
                stages={stageStates}
                stageLabels={STAGE_LABELS}
                aria-label="回填处理阶段"
            />

            <BackgroundJobProgress
                mode="partialAllowed"
                status={progressStatus}
                total={job.progress.totalCount}
                completed={job.progress.processedCount}
                succeeded={job.progress.insertedCount}
                skipped={job.progress.deduplicatedCount}
                failed={job.progress.failedCount}
                label={`后台回填进度 · ${job.jobNo}`}
                description={
                    <>
                        后台执行，不伪装同步完成。已处理{" "}
                        {job.progress.processedCount.toLocaleString("zh-CN")} ·
                        新增{" "}
                        {job.progress.insertedCount.toLocaleString("zh-CN")} ·
                        去重{" "}
                        {job.progress.deduplicatedCount.toLocaleString("zh-CN")}{" "}
                        · 待归集{" "}
                        {job.progress.unattributedCount.toLocaleString("zh-CN")}{" "}
                        · 失败{" "}
                        {job.progress.failedCount.toLocaleString("zh-CN")}
                        {job.progress.heartbeatAt
                            ? ` · 最近更新于 ${formatDateTime(job.progress.heartbeatAt, "dateStyle")}`
                            : ""}
                    </>
                }
            />

            <CostCoverageNotice
                basis={dominantBasis}
                coveragePercent={job.coveragePercent}
                coverageLabel={job.coverageRate ?? "—"}
                coverageState={coverageState}
                breakdown={{
                    ACTUAL:
                        job.costBasis.find((c) => c.basis === "ACTUAL")
                            ?.consumptionAmountGross ?? "—",
                    STANDARD:
                        job.costBasis.find((c) => c.basis === "STANDARD")
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
                <Fact label="发起人" value={job.requestedBy} />
                <Fact
                    label="发起时间"
                    value={formatDateTime(job.requestedAt, "dateStyle")}
                />
                <Fact
                    label="来源更新时间"
                    value={formatDateTime(job.sourceAsOf, "dateStyle")}
                />
                <Fact label="范围说明" value={job.scopeNote} />
                <Fact label="履约说明" value={job.legacyManualNote} />
            </div>
        </>
    )
}
