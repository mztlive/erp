/** W13 卡券票款复核 · 客户端契约类型（对齐工作面文档 §5/§8）。 */

import type { WorkItemStatus } from "@/features/work-items"

export type ReviewType = "OPENING" | "SYNC_DELTA"

export type WorkItemType = "CARD_FUNDS_REVIEW" | "CARD_FUNDS_DELTA_REVIEW"

/** 当前任务允许执行的领域动作（责任动作 + 复核/登记动作）。 */
export type WorkItemAction =
    | "CONFIRM_ZERO"
    | "APPROVE"
    | "REJECT"
    | "REASSIGN"
    | "REGISTER_RECEIPT"
    | "REGISTER_INVOICE"

type ReviewResult = "APPROVED" | "REJECTED"

export type ApproveConclusion =
    | "NO_HISTORY_FROM_ZERO"
    | "RECORDED_FACTS_RECONCILED"

export type RejectReasonCode =
    | "EVIDENCE_INSUFFICIENT"
    | "FACTS_MISMATCH"
    | "COUNTERPARTY_UNCLEAR"
    | "OTHER"

type FundsReliability =
    | "VERIFIED"
    | "UNRELIABLE_PENDING_REVIEW"
    | "STALE_FINGERPRINT"

type ReceiptSummary = Readonly<{
    receiptId: string
    receiptNo: string
    receivedAt: string
    grossAmount: string
    allocatedToAccount: string
    otherAllocationSummary?: string
    reversed: boolean
}>

type InvoiceSummary = Readonly<{
    invoiceId: string
    invoiceNo: string
    direction: "BLUE" | "RED"
    issuedAt: string
    grossAmount: string
    netAmount: string
    taxAmount: string
    allocatedToAccount: string
    reversed: boolean
}>

type ReviewHistoryItem = Readonly<{
    reviewId: string
    reviewNo: number
    reviewType: ReviewType
    reviewResult: ReviewResult
    conclusion: ApproveConclusion | "REJECTED"
    reviewerLabel: string
    completedAt: string
    subjectHashAtReview: string
    predecessorReviewId?: string
    /** 历史只读：前端禁止编辑/删除 */
    readOnly: true
}>

type DiffChange = Readonly<{
    id: string
    field: string
    before: string
    after: string
    note?: string
    sourceObject?: string
    occurredAt?: string
}>

type ReviewDifference = Readonly<{
    title: string
    baselineReviewNo?: number
    baselineSubjectHash?: string
    changes: readonly DiffChange[]
    invalidatedAt?: string
}>

type EvidenceDraft = Readonly<{
    evidenceDocumentIds: string[]
    evidenceReferences: string[]
    comment?: string
}>

export type AllocationDraftLine = Readonly<{
    lineId: string
    targetAccountId: string
    targetLabel: string
    amount: string
}>

export type CardFundsReviewItemView = Readonly<{
    workItem: {
        workItemId: string
        taskVersion: string
        workItemType: WorkItemType
        subjectVersion: string
        workItemStatus: WorkItemStatus
        dueAt?: string
        ownerUser?: { id: string; displayName: string }
        allowedActions: readonly WorkItemAction[]
        actionBlockers: readonly {
            action: string
            code: string
            message: string
        }[]
        reason: string
        impact: string
        priority: number
    }
    salesOrder: {
        id: string
        orderNo: string
        revisionNo: number
        snapshotAt: string
    }
    account: {
        id: string
        accountSeq: number
        domainVersion: string
        customerId: string
        customerName: string
        counterpartyPartyId: string
        counterpartyPartyName: string
        mallName: string
        reviewStatus: string
        grossTotal: string
        settledTotal: string
        openTotal: string
        invoicedTotal: string
        openInvoiceableTotal: string
        /** 同步成交额（可能与当前应收不同） */
        syncedGrossAmount: string
        fundsReliability: FundsReliability
        reliabilityNote: string
    }
    reviewChain: {
        tailReviewId?: string
        chainVersion: string
        nextReviewNo: number
        items: readonly ReviewHistoryItem[]
    }
    currentSalesOrderRevisionId: string
    fundsFactVersion: string
    receiptFacts: readonly ReceiptSummary[]
    invoiceFacts: readonly InvoiceSummary[]
    difference?: ReviewDifference
    reviewType: ReviewType
    fingerprintStatus: {
        label: string
        tone: "success" | "warning" | "destructive" | "info" | "neutral"
        detail: string
    }
    currentEvidence: EvidenceDraft
}>

export type CardFundsReviewQueueQuery = {
    queueContextId?: string
    currentWorkItemId?: string
    type: "all" | "opening" | "delta"
    scope: "mine" | "history"
    q?: string
    due?: "all" | "today" | "overdue"
    status?: "OPEN" | "COMPLETED" | "CLOSED"
    sort?: string
    autoNext?: boolean
}

export type CardFundsReviewQueueView = Readonly<{
    preferences: { autoNextDefault: boolean }
    context: {
        queueContextId: string
        position: number
        total: number
        currentWorkItemId?: string
        previousWorkItemId?: string
        nextWorkItemId?: string
        filterSummary: string
        queueContextUpdatedAt: string
    }
    tasks: readonly CardFundsReviewItemView[]
    current?: CardFundsReviewItemView
    emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
}>

/** 统一动作命令<CardFundsReviewDecision> 的 decision 段 */
export type CardFundsReviewDecision =
    | {
          reviewResult: "APPROVED"
          conclusion: ApproveConclusion
          receivableAccountId: string
          expectedAccountSeq: number
          expectedAccountDomainVersion: string
          expectedReviewChainTailId?: string
          expectedReviewChainVersion: string
          expectedNextReviewNo: number
          expectedSalesOrderRevisionId: string
          expectedFundsFactVersion: string
          reviewType: ReviewType
          evidenceDocumentIds: string[]
          evidenceReferences: string[]
          comment?: string
      }
    | {
          reviewResult: "REJECTED"
          conclusion: "REJECTED"
          reasonCode: RejectReasonCode
          receivableAccountId: string
          expectedAccountSeq: number
          expectedAccountDomainVersion: string
          expectedReviewChainTailId?: string
          expectedReviewChainVersion: string
          expectedNextReviewNo: number
          expectedSalesOrderRevisionId: string
          expectedFundsFactVersion: string
          reviewType: ReviewType
          evidenceDocumentIds: string[]
          evidenceReferences: string[]
          comment: string
      }

export type CompleteCardFundsReviewCommand = Readonly<{
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    decision: CardFundsReviewDecision
    idempotencyKey: string
}>

type CardFundsReviewBusinessResultBase = Readonly<{
    receivableFundsReviewId: string
    receivableAccountId: string
    reviewNo: number
    accountReviewStatus: string
    workflowActionId: string
    operationId: string
    completedAt: string
}>

export type FormalOutcome =
    | {
          kind: "APPROVED"
          business: CardFundsReviewBusinessResultBase & {
              reviewResult: "APPROVED"
              conclusion: ApproveConclusion
          }
      }
    | {
          kind: "REJECTED"
          business: CardFundsReviewBusinessResultBase & {
              reviewResult: "REJECTED"
              conclusion: "REJECTED"
              followUpWorkItem: {
                  workItemId: string
                  workItemType: WorkItemType
                  status: "OPEN"
              }
          }
      }

export type FormalActionResponse =
    | { status: "succeeded"; outcome: FormalOutcome }
    | { status: "failed"; message: string; code: string }
    | { status: "unknown"; message: string; idempotencyKey: string }

export type RegisterFundsResult = Readonly<{
    fundsFactVersion: string
    subjectHash: string
    settledTotal: string
    invoicedTotal: string
    openTotal: string
    openInvoiceableTotal: string
    receiptFacts: readonly ReceiptSummary[]
    invoiceFacts: readonly InvoiceSummary[]
    replacementWorkItemId?: string
}>

export const REVIEW_TYPE_LABEL: Record<ReviewType, string> = {
    OPENING: "期初复核",
    SYNC_DELTA: "同步差额复核",
}

export const WORK_ITEM_TYPE_LABEL: Record<WorkItemType, string> = {
    CARD_FUNDS_REVIEW: "期初票款复核",
    CARD_FUNDS_DELTA_REVIEW: "差额票款复核",
}

export const REJECT_REASON_LABEL: Record<RejectReasonCode, string> = {
    EVIDENCE_INSUFFICIENT: "证据不足",
    FACTS_MISMATCH: "票款记录不一致",
    COUNTERPARTY_UNCLEAR: "往来主体不清",
    OTHER: "其他原因",
}

export const APPROVE_CONCLUSION_LABEL: Record<ApproveConclusion, string> = {
    NO_HISTORY_FROM_ZERO: "无历史票款，从 0 起",
    RECORDED_FACTS_RECONCILED: "已登记记录并核对一致",
}

/** 登记历史回款表单草稿。 */
export type ReceiptDraft = Readonly<{
    receiptNo: string
    receivedAt: string
    grossAmount: string
}>

/** 登记历史发票表单草稿。 */
export type InvoiceDraft = Readonly<{
    invoiceNo: string
    issuedAt: string
    grossAmount: string
    netAmount: string
    taxAmount: string
}>

/** 复核结论确认弹窗模式。 */
export type ConfirmMode =
    | { kind: "approve"; conclusion: ApproveConclusion; advance: boolean }
    | { kind: "zero"; advance: boolean }
    | { kind: "reject" }
    | null
