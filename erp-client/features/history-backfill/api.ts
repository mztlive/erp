/**
 * W30 历史消费回填 · 真实 HTTP API
 * 路径：/admin/mall-consumption-backfill-jobs、/admin/mall-consumption-backfill-items
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
    CreateBackfillContext,
    HistoryBackfillCommandInput,
    HistoryBackfillCommandResult,
    HistoryBackfillDetailQuery,
    HistoryBackfillDetailView,
    HistoryBackfillItemView,
    HistoryBackfillJobCore,
    HistoryBackfillListItem,
    HistoryBackfillListQuery,
    HistoryBackfillListView,
    HistoryBackfillProcessingStatus,
    HistoryBackfillReportReviewStatus,
    ItemResult,
    CostBasis,
} from "@/features/history-backfill/types"
import { PROCESSING_STATUS_LABEL } from "@/features/history-backfill/types"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

type BackendJob = {
    id: string
    mall_id: string
    cutover_id: string
    range_start: number
    range_end: number
    status: string
    total_count: number
    total_amount: string
    deduplicated_count: number
    actual_count: number
    standard_count: number
    none_count: number
    unattributed_count: number
    report_file_id?: string | null
    version: number
    created_at: number
}

type BackendJobDetail = {
    job: BackendJob
    item_total_count: number
}

type BackendItem = {
    id: string
    job_id: string
    business_fact_key: string
    source_event_reference: string
    mall_order_fact_id?: string | null
    result: string
    cost_basis: string
    error_code?: string | null
    error_detail?: string | null
    created_at: number
}

type BackendCommandResult = {
    status: string
    job_id: string
    job_no: string
    operation_id: string
    idempotency_key: string
    next_step: string
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
        return ""
    return new Date(Number(secs) * 1000).toISOString()
}

function dateToUnix(value?: string): number | undefined {
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

function mapProcessingStatus(raw: string): HistoryBackfillProcessingStatus {
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

function processingToBackend(
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

function mapItemResult(raw: string): ItemResult {
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

function itemResultToBackend(r: ItemResult): string {
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

function mapCostBasis(raw: string): CostBasis {
    const u = raw.toUpperCase()
    if (u === "ACTUAL") return "ACTUAL"
    if (u === "STANDARD") return "STANDARD"
    return "NONE"
}

function reportReviewOf(
    status: HistoryBackfillProcessingStatus,
): HistoryBackfillReportReviewStatus {
    if (status === "COMPLETED") return "PENDING"
    return "NOT_READY"
}

function allowedActionsOf(
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

function mapJobCore(job: BackendJob): HistoryBackfillJobCore {
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

function toListItem(job: HistoryBackfillJobCore): HistoryBackfillListItem {
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

function mapItem(it: BackendItem): HistoryBackfillItemView {
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

function emptyCreateContext(): CreateBackfillContext {
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
        blockReasons: ["创建上下文接口未交付（backend_gap）"],
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function fetchHistoryBackfillList(
    query: HistoryBackfillListQuery,
): Promise<HistoryBackfillListView> {
    const queriedAt = new Date().toISOString()

    const pageRes = await apiGet<Page<BackendJob>>(
        "/admin/mall-consumption-backfill-jobs",
        {
            page: query.page,
            page_size: query.pageSize,
            mall_id: query.mallId,
            status: processingToBackend(query.processingStatus),
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )

    let jobs = (pageRes.items ?? []).map(mapJobCore)

    if (query.view === "active") {
        jobs = jobs.filter((j) =>
            [
                "DRAFT",
                "VALIDATING",
                "READY",
                "RUNNING",
                "PARTIAL",
                "FAILED",
            ].includes(j.processingStatus),
        )
    } else if (query.view === "processing_completed") {
        jobs = jobs.filter((j) => j.processingStatus === "COMPLETED")
    } else if (query.view === "report_pending") {
        jobs = jobs.filter(
            (j) =>
                j.reportReviewStatus === "PENDING" ||
                j.reportReviewStatus === "POLICY_NOT_CONFIGURED",
        )
    }

    if (query.environment) {
        jobs = jobs.filter((j) => j.environment === query.environment)
    }
    if (query.q?.trim()) {
        const q = query.q.trim().toLowerCase()
        jobs = jobs.filter(
            (j) =>
                j.jobNo.toLowerCase().includes(q) ||
                j.mallName.toLowerCase().includes(q) ||
                j.id.toLowerCase().includes(q),
        )
    }
    if (query.basis) {
        const basis = query.basis
        jobs = jobs.filter((j) =>
            j.costBasis.some((c) => c.basis === basis && c.count > 0),
        )
    }

    return {
        metrics: {
            running: jobs.filter((j) => j.processingStatus === "RUNNING")
                .length,
            unattributed: 0,
            deduplicated: 0,
            noneConsumption: 0,
            failed: 0,
        },
        rows: jobs.map(toListItem),
        totalCount: pageRes.total ?? jobs.length,
        queriedAt,
        createContext: emptyCreateContext(),
    }
}

export async function fetchHistoryBackfillDetail(
    query: HistoryBackfillDetailQuery,
): Promise<HistoryBackfillDetailView | null> {
    try {
        const detail = await apiGet<BackendJobDetail>(
            `/admin/mall-consumption-backfill-jobs/${encodeURIComponent(query.jobId)}`,
        )
        const job = mapJobCore(detail.job)

        const itemsPage = await apiGet<Page<BackendItem>>(
            "/admin/mall-consumption-backfill-items",
            {
                page: query.page,
                page_size: query.pageSize,
                job_id: query.jobId,
                result: query.results?.[0]
                    ? itemResultToBackend(query.results[0])
                    : undefined,
                cost_basis: query.costBases?.[0],
                sort_by: "created_at",
                sort_dir: "desc",
            },
        )

        let items = (itemsPage.items ?? []).map(mapItem)
        if (query.q?.trim()) {
            const q = query.q.trim().toLowerCase()
            items = items.filter(
                (i) =>
                    i.mallOrderNo.toLowerCase().includes(q) ||
                    i.businessFactKeySummary.toLowerCase().includes(q) ||
                    (i.sourceDocNo?.toLowerCase().includes(q) ?? false),
            )
        }

        return {
            job,
            items,
            totalItems:
                itemsPage.total ?? detail.item_total_count ?? items.length,
            report: detail.job.report_file_id
                ? {
                      reportId: detail.job.report_file_id,
                      reportVersion: 1,
                      generatedAt: tsToIso(detail.job.created_at),
                      reviewLabel:
                          job.reportReviewStatus === "CONFIRMED"
                              ? "CONFIRMED"
                              : "UNCONFIRMED",
                      downloadLabel: `回填报告_${job.jobNo}`,
                      schemaVersion: "1",
                      ruleVersion: "1",
                      rangeStart: job.rangeStart,
                      rangeEnd: job.rangeEnd,
                      cutoverAt: job.cutoverAt,
                      totalCount: job.progress.totalCount,
                      totalAmount: detail.job.total_amount,
                      insertedCount: job.progress.insertedCount,
                      deduplicatedCount: job.progress.deduplicatedCount,
                      unattributedCount: job.progress.unattributedCount,
                      failedCount: job.progress.failedCount,
                      costBasis: job.costBasis,
                      coverageRate: job.coverageRate,
                      unattributedSummaries: [],
                      failedSummaries: [],
                      operatorLabel: job.requestedBy,
                      processingStatus: job.processingStatus,
                      reportReviewStatus: job.reportReviewStatus,
                      fullHistoryFinalComplete: job.coverageComplete,
                      sensitiveRedactionNote:
                          "报告已脱敏：不含卡号/卡密/手机/完整地址/原始报文。",
                  }
                : undefined,
            queriedAt: new Date().toISOString(),
            permissionVersion: "server",
        }
    } catch (err) {
        const status =
            err && typeof err === "object" && "status" in err
                ? (err as { status?: number }).status
                : undefined
        if (status === 404) return null
        throw err
    }
}

export async function submitHistoryBackfillCommand(
    input: HistoryBackfillCommandInput,
): Promise<HistoryBackfillCommandResult> {
    const { action, operationId, idempotencyKey } = input

    if (action === "CREATE_DRAFT") {
        if (!input.cutoverId || !input.rangeStart || !input.rangeEnd) {
            return {
                status: "BLOCKED",
                title: "缺少创建参数",
                description:
                    "创建回填任务需要 cutoverId、rangeStart、rangeEnd；创建上下文接口未交付时无法自动填参。",
                operationId,
                idempotencyKey,
                blockers: ["CREATE_CONTEXT_MISSING"],
            }
        }
        const rangeStart = dateToUnix(input.rangeStart)
        const rangeEnd = dateToUnix(input.rangeEnd)
        if (!rangeStart || !rangeEnd) {
            return {
                status: "BLOCKED",
                title: "范围非法",
                description: "范围起点/终点无法解析为时间戳。",
                operationId,
                idempotencyKey,
                blockers: ["RANGE_INVALID"],
            }
        }

        const created = await apiPost<BackendJob>(
            "/admin/mall-consumption-backfill-jobs",
            {
                mall_id: "default",
                cutover_id: input.cutoverId,
                range_start: rangeStart,
                range_end: rangeEnd,
                total_count: 1,
                total_amount: "0.00",
            },
        )

        return {
            status: "COMMITTED",
            title: "已创建回填任务草稿",
            description: `任务 ${created.id} 已创建。`,
            jobId: created.id,
            jobNo: created.id,
            operationId,
            idempotencyKey,
            nextStep: "完成来源校验后开始回填",
        }
    }

    if (action === "VALIDATE_SOURCE") {
        // Backend has no validate endpoint — treat as client-side pass marker via detail refresh
        if (!input.jobId) {
            return {
                status: "FAILED",
                title: "缺少任务 ID",
                description: "校验来源必须引用既有任务。",
                operationId,
                idempotencyKey,
            }
        }
        return {
            status: "BLOCKED",
            title: "来源校验接口未交付",
            description: "后端未提供独立 VALIDATE_SOURCE 命令；可直接 START。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["VALIDATE_SOURCE_NOT_IMPLEMENTED"],
        }
    }

    if (action === "START" || action === "RESUME") {
        if (!input.jobId) {
            return {
                status: "FAILED",
                title: "缺少任务 ID",
                description: `${action} 必须引用既有任务。`,
                operationId,
                idempotencyKey,
            }
        }
        const version = input.expectedLockVersion ?? 1
        const result = await apiPost<BackendCommandResult>(
            `/admin/mall-consumption-backfill-jobs/${encodeURIComponent(input.jobId)}/commands`,
            {
                command: action === "START" ? "START" : "RESUME",
                version,
                operation_id: operationId,
                idempotency_key: idempotencyKey,
            },
        )
        return {
            status: "COMMITTED",
            title: action === "START" ? "回填已提交后台" : "已续跑原任务",
            description: result.next_step || "进度以任务记录为准。",
            jobId: result.job_id,
            jobNo: result.job_no,
            operationId: result.operation_id,
            idempotencyKey: result.idempotency_key,
            nextStep: result.next_step,
        }
    }

    if (action === "REATTRIBUTE") {
        return {
            status: "BLOCKED",
            title: "重新归集未交付",
            description: "后端尚未提供回填明细重新归集命令。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["REATTRIBUTE_NOT_IMPLEMENTED"],
        }
    }

    if (action === "CONFIRM_REPORT") {
        return {
            status: "BLOCKED",
            title: "报告确认未交付",
            description:
                "后端 BackfillJobView 仅有 report_file_id，无报告确认状态机接口。",
            jobId: input.jobId,
            operationId,
            idempotencyKey,
            blockers: ["CONFIRM_REPORT_NOT_IMPLEMENTED"],
        }
    }

    return {
        status: "FAILED",
        title: "未知动作",
        description: `不支持的 action: ${action}`,
        operationId,
        idempotencyKey,
    }
}
