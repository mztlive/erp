/**
 * W27 · API 供应商结算 · 客户端契约
 * 对齐 docs/ui-workspaces/w27-api-settlement.md §5/§7/§8
 */

import type { StatusTone } from "@/components/ui/status-badge"
import type {
    AssignmentMode,
    WorkItemAllowedAction,
    WorkItemProcessingState,
    WorkItemStatus,
} from "@/features/work-items/types"

export type SettlementView =
    | "pending"
    | "prepared_by_me"
    | "review_by_me"
    | "confirmed"

export type SettlementStatus =
    | "DRAFT"
    | "PENDING_RECONCILE"
    | "HAS_DIFFERENCE"
    | "PENDING_REVIEW"
    | "CONFIRMED"
    | "VOIDED"

export type DifferenceType =
    | "MISSING_ORDER"
    | "DUPLICATE"
    | "AMOUNT"
    | "REFUND"
    | "STATUS"

/**
 * 差异结论状态（单轨，5 值固定枚举，对齐 docs/erp-data-model.md §6.20）：
 * 结论即状态。「待举证」/「阻塞」是标志而非状态，由
 * `requiresProcurementEvidence` / `blocking` 字段表达。
 */
export type DifferenceStatus =
    | "PENDING"
    | "SUPPLIER_ACCEPTED"
    | "ERP_ACCEPTED"
    | "COMPENSATED"
    | "CLOSED"

/**
 * 处理动作/结论记录（提交结论动作枚举，值不变）。
 * `status` 是结论状态（单轨），`resolution` 是追加式处理记录，
 * 两者并存：`DifferenceResolutionRecord.resolution` 与 `status` 对应但不互斥。
 */
export type DifferenceResolution =
    | "SUPPLIER_ACCEPTED"
    | "ERP_ACCEPTED"
    | "COMPENSATED"
    | "CLOSED_NO_ADJUSTMENT"

/** 处理动作 → 结论状态（CLOSED_NO_ADJUSTMENT 动作落为 CLOSED 状态） */
export {
    AUDIT_ACTION_LABEL,
    DIFF_STATUS_LABEL,
    DIFF_TYPE_LABEL,
    REASON_CODE_LABEL,
    RESOLUTION_LABEL,
    RESOLUTION_TO_STATUS,
    SECTION_LABEL,
    SECTIONS,
    STATUS_LABEL,
    STATUS_TONE,
    VIEW_LABEL,
} from "./lib/supplier-settlements-labels"

export type SettlementSection =
    | "overview"
    | "items"
    | "differences"
    | "review"
    | "payable"
    | "audit"

type EmptyReason =
    | "NO_PERMISSION"
    | "NO_SCOPE"
    | "NO_STATEMENTS"
    | "FILTER_NO_RESULT"

type ActionBlocker = {
    action: string
    code: string
    message: string
    destinationWorkspaceId?: string
}

type ActorView = {
    userId: string
    displayName: string
}

export type SettlementListRow = {
    statementId: string
    statementNo: string
    supplierId: string
    supplierName: string
    periodStart: string
    periodEnd: string
    periodLabel: string
    status: SettlementStatus
    statusLabel: string
    statusTone: StatusTone
    erpAmountGross: string
    supplierAmountGross?: string
    differenceAmountGross?: string
    differenceDirectionLabel?: string
    unresolvedDifferenceCount: number
    preparedBy?: ActorView
    reviewedBy?: ActorView
    preparedByLabel: string
    reviewedByLabel: string
    updatedAt: string
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
}

type SettlementListTotals = {
    pendingReconcile: number
    hasDifference: number
    pendingReview: number
    confirmedAmountThisPeriod: string
}

export type SettlementListView = {
    view: SettlementView
    rows: SettlementListRow[]
    page: number
    pageSize: number
    total: number
    totals: SettlementListTotals
    metrics: {
        pending: number
        hasDifference: number
        pendingReview: number
        confirmedAmount: string
    }
    suppliers: Array<{ supplierId: string; supplierName: string }>
    emptyReason?: EmptyReason
    hasModulePermission: boolean
    hasDataScope: boolean
    permissionVersion: string
    sourceAsOf: string
    queriedAt: string
    filterSummary: string
}

type SettlementItemView = {
    itemId: string
    supplierOrderNo: string
    /** 采购单号（W08 业务单号）；结算明细可直达采购单详情 */
    purchaseNo?: string
    /** 采购单内部 id，用于详情深链；无 id 时回退采购单列表搜索 */
    purchaseOrderId?: string
    externalOrderNo: string
    productName: string
    quantity: string
    factLabel: string
    orderAmountGross: string
    freightGross: string
    serviceFeeGross: string
    refundGross: string
    erpAmountGross: string
    erpAmountNet: string
    erpTaxAmount: string
    supplierBillLineGross?: string
    supplierBillLineNet?: string
    supplierBillLineTax?: string
    /** 只读：页面不得改写 */
    readOnly: true
}

type DifferenceEvidence = {
    evidenceId: string
    referenceIds: string[]
    kind: "PROCUREMENT_OPINION" | "SUPPLIER_CONFIRM" | "TICKET" | "ATTACHMENT"
    label: string
    comment?: string
    by: ActorView
    at: string
}

type DifferenceResolutionRecord = {
    resolutionId: string
    resolution: DifferenceResolution
    resolutionLabel: string
    reasonCode: string
    reasonLabel: string
    by: ActorView
    at: string
    costImpactPreview?: string
}

export type SettlementDifferenceView = {
    differenceId: string
    type: DifferenceType
    typeLabel: string
    status: DifferenceStatus
    statusLabel: string
    statusTone: StatusTone
    blocking: boolean
    erpSideLabel: string
    erpSideAmount?: string
    supplierSideLabel: string
    supplierSideAmount?: string
    amountDirectionLabel: string
    amountGross?: string
    version: number
    evidence: DifferenceEvidence[]
    resolution?: DifferenceResolutionRecord
    requiresProcurementEvidence: boolean
    leftFields: Array<{
        id: string
        field: string
        before: string
        after: string
        note?: string
    }>
}

type ReviewRecordView = {
    recordId: string
    action: "SUBMIT" | "REJECT" | "CONFIRM"
    actionLabel: string
    by: ActorView
    at: string
    comment?: string
    reasonCode?: string
}

type AuditEventView = {
    eventId: string
    at: string
    actor: string
    action: string
    summary: string
    auditNo?: string
}

type SettlementTotalsView = {
    orderAmountGross: string
    freightGross: string
    serviceFeeGross: string
    refundGross: string
    erpAmountGross: string
    supplierAmountGross?: string
    differenceAmountGross?: string
    differenceDirectionLabel?: string
    taxBasisLabel: "含税"
    pendingCostDeltaGross?: string
    confirmedCostDeltaGross?: string
}

type PayableLinkView = {
    payableAccountId: string
    payableNo: string
    grossAmount: string
    dueDate: string
    statusLabel: string
    w12Href: string
}

export type SettlementDetailView = {
    statement: {
        id: string
        statementNo: string
        supplierId: string
        supplierName: string
        periodStart: string
        periodEnd: string
        periodLabel: string
        externalBillNo?: string
        externalBillVersion?: string
        erpAmountGross: string
        supplierAmountGross?: string
        differenceAmountGross?: string
        differenceDirectionLabel?: string
        status: SettlementStatus
        statusLabel: string
        statusTone: StatusTone
        preparedBy?: ActorView
        reviewedBy?: ActorView
        lockVersion: number
        subjectHash?: string
        sourceAsOf: string
        sourceSnapshotAt: string
        sourceSnapshotHash?: string
    }
    totals: SettlementTotalsView
    items: SettlementItemView[]
    differences: SettlementDifferenceView[]
    differenceSummary: {
        total: number
        open: number
        blocking: number
        resolved: number
    }
    reviewRecords: ReviewRecordView[]
    payable?: PayableLinkView
    workItem?: {
        workItemId: string
        taskVersion: string
        workItemType: "SUPPLIER_SETTLEMENT_REVIEW"
        businessObjectType: "SUPPLIER_SETTLEMENT_STATEMENT"
        businessObjectId: string
        subjectVersion: string
        assignmentMode: AssignmentMode
        processingState: WorkItemProcessingState
        ownerUser?: { id: string; displayName: string }
        status: WorkItemStatus
        allowedTaskActions: readonly WorkItemAllowedAction[]
        actionBlockers: readonly string[]
    }
    workItemBlocker?: ActionBlocker
    reviewSubmissionPolicy?: {
        refreshCutoffPolicyId: string
        version: string
    }
    auditEvents: AuditEventView[]
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
    freshness: {
        immutableFactsAsOf: string
        externalBillAsOf?: string
        w26ProjectionUpdatedAt?: string
        queriedAt: string
    }
    canEditBillOrOrder: false
}

export type FormalOutcome = {
    status: "succeeded" | "failed" | "blocked" | "unknown" | "rejected"
    code?: string
    title: string
    message: string
    reference?: string
    operationId?: string
    idempotencyKey?: string
    statementId?: string
    payableNo?: string
    payableAccountId?: string
    costDeltaGross?: string
    sourceSnapshotHash?: string
    subjectHash?: string
    lockVersion?: number
    facts?: Array<{ label: string; value: string }>
}

export type CreateDraftInput = {
    supplierId: string
    periodStart: string
    periodEnd: string
    requestId: string
    idempotencyKey: string
}

export type RefreshDraftInput = {
    statementId: string
    expectedLockVersion: number
    expectedSourceSnapshotHash: string
    requestId: string
    idempotencyKey: string
}

export type AppendEvidenceInput = {
    statementId: string
    differenceId: string
    expectedDifferenceVersion: number
    evidenceReferenceIds: string[]
    opinionCode?: string
    comment?: string
    requestId: string
    idempotencyKey: string
}

export type ResolveDifferenceInput = {
    statementId: string
    differenceId: string
    expectedLockVersion: number
    expectedDifferenceVersion: number
    resolution: DifferenceResolution
    reasonCode: string
    evidenceReferenceIds: string[]
    operationId: string
    idempotencyKey: string
}

export type SubmitReviewInput = {
    statementId: string
    expectedLockVersion: number
    subjectHash: string
    refreshCutoffPolicyId: string
    expectedRefreshCutoffPolicyVersion: string
    operationId: string
    idempotencyKey: string
    comment?: string
}

export type ReviewDecisionInput = {
    statementId: string
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    expectedLockVersion: number
    action: "REJECT" | "CONFIRM"
    operationId: string
    idempotencyKey: string
    reasonCode?: string
    comment?: string
}

// 枚举/动作中文映射表已移至 lib/supplier-settlements-labels.ts（上面统一再导出）。
// 本文件剩余内容为纯类型声明（客户端契约），不拆分以免破坏按域聚合的引用面。
