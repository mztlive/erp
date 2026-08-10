/**
 * W18 导入与期初 · 客户端契约类型
 * 对齐 docs/ui-workspaces/w18-import-opening.md §5/§8。
 */

import type { ImportStageKey } from "@/components/business"

/** 文档阶段（六段流水线） */
export type ImportPipelineStage =
    | "RECEIVE"
    | "VALIDATE"
    | "TRIAL"
    | "CONFIRM"
    | "APPLY"
    | "RESULT"

export type ImportEnvironment = "VALIDATION" | "PRODUCTION"

export type ImportBatchStatus =
    | "RECEIVING"
    | "SCANNING"
    | "VALIDATING"
    | "TRIAL_READY"
    | "AWAITING_CONFIRMATION"
    | "CONFIRMATION_BLOCKED"
    | "READY_TO_APPLY"
    | "APPLYING"
    | "PARTIAL_SUCCESS"
    | "SUCCEEDED"
    | "FAILED"
    | "CANCELLED"

export type ImportObjectCode =
    | "CUSTOMER"
    | "CONTRACT"
    | "SUPPLIER"
    | "WAREHOUSE"
    | "OPENING_STOCK"
    | "SKU"
    | "CARD_CATEGORY"
    | "CARD_SALES_ORDER"
    | "CARD_OPENING_AR"

export type ConfirmationScope =
    | "SALES"
    | "PROCUREMENT"
    | "OPERATIONS"
    | "WAREHOUSE"
    | "FINANCE"

type ConfirmationResult = "PENDING" | "CONFIRMED" | "REJECTED" | "INVALIDATED"

export type IssueRowStatus =
    | "PENDING_MAPPING"
    | "CONFLICT"
    | "FAILED"
    | "SKIPPED"

export type ImportIssueCode =
    | "CUSTOMER_NOT_FOUND"
    | "AMOUNT_PRECISION"
    | "BASELINE_DATE_MISMATCH"
    | "HISTORY_FLOW_FORBIDDEN"
    | "CARD_DRAFT_EXCLUDED"
    | "MAPPING_CONFLICT"
    | "QUALIFICATION_EXPIRED"
    | "STOCK_QTY_INVALID"

export type RetentionClass = "SUCCESS_AUDIT" | "FAILURE_DIAG" | "EXPORT_TEMP"

export type BatchSection =
    | "overview"
    | "files"
    | "trial"
    | "confirm"
    | "progress"
    | "result"
    | "audit"

/** 文档阶段 → ImportStageIndicator 键 */
export const PIPELINE_TO_INDICATOR: Record<
    ImportPipelineStage,
    ImportStageKey
> = {
    RECEIVE: "upload",
    VALIDATE: "mapping",
    TRIAL: "validation",
    CONFIRM: "preview",
    APPLY: "submission",
    RESULT: "result",
}

export const PIPELINE_STAGE_LABEL: Record<ImportPipelineStage, string> = {
    RECEIVE: "安全接收",
    VALIDATE: "结构校验",
    TRIAL: "业务校验与试算",
    CONFIRM: "责任确认",
    APPLY: "后台应用",
    RESULT: "结果",
}

export const ENVIRONMENT_LABEL: Record<ImportEnvironment, string> = {
    VALIDATION: "验证环境",
    PRODUCTION: "生产环境",
}

export const OBJECT_CODE_LABEL: Record<ImportObjectCode, string> = {
    CUSTOMER: "客户",
    CONTRACT: "合同",
    SUPPLIER: "供应商",
    WAREHOUSE: "仓库",
    OPENING_STOCK: "期初库存",
    SKU: "商品 SKU",
    CARD_CATEGORY: "卡券类目",
    CARD_SALES_ORDER: "卡券销售单",
    CARD_OPENING_AR: "卡券期初应收",
}

export const CONFIRMATION_SCOPE_LABEL: Record<ConfirmationScope, string> = {
    SALES: "销售确认",
    PROCUREMENT: "采购确认",
    OPERATIONS: "运营确认",
    WAREHOUSE: "仓储确认",
    FINANCE: "财务确认",
}

export const BATCH_STATUS_LABEL: Record<ImportBatchStatus, string> = {
    RECEIVING: "接收中",
    SCANNING: "安全扫描",
    VALIDATING: "校验中",
    TRIAL_READY: "试算完成",
    AWAITING_CONFIRMATION: "待业务确认",
    CONFIRMATION_BLOCKED: "确认入口阻断",
    READY_TO_APPLY: "可提交应用",
    APPLYING: "后台应用中",
    PARTIAL_SUCCESS: "部分成功",
    SUCCEEDED: "已成功",
    FAILED: "失败",
    CANCELLED: "已取消",
}

export const BATCH_STATUS_TONE: Record<
    ImportBatchStatus,
    "neutral" | "info" | "success" | "warning" | "destructive"
> = {
    RECEIVING: "info",
    SCANNING: "info",
    VALIDATING: "info",
    TRIAL_READY: "info",
    AWAITING_CONFIRMATION: "warning",
    CONFIRMATION_BLOCKED: "warning",
    READY_TO_APPLY: "success",
    APPLYING: "info",
    PARTIAL_SUCCESS: "warning",
    SUCCEEDED: "success",
    FAILED: "destructive",
    CANCELLED: "neutral",
}

export const ISSUE_CODE_LABEL: Record<ImportIssueCode, string> = {
    CUSTOMER_NOT_FOUND: "客户不存在",
    AMOUNT_PRECISION: "金额精度超限",
    BASELINE_DATE_MISMATCH: "基准日不一致",
    HISTORY_FLOW_FORBIDDEN: "禁止历史流水",
    CARD_DRAFT_EXCLUDED: "卡券草稿不迁移",
    MAPPING_CONFLICT: "映射冲突",
    QUALIFICATION_EXPIRED: "资质过期",
    STOCK_QTY_INVALID: "库存数量无效",
}

export const ROW_STATUS_LABEL: Record<IssueRowStatus, string> = {
    PENDING_MAPPING: "待映射",
    CONFLICT: "冲突",
    FAILED: "失败",
    SKIPPED: "跳过",
}

export const RETENTION_LABEL: Record<RetentionClass, string> = {
    SUCCESS_AUDIT: "成功审计资产 · 长期保留",
    FAILURE_DIAG: "失败诊断 · 保留 30 天",
    EXPORT_TEMP: "导出结果 · 保留 7 天",
}

/** 权威模型尚未登记时的固定实施 blocker 文案 */
export const WORK_ITEM_TYPE_BLOCKER = {
    message:
        "导入业务确认/退回任务尚未配置；配置前不得启用业务确认入口，也不得借用异常入口伪装正常必经确认。",
    requiredRegistration: [
        "确认与退回任务配置",
        "责任归属配置",
        "队列展示配置",
    ] as const,
}

type SafeFileAssetView = Readonly<{
    assetId: string
    fileName: string
    byteSize: number
    securityScanStatus: "PENDING" | "PASSED" | "REJECTED" | "QUARANTINED"
    contentHmacShort?: string
    retentionClass: RetentionClass
    expiresAt?: string
    /** 绝不回显：原始 SQL / 连接头 / 存储键 */
}>

export type ImportConfirmationView = Readonly<{
    scope: ConfirmationScope
    result: ConfirmationResult
    confirmedByLabel?: string
    confirmedAt?: string
    trialVersion: string
    comment?: string
    inViewerResponsibility: boolean
}>

type BackgroundJobView = Readonly<{
    jobId: string
    status: "queued" | "running" | "succeeded" | "partial" | "failed" | "frozen"
    mode: "partialAllowed"
    total: number
    processed: number
    succeeded: number
    skipped: number
    failed: number
    updatedAt: string
}>

type ApplyPartitionItem = Readonly<{
    id: string
    label: string
    detail?: string
    code?: string
    objectHref?: string
}>

export type ImportBatchListItem = Readonly<{
    batchId: string
    batchNo: string
    environment: ImportEnvironment
    sourceObjectSet: readonly ImportObjectCode[]
    baselineDate: string
    importRuleVersion: string
    stage: ImportPipelineStage
    status: ImportBatchStatus
    progressLabel: string
    confirmationSummary: string
    initiatorLabel: string
    updatedAt: string
}>

export type ImportBatchView = Readonly<{
    batchId: string
    batchNo: string
    environment: ImportEnvironment
    sourceSystem: { id: string; name: string }
    sourceObjectSet: readonly ImportObjectCode[]
    baselineDate: string
    importRuleVersion: string
    trialVersion: string
    stage: ImportPipelineStage
    status: ImportBatchStatus
    /** 上传/扫描成功后仍为 true，直至 RESULT 且成功形成业务对象 */
    formalDataFormed: boolean
    notFormalDataMessage: string
    inputAsset?: SafeFileAssetView
    resultAssets: readonly SafeFileAssetView[]
    metrics: {
        total: number
        valid: number
        conflict: number
        failed: number
        skipped: number
    }
    confirmations: readonly ImportConfirmationView[]
    backgroundJob?: BackgroundJobView
    /** 部分成功分区 */
    applyPartitions?: {
        succeeded: readonly ApplyPartitionItem[]
        skipped: readonly ApplyPartitionItem[]
        failed: readonly ApplyPartitionItem[]
    }
    repairBatchId?: string
    repairBatchNo?: string
    /** 生产应用门禁 */
    productionGates: {
        validationEnvPassed: boolean
        allConfirmationsComplete: boolean
        noBlockingIssues: boolean
        trialVersionMatches: boolean
        ruleVersionStable: boolean
        workItemTypeRegistered: boolean
    }
    /** 试算/规则变化使旧确认失效 */
    invalidation?: {
        reason: string
        previousTrialVersion: string
        previousRuleVersion?: string
        invalidatedAt: string
    }
    /** 期初口径提示（按对象集） */
    openingPolicyHints: readonly {
        objectCode: ImportObjectCode
        message: string
    }[]
    allowedActions: readonly string[]
    actionBlockers: readonly {
        action: string
        code: string
        message: string
    }[]
    version: string
    updatedAt: string
    initiatorLabel: string
    linkedValidationBatchId?: string
    linkedValidationBatchNo?: string
}>

type ImportIssueRowView = Readonly<{
    issueId: string
    batchId: string
    issueCode: ImportIssueCode
    objectType: ImportObjectCode
    sourceRowNo: number
    sourceColumnName: string
    rowStatus: IssueRowStatus
    errorDetail: string
    repairable: boolean
}>

export type ImportBatchListQuery = {
    environment: ImportEnvironment
    status?: string
    objectType?: ImportObjectCode | "all"
    q?: string
    page: number
    pageSize: number
}

export type ImportBatchListView = Readonly<{
    metrics: {
        pendingValidate: number
        pendingConfirm: number
        applying: number
        failedOrPartial: number
    }
    rows: readonly ImportBatchListItem[]
    totalCount: number
    queriedAt: string
}>

export type ImportIssueQuery = {
    batchId: string
    issueCode?: ImportIssueCode | "all"
    objectType?: ImportObjectCode | "all"
    rowStatus?: IssueRowStatus | "all"
    page: number
    pageSize: number
}

export type ImportIssuePage = Readonly<{
    rows: readonly ImportIssueRowView[]
    totalCount: number
    issueVersion: string
    queriedAt: string
}>
