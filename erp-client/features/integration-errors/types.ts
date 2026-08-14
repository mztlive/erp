/**
 * W29 接口错误与对账中心 · 客户端契约类型
 * 对齐 docs/ui-workspaces/w29-integration-error-reconciliation.md §5/§7/§8
 */

import type { InterfaceErrorClass } from "@/components/business"
import type {
    AssignmentMode,
    WorkItemAllowedAction,
    WorkItemProcessingState,
    WorkItemStatus,
} from "@/features/work-items"

export type IntegrationItemType = "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"

export type IntegrationView =
    | "mine"
    | "result_unknown"
    | "security"
    | "auto_retry"
    | "reconciliation"
    | "resolved"

export type IntegrationMode = "all" | "errors"

export type IntegrationEnvironment = "production" | "verification"

export type IntegrationOwnerFilter = "me" | "team" | "assigned"

export type FundsImpact = "NONE" | "POTENTIAL" | "POSTED"

type QueryOutcome =
    | "TERMINAL_EVIDENCE_FOUND"
    | "NO_RESULT_CONFIRMED"
    | "RESULT_UNKNOWN"

type ActionOutcome =
    | QueryOutcome
    | "REPLAY_ACCEPTED"
    | "REATTRIBUTED"
    | "EVIDENCE_LINKED"
    | "EVIDENCE_ADDED"
    | "RESOLVED"
    | "CLOSED_DUPLICATE"
    | "CLOSED_MISROUTED"
    | "CONFIRMED_NO_ERROR"
    | "CONFIRMED_VALID_DIFFERENCE"

export type IntegrationActionKind =
    | "QUERY_ORIGINAL_RESULT"
    | "REPLAY_ORIGINAL"
    | "REATTRIBUTE"
    | "LINK_COMPENSATION"
    | "ADD_EVIDENCE"
    | "RESOLVE"
    | "CONFIRM_NO_ERROR"
    | "CONFIRM_VALID_DIFFERENCE"

export type ControlledEvidenceKind =
    | "EXTERNAL_CASE_RESULT"
    | "BUSINESS_OBJECT_VERIFICATION"
    | "FINANCIAL_RECONCILIATION"
    | "COMPENSATION_RESULT"
    | "DISTINCT_REVIEW"

export type DirectReconciliationReasonCode =
    | "SOURCE_CORRECTED_AND_REATTRIBUTED"
    | "BUSINESS_CONFIRMED_NO_ERROR"
    | "COMPENSATION_CLOSED"

export type ControlledTerminalEvidenceRef = {
    kind: ControlledEvidenceKind
    recordId: string
    label: string
}

export type ResolutionEvidencePolicyView = {
    evidencePolicyId: string
    evidencePolicyVersion: number
    key: { errorType: string; fundsImpact: FundsImpact }
    requiredEvidenceKinds: ControlledEvidenceKind[]
    reviewerSeparation:
        | "NONE"
        | "DISTINCT_REVIEWER"
        | "DISTINCT_FINANCE_REVIEWER"
}

export type RegisteredReconciliationReason = {
    registeredReasonId: DirectReconciliationReasonCode
    registeredReasonVersion: number
    conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE"
    label: string
    requiredEvidenceKinds: ControlledEvidenceKind[]
}

export type ReconciliationReasonRegistryView = {
    reasonRegistryId: string
    reasonRegistryVersion: number
    registeredReasons: RegisteredReconciliationReason[]
}

type IntegrationAttemptSummary = {
    attemptNumber: number
    attemptedAt: string
    result: string
    requestSummary?: string
    responseSummary?: string
    nextRetryAt?: string
}

type MessageEventSummary = {
    eventIdSummary: string
    idempotencyKeySummary: string
    businessFactKeySummary: string
    schemaVersion: string
    directionLabel: string
    /** Never full payload — masked technical summary only */
    maskedPayloadSummary: string
}

type OriginalActionView = {
    originalActionId: string
    originalActionIdempotencyKeySummary: string
    /** Always true in views; client must never send the raw key */
    originalActionIdempotencyKeyLocked: true
    actionLabel: string
}

type DifferenceEvidenceView = {
    leftLabel: string
    leftSummary: string
    rightLabel: string
    rightSummary: string
    boundary: string
    watermark: string
    differenceType: string
    differenceSummary: string
}

type RepairLink = {
    workspaceId: "W17" | "W21" | "W26" | "W20" | "W27"
    label: string
    href: string
}

type AuditEntryView = {
    id: string
    at: string
    actor: string
    action: string
    detail: string
}

export type IntegrationResolutionItemView = {
    identity: {
        itemType: IntegrationItemType
        id: string
        number: string
        subjectHash: string
    }
    workItem?: {
        workItemId: string
        workItemType: "INTEGRATION_RESULT_UNKNOWN" | "BUSINESS_EXCEPTION"
        taskVersion: string
        status: WorkItemStatus
        assignmentMode: AssignmentMode
        processingState: WorkItemProcessingState
        subjectVersion: string
        ownerUser?: { id: string; displayName: string }
        allowedActions: readonly WorkItemAllowedAction[]
    }
    businessObject: {
        objectType: string
        objectId: string
        title: string
    }
    classification: {
        code: string
        /** Maps to InterfaceErrorResolutionPanel errorClass when ERROR_TASK */
        errorClass: InterfaceErrorClass | "reconciliation-difference"
        label: string
        severity: "critical" | "high" | "medium" | "low"
        severityLabel: string
    }
    environment: IntegrationEnvironment
    environmentLabel: string
    status: {
        code: string
        label: string
    }
    fundsImpact: FundsImpact
    fundsImpactLabel: string
    compensationOpen: boolean
    ageLabel: string
    ownerRole: string
    ownerUser?: string
    createdAt: string
    dueAt?: string
    message?: MessageEventSummary
    originalAction?: OriginalActionView
    difference?: DifferenceEvidenceView
    resolutionEvidencePolicy?: ResolutionEvidencePolicyView
    reconciliationReasonRegistry?: ReconciliationReasonRegistryView
    /** Direct recon only: no work_item path */
    hasWorkItem: boolean
    attempts: IntegrationAttemptSummary[]
    objectVersion: string
    allowedActions: IntegrationActionKind[]
    actionBlockers: {
        action: string
        code: string
        message: string
    }[]
    repairLinks: RepairLink[]
    auditTrail: AuditEntryView[]
    evidenceTimeline: AuditEntryView[]
    /** Session-derived query stage for RESULT_UNKNOWN gate */
    queryStage?: QueryOutcome | null
    /** Appended evidence refs (session) */
    linkedEvidence: ControlledTerminalEvidenceRef[]
    freshness: { updatedAt: string; sourceWatermark?: string }
}

type IntegrationMetrics = {
    resultUnknown: number
    manualRequired: number
    securityFaults: number
    openDifferences: number
    longestAgeLabel: string
}

export type IntegrationResolutionQuery = {
    view: IntegrationView
    mode: IntegrationMode
    environment: IntegrationEnvironment | "all"
    errorClass?: string
    owner: IntegrationOwnerFilter
    q?: string
    queueContextId?: string
    resolveWorkItemId?: string
    currentTaskId?: string
    currentDifferenceId?: string
    autoNext?: boolean
}

export type IntegrationQueueView = {
    items: IntegrationResolutionItemView[]
    metrics: IntegrationMetrics
    context: {
        queueContextId: string
        filterSummary: string
        updatedAt: string
    }
    /** After resolveWorkItemId succeeds, client should replace URL to this target */
    resolvedEntry?: {
        itemType: IntegrationItemType
        id: string
        workItemId: string
    }
}

type FormalActionStatus =
    | "succeeded"
    | "blocked"
    | "rejected"
    | "unknown"
    | "failed"

export type IntegrationFormalResult = {
    status: FormalActionStatus
    title: string
    description: string
    reference?: string
    outcome?: ActionOutcome
    nextAllowedActions?: IntegrationActionKind[]
    workItemStatus?: WorkItemStatus
    /** Stay on current item — never auto-next for non-terminal / unknown */
    stayOnItem: boolean
    terminal?: boolean
    replacementWorkItemId?: string
    facts?: { label: string; value: string }[]
}

export type IntegrationTaskActionInput = {
    itemType: IntegrationItemType
    itemId: string
    workItemId: string
    expectedSubjectVersion: string
    expectedTaskVersion: string
    kind:
        | "QUERY_ORIGINAL_RESULT"
        | "REPLAY_ORIGINAL"
        | "REATTRIBUTE"
        | "LINK_COMPENSATION"
        | "ADD_EVIDENCE"
    operationId: string
    idempotencyKey: string
    reasonCode?: string
    comment?: string
    evidenceRefs?: ControlledTerminalEvidenceRef[]
}

export type IntegrationResolveInput = {
    itemType: IntegrationItemType
    itemId: string
    workItemId: string
    expectedSubjectVersion: string
    expectedTaskVersion: string
    operationId: string
    idempotencyKey: string
    reasonCode: "TERMINAL_EVIDENCE_VERIFIED"
    evidencePolicyId: string
    evidencePolicyVersion: number
    policyKey: { errorType: string; fundsImpact: FundsImpact }
    evidenceRefs: ControlledTerminalEvidenceRef[]
    comment?: string
}

export type DirectReconciliationInput = {
    differenceId: string
    expectedDifferenceVersion: string
    operationId: string
    idempotencyKey: string
    decision:
        | {
              kind: "NON_TERMINAL_ACTION"
              action:
                  | "ADD_EVIDENCE"
                  | "QUERY_ORIGINAL_RESULT"
                  | "REPLAY_ORIGINAL"
                  | "REATTRIBUTE"
                  | "LINK_COMPENSATION"
              evidenceRefs?: ControlledTerminalEvidenceRef[]
              comment?: string
          }
        | {
              kind: "TERMINAL_CONCLUSION"
              reasonCode: DirectReconciliationReasonCode
              reasonRegistryId: string
              reasonRegistryVersion: number
              registeredReasonId: DirectReconciliationReasonCode
              conclusion: "CONFIRM_NO_ERROR" | "CONFIRM_VALID_DIFFERENCE"
              evidenceRefs: ControlledTerminalEvidenceRef[]
              comment?: string
          }
}

export const ERROR_CLASS_LABEL: Record<string, string> = {
    "capability-unsupported": "能力不足",
    "parameter-or-mapping": "参数/映射错误",
    "business-rejected": "供应商业务拒绝",
    "network-timeout": "临时故障",
    "result-unknown": "结果未知",
    "authentication-or-signature": "鉴权/签名失败",
    "rate-limited": "调用次数受限",
    "duplicate-callback": "重复通知",
    "out-of-order-callback": "通知顺序异常",
    "reconciliation-difference": "对账差异",
}

export const VIEW_LABEL: Record<IntegrationView, string> = {
    mine: "我的任务",
    result_unknown: "结果未知",
    security: "安全故障",
    auto_retry: "自动重试",
    reconciliation: "对账差异",
    resolved: "已解决",
}

export const MODE_LABEL: Record<IntegrationMode, string> = {
    all: "全部",
    errors: "错误任务",
}

export const ENV_LABEL: Record<IntegrationEnvironment | "all", string> = {
    all: "全部环境",
    production: "生产",
    verification: "验证",
}

export const OWNER_LABEL: Record<IntegrationOwnerFilter, string> = {
    me: "我的任务",
    team: "团队待处理",
    assigned: "已分派",
}

export const FUNDS_LABEL: Record<FundsImpact, string> = {
    NONE: "无资金影响",
    POTENTIAL: "潜在资金影响",
    POSTED: "已入账资金",
}

export const EVIDENCE_KIND_LABEL: Record<ControlledEvidenceKind, string> = {
    EXTERNAL_CASE_RESULT: "外部案例结果",
    BUSINESS_OBJECT_VERIFICATION: "业务对象核验",
    FINANCIAL_RECONCILIATION: "财务对账",
    COMPENSATION_RESULT: "补偿结果",
    DISTINCT_REVIEW: "独立复核",
}

/** 对账差异类型中文映射（differenceType） */
export const DIFFERENCE_TYPE_LABEL: Record<string, string> = {
    AMOUNT_AND_LINE_COUNT: "金额与行数差异",
    MISSING_ERP_FACT: "ERP 无对应记录",
    MISSING_MALL_FACT: "商城无对应记录",
    STATUS_MISMATCH: "状态不一致",
}

/** 岗位分离策略中文映射（reviewerSeparation） */
export const REVIEWER_SEPARATION_LABEL: Record<string, string> = {
    NONE: "无独立复核要求",
    DISTINCT_REVIEWER: "需独立复核",
    DISTINCT_FINANCE_REVIEWER: "需财务独立复核",
}
