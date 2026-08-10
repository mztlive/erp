/**
 * W30 历史消费回填 · 客户端契约类型
 * 对齐 docs/ui-workspaces/w30-historical-consumption-backfill.md §5/§8。
 */

import type { ImportStageKey } from "@/components/business"

/** 技术处理状态（≠ 报告确认 / 全业务完成） */
export type HistoryBackfillProcessingStatus =
    | "DRAFT"
    | "VALIDATING"
    | "READY"
    | "RUNNING"
    | "PARTIAL"
    | "COMPLETED"
    | "FAILED"

/** 独立报告确认状态 */
export type HistoryBackfillReportReviewStatus =
    | "NOT_READY"
    | "POLICY_NOT_CONFIGURED"
    | "PENDING"
    | "CONFIRMED"
    | "REJECTED"

export type HistoryBackfillView =
    | "active"
    | "processing_completed"
    | "report_pending"
    | "all"

export type HistoryBackfillEnvironment = "production" | "verification"

export type CostBasis = "ACTUAL" | "STANDARD" | "NONE"

export type ItemResult = "INSERTED" | "DEDUPLICATED" | "UNATTRIBUTED" | "FAILED"

/** 五类关键记录，同一订单下不得合并 */
export type MallOrderFactType =
    | "PAYMENT_SUCCEEDED"
    | "ORDER_CANCELED"
    | "REFUND_SUCCEEDED"
    | "ORDER_COMPLETED"
    | "CARD_BALANCE_RESTORED"

export type BackfillPipelineStage =
    | "SCOPE"
    | "VALIDATE_SOURCE"
    | "INGEST"
    | "ATTRIBUTE"
    | "REPORT"
    | "DONE"

export type JobSection =
    | "overview"
    | "facts"
    | "dedupe"
    | "unattributed"
    | "cost"
    | "failures"
    | "report"

type FormalCommandAction =
    | "CREATE_DRAFT"
    | "VALIDATE_SOURCE"
    | "START"
    | "RESUME"
    | "REATTRIBUTE"
    | "CONFIRM_REPORT"

type FormalCommandResultStatus =
    | "COMMITTED"
    | "BLOCKED"
    | "RESULT_UNKNOWN"
    | "FAILED"

export const PIPELINE_TO_INDICATOR: Record<
    BackfillPipelineStage,
    ImportStageKey
> = {
    SCOPE: "upload",
    VALIDATE_SOURCE: "mapping",
    INGEST: "validation",
    ATTRIBUTE: "preview",
    REPORT: "submission",
    DONE: "result",
}

export const PIPELINE_STAGE_LABEL: Record<BackfillPipelineStage, string> = {
    SCOPE: "范围确认",
    VALIDATE_SOURCE: "来源校验",
    INGEST: "记录入库",
    ATTRIBUTE: "归集评估",
    REPORT: "报告",
    DONE: "完成",
}

export const PIPELINE_ORDER: BackfillPipelineStage[] = [
    "SCOPE",
    "VALIDATE_SOURCE",
    "INGEST",
    "ATTRIBUTE",
    "REPORT",
    "DONE",
]

export const PROCESSING_STATUS_LABEL: Record<
    HistoryBackfillProcessingStatus,
    string
> = {
    DRAFT: "待执行",
    VALIDATING: "校验中",
    READY: "可执行",
    RUNNING: "运行中",
    PARTIAL: "部分完成",
    COMPLETED: "技术处理完成",
    FAILED: "失败",
}

export const PROCESSING_STATUS_TONE: Record<
    HistoryBackfillProcessingStatus,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    DRAFT: "neutral",
    VALIDATING: "info",
    READY: "info",
    RUNNING: "info",
    PARTIAL: "warning",
    COMPLETED: "success",
    FAILED: "destructive",
}

export const REPORT_REVIEW_STATUS_LABEL: Record<
    HistoryBackfillReportReviewStatus,
    string
> = {
    NOT_READY: "未就绪",
    POLICY_NOT_CONFIGURED: "策略未配置",
    PENDING: "待确认",
    CONFIRMED: "已确认",
    REJECTED: "已驳回",
}

export const REPORT_REVIEW_STATUS_TONE: Record<
    HistoryBackfillReportReviewStatus,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    NOT_READY: "neutral",
    POLICY_NOT_CONFIGURED: "warning",
    PENDING: "warning",
    CONFIRMED: "success",
    REJECTED: "destructive",
}

export const FACT_TYPE_LABEL: Record<MallOrderFactType, string> = {
    PAYMENT_SUCCEEDED: "支付成功",
    ORDER_CANCELED: "订单取消",
    REFUND_SUCCEEDED: "退款成功",
    ORDER_COMPLETED: "订单完成",
    CARD_BALANCE_RESTORED: "余额恢复",
}

export const ITEM_RESULT_LABEL: Record<ItemResult, string> = {
    INSERTED: "新增业务记录",
    DEDUPLICATED: "重叠去重",
    UNATTRIBUTED: "待归集",
    FAILED: "处理失败",
}

export const ITEM_RESULT_TONE: Record<
    ItemResult,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    INSERTED: "success",
    DEDUPLICATED: "info",
    UNATTRIBUTED: "warning",
    FAILED: "destructive",
}

export const COST_BASIS_LABEL: Record<CostBasis, string> = {
    ACTUAL: "实际成本",
    STANDARD: "时点标准成本",
    NONE: "未覆盖",
}

/** 失败明细错误码中文映射；未命中不展示原码。 */
export const FAILURE_CODE_LABEL: Record<string, string> = {
    SOURCE_SCHEMA_FIELD_MISSING: "来源字段缺失",
    TAX_BASIS_UNRESOLVED: "税口径无法解析",
}

export const ENVIRONMENT_LABEL: Record<HistoryBackfillEnvironment, string> = {
    production: "生产环境",
    verification: "验证环境",
}

export const VIEW_LABEL: Record<HistoryBackfillView, string> = {
    active: "活跃任务",
    processing_completed: "技术处理完成",
    report_pending: "报告待确认",
    all: "全部",
}

type CoverageGap = {
    from: string
    to: string
    reasonCode: string
    reasonLabel: string
}

type CostBasisRow = {
    basis: CostBasis
    count: number
    /** 消费金额（含税）文案；NONE 成本金额为空而非 0 */
    consumptionAmountGross: string
    /** 成本净额；NONE 必须为 null（展示为空） */
    costAmountNet: string | null
}

type HistoryBackfillProgress = {
    totalCount: number
    processedCount: number
    insertedCount: number
    deduplicatedCount: number
    unattributedCount: number
    failedCount: number
    lastProgressAt?: string
    /** 心跳；滞留检测用 */
    heartbeatAt?: string
}

export type HistoryBackfillJobCore = {
    id: string
    jobNo: string
    mallId: string
    mallName: string
    environment: HistoryBackfillEnvironment
    cutoverId: string
    /** 服务端登记的必须覆盖起点；rangeStart 必须等于此值 */
    requiredHistoryStart: string
    rangeStart: string
    /** 半开区间右端 = T；occurredAt=T 不在范围内 */
    rangeEnd: string
    cutoverAt: string
    sourceCoverageStart?: string
    coverageComplete: boolean
    coverageGaps: CoverageGap[]
    processingStatus: HistoryBackfillProcessingStatus
    reportReviewStatus: HistoryBackfillReportReviewStatus
    pipelineStage: BackfillPipelineStage
    reportReviewPolicy?: {
        reportReviewPolicyId: string
        reportReviewPolicyVersion: number
    }
    formalDownstreamUnlocked: boolean
    lockVersion: number
    requestedBy: string
    requestedAt: string
    sourceAsOf: string
    fulfillmentNote: string
    scopeNote: string
    legacyManualNote: string
    progress: HistoryBackfillProgress
    costBasis: CostBasisRow[]
    /** 有成本消费金额 ÷ 总消费金额；NONE 进分母 */
    coverageRate: string | null
    coveragePercent: number
    allowedActions: FormalCommandAction[]
    actionBlockers: Array<{
        action: FormalCommandAction | string
        code: string
        message: string
    }>
    /** 正式任务 idempotency 命名空间；续跑复用 */
    idempotencyNamespace: string
    hasOverlappingFormalJob?: boolean
    overlappingJobNo?: string
}

export type HistoryBackfillListItem = {
    id: string
    jobNo: string
    mallId: string
    mallName: string
    environment: HistoryBackfillEnvironment
    rangeStart: string
    rangeEnd: string
    rangeLabel: string
    processingStatus: HistoryBackfillProcessingStatus
    reportReviewStatus: HistoryBackfillReportReviewStatus
    progressLabel: string
    processedCount: number
    totalCount: number
    deduplicatedCount: number
    unattributedCount: number
    costCoverageLabel: string
    coverageComplete: boolean
    lastProgressAt?: string
}

type HistoryBackfillListMetrics = {
    running: number
    unattributed: number
    deduplicated: number
    noneConsumption: number
    failed: number
}

export type HistoryBackfillListView = {
    metrics: HistoryBackfillListMetrics
    rows: HistoryBackfillListItem[]
    totalCount: number
    queriedAt: string
    /** 可创建草稿的目标范围（服务端计算） */
    createContext: CreateBackfillContext
}

export type CreateBackfillContext = {
    cutoverId: string
    mallId: string
    mallName: string
    environment: HistoryBackfillEnvironment
    requiredHistoryStart: string
    rangeEnd: string
    cutoverAt: string
    sourceCoverageStart: string
    coverageComplete: boolean
    coverageGaps: CoverageGap[]
    estimatedFactCount: number
    hasOverlappingFormalJob: boolean
    overlappingJobNo?: string
    canCreateDraft: boolean
    blockReasons: string[]
}

export type HistoryBackfillItemView = {
    itemId: string
    jobId: string
    factType: MallOrderFactType
    businessFactKeySummary: string
    mallOrderNo: string
    /** 退款/恢复等子单号 */
    sourceDocNo?: string
    occurredAt: string
    result: ItemResult
    costBasis?: CostBasis | "N_A"
    /** NONE 时为空；禁止展示 0 */
    costAmountNet?: string | null
    consumptionAmountGross?: string
    dedupeProof?: {
        matchedSource: "REALTIME" | "PRIOR_BACKFILL"
        originalMessageId: string
        formalFactId: string
        formalFactSummary: string
    }
    unattributedReason?: string
    /** 命中 W29 错误任务/对账差异时携带的稳定 work item（去接口错误中心入口用） */
    workItemId?: string
    failure?: {
        errorCode: string
        stage: string
        retryable: boolean
        summary: string
    }
    fulfillmentChain?: "LEGACY_MANUAL" | "N_A"
    /** 白名单字段；禁止卡号/卡密/手机/完整地址/原始报文 */
    whitelistFields: Array<{ field: string; label: string; value: string }>
}

type HistoryBackfillReportView = {
    reportId: string
    reportVersion: number
    generatedAt: string
    reviewLabel: "UNCONFIRMED" | "CONFIRMED"
    downloadLabel: string
    schemaVersion: string
    ruleVersion: string
    rangeStart: string
    rangeEnd: string
    cutoverAt: string
    totalCount: number
    totalAmount: string
    insertedCount: number
    deduplicatedCount: number
    unattributedCount: number
    failedCount: number
    costBasis: CostBasisRow[]
    coverageRate: string | null
    unattributedSummaries: string[]
    failedSummaries: string[]
    operatorLabel: string
    processingStatus: HistoryBackfillProcessingStatus
    reportReviewStatus: HistoryBackfillReportReviewStatus
    fullHistoryFinalComplete: boolean
    sensitiveRedactionNote: string
}

export type HistoryBackfillDetailView = {
    job: HistoryBackfillJobCore
    items: HistoryBackfillItemView[]
    /** 筛选后明细总数（分页用） */
    totalItems: number
    report?: HistoryBackfillReportView
    queriedAt: string
    permissionVersion: string
}

export type HistoryBackfillListQuery = {
    view: HistoryBackfillView
    mallId?: string
    environment?: HistoryBackfillEnvironment
    processingStatus?: HistoryBackfillProcessingStatus
    reportReviewStatus?: HistoryBackfillReportReviewStatus
    basis?: CostBasis
    q?: string
    page: number
    pageSize: number
}

export type HistoryBackfillDetailQuery = {
    jobId: string
    results?: ItemResult[]
    factTypes?: MallOrderFactType[]
    costBases?: CostBasis[]
    q?: string
    page: number
    pageSize: number
    section?: JobSection
}

export type HistoryBackfillCommandInput = {
    action: FormalCommandAction
    jobId?: string
    cutoverId?: string
    expectedLockVersion?: number
    rangeStart?: string
    rangeEnd?: string
    operationId: string
    idempotencyKey: string
    itemIds?: string[]
    reportVersion?: number
}

export type HistoryBackfillCommandResult = {
    status: FormalCommandResultStatus
    title: string
    description: string
    jobId?: string
    jobNo?: string
    operationId: string
    idempotencyKey: string
    nextStep?: string
    blockers?: string[]
}
