/**
 * W30 历史消费回填 · 客户端契约类型
 *
 * 枚举中文映射与阶段常量已拆到 lib/labels.ts，此处按原名重新导出，
 * 既有 import "@/features/history-backfill/types" 不受影响。
 */

export {
    COST_BASIS_LABEL,
    ENVIRONMENT_LABEL,
    FACT_TYPE_LABEL,
    FAILURE_CODE_LABEL,
    ITEM_RESULT_LABEL,
    ITEM_RESULT_TONE,
    PIPELINE_ORDER,
    PIPELINE_STAGE_LABEL,
    PIPELINE_TO_INDICATOR,
    PROCESSING_STATUS_LABEL,
    PROCESSING_STATUS_TONE,
    REPORT_REVIEW_STATUS_LABEL,
    REPORT_REVIEW_STATUS_TONE,
    VIEW_LABEL,
} from "@/features/history-backfill/lib/labels"

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

export type HistoryBackfillReportView = {
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
