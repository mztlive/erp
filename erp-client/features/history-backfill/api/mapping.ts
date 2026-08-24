/**
 * W30 历史消费回填 · 后端字段 → 客户端契约映射
 */

import { PROCESSING_STATUS_LABEL } from "@/features/history-backfill/types"
import type {
    CostBasis,
    CreateBackfillContext,
    HistoryBackfillItemView,
    HistoryBackfillJobCore,
    HistoryBackfillListItem,
    HistoryBackfillProcessingStatus,
    HistoryBackfillReportReviewStatus,
    ItemResult,
} from "@/features/history-backfill/types"
import type { BackendItem, BackendJob } from "@/features/history-backfill/api/wire"

export function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

export function dateToUnix(value?: string): number | undefined {
    if (!value) return undefined
    if (/^\d{4}-\d{2}-\d{2}$/.test(value)) {
        return Math.floor(new Date(`${value}T00:00:00+08:00`).getTime() / 1000)
    }
    const t = Math.floor(new Date(value).getTime() / 1000)
    return Number.isFinite(t) ? t : undefined
}

function formatRange(startIso: string, endIso: string) {
    const s = startIso.slice(0, 10) || "—"
    const e = endIso.slice(0, 10) || "—"
    return `${s} 至 ${e}`
}

export function mapProcessingStatus(
    raw: string,
): HistoryBackfillProcessingStatus {
    switch (raw) {
        case "pending":
            return "READY"
        case "running":
            return "RUNNING"
        case "partially_completed":
            return "PARTIAL"
        case "completed":
            return "COMPLETED"
        case "failed":
            return "FAILED"
        default:
            return "DRAFT"
    }
}

export function processingToBackend(
    status?: HistoryBackfillProcessingStatus,
): string | undefined {
    if (!status) return undefined
    switch (status) {
        case "DRAFT":
        case "VALIDATING":
        case "READY":
            return "pending"
        case "RUNNING":
            return "running"
        case "PARTIAL":
            return "partially_completed"
        case "COMPLETED":
            return "completed"
        case "FAILED":
            return "failed"
        default:
            return undefined
    }
}

export function mapItemResult(raw: string): ItemResult {
    switch (raw) {
        case "new":
            return "INSERTED"
        case "duplicate":
            return "DEDUPLICATED"
        case "pending_attribution":
            return "UNATTRIBUTED"
        case "failed":
            return "FAILED"
        default:
            return "FAILED"
    }
}

export function itemResultToBackend(r: ItemResult): string {
    switch (r) {
        case "INSERTED":
            return "new"
        case "DEDUPLICATED":
            return "duplicate"
        case "UNATTRIBUTED":
            return "pending_attribution"
        case "FAILED":
            return "failed"
    }
}

export function mapCostBasis(raw: string): CostBasis {
    const u = raw.toUpperCase()
    if (u === "ACTUAL") return "ACTUAL"
    if (u === "STANDARD") return "STANDARD"
    return "NONE"
}

export function reportReviewOf(
    status: HistoryBackfillProcessingStatus,
): HistoryBackfillReportReviewStatus {
    if (status === "COMPLETED") return "PENDING"
    return "NOT_READY"
}

export function allowedActionsOf(
    status: HistoryBackfillProcessingStatus,
): HistoryBackfillJobCore["allowedActions"] {
    switch (status) {
        case "DRAFT":
        case "READY":
            return ["VALIDATE_SOURCE", "START"]
        case "PARTIAL":
        case "FAILED":
            return ["RESUME"]
        case "COMPLETED":
            return ["CONFIRM_REPORT"]
        default:
            return []
    }
}

export function mapJobCore(job: BackendJob): HistoryBackfillJobCore {
    const processingStatus = mapProcessingStatus(job.status)
    const rangeStart = tsToIso(job.range_start)
    const rangeEnd = tsToIso(job.range_end)
    // processed_count 未交付：完成态展示 total，其它为 0（backend_gap）
    const processedCount =
        processingStatus === "COMPLETED" ? job.total_count : 0

    return {
        id: job.id,
        jobNo: job.id,
        mallId: job.mall_id,
        mallName: job.mall_id,
        environment: "production",
        cutoverId: job.cutover_id,
        requiredHistoryStart: rangeStart,
        rangeStart,
        rangeEnd,
        cutoverAt: rangeEnd,
        coverageComplete: true,
        coverageGaps: [],
        processingStatus,
        reportReviewStatus: reportReviewOf(processingStatus),
        pipelineStage:
            processingStatus === "COMPLETED"
                ? "DONE"
                : processingStatus === "RUNNING"
                  ? "INGEST"
                  : processingStatus === "READY"
                    ? "VALIDATE_SOURCE"
                    : "SCOPE",
        formalDownstreamUnlocked: false,
        lockVersion: job.version,
        requestedBy: "—",
        requestedAt: tsToIso(job.created_at),
        sourceAsOf: tsToIso(job.created_at),
        fulfillmentNote: "历史记录追加写入，不覆盖实时记录",
        scopeNote: "生效范围从范围起点至截止时点（截止时点当天除外）。",
        legacyManualNote:
            "截止时点前支付只补台账，履约链固定为历史手工口径，不创建供应商订单。",
        progress: {
            totalCount: job.total_count,
            processedCount,
            insertedCount: processedCount,
            deduplicatedCount: job.deduplicated_count,
            unattributedCount: job.unattributed_count,
            failedCount: 0,
            lastProgressAt: tsToIso(job.created_at),
        },
        costBasis: [
            {
                basis: "ACTUAL",
                count: job.actual_count,
                consumptionAmountGross: "—",
                costAmountNet: "—",
            },
            {
                basis: "STANDARD",
                count: job.standard_count,
                consumptionAmountGross: "—",
                costAmountNet: "—",
            },
            {
                basis: "NONE",
                count: job.none_count,
                consumptionAmountGross: "—",
                costAmountNet: null,
            },
        ],
        coverageRate: null,
        coveragePercent: 0,
        allowedActions: allowedActionsOf(processingStatus),
        actionBlockers: [],
        idempotencyNamespace: `mall-backfill:${job.id}`,
    }
}

export function toListItem(
    job: HistoryBackfillJobCore,
): HistoryBackfillListItem {
    const p = job.progress
    const none = job.costBasis.find((c) => c.basis === "NONE")
    const costCoverageLabel =
        job.coverageRate == null
            ? "—"
            : `${job.coverageRate}${none && none.count > 0 ? ` · 未覆盖 ${none.count}` : ""}`

    return {
        id: job.id,
        jobNo: job.jobNo,
        mallId: job.mallId,
        mallName: job.mallName,
        environment: job.environment,
        rangeStart: job.rangeStart,
        rangeEnd: job.rangeEnd,
        rangeLabel: formatRange(job.rangeStart, job.rangeEnd),
        processingStatus: job.processingStatus,
        reportReviewStatus: job.reportReviewStatus,
        progressLabel:
            p.totalCount > 0
                ? `${p.processedCount.toLocaleString("zh-CN")} / ${p.totalCount.toLocaleString("zh-CN")}`
                : PROCESSING_STATUS_LABEL[job.processingStatus],
        processedCount: p.processedCount,
        totalCount: p.totalCount,
        deduplicatedCount: p.deduplicatedCount,
        unattributedCount: p.unattributedCount,
        costCoverageLabel,
        coverageComplete: job.coverageComplete,
        lastProgressAt: p.lastProgressAt,
    }
}

export function mapItem(it: BackendItem): HistoryBackfillItemView {
    const result = mapItemResult(it.result)
    return {
        itemId: it.id,
        jobId: it.job_id,
        factType: "PAYMENT_SUCCEEDED",
        businessFactKeySummary: it.business_fact_key,
        mallOrderNo: it.business_fact_key,
        sourceDocNo: it.source_event_reference,
        occurredAt: tsToIso(it.created_at),
        result,
        costBasis: mapCostBasis(it.cost_basis),
        costAmountNet: null,
        failure:
            result === "FAILED"
                ? {
                      errorCode: it.error_code ?? "FAILED",
                      stage: "INGEST",
                      retryable: false,
                      summary: it.error_detail ?? it.error_code ?? "处理失败",
                  }
                : undefined,
        whitelistFields: [
            {
                field: "business_fact_key",
                label: "业务事实键",
                value: it.business_fact_key,
            },
            {
                field: "source_event_reference",
                label: "来源引用",
                value: it.source_event_reference,
            },
        ],
    }
}

export function emptyCreateContext(): CreateBackfillContext {
    return {
        cutoverId: "",
        mallId: "",
        mallName: "",
        environment: "production",
        requiredHistoryStart: "",
        rangeEnd: "",
        cutoverAt: "",
        sourceCoverageStart: "",
        coverageComplete: false,
        coverageGaps: [],
        estimatedFactCount: 0,
        hasOverlappingFormalJob: false,
        canCreateDraft: false,
        blockReasons: ["当前暂不能创建任务，请联系管理员确认功能是否已启用"],
    }
}
