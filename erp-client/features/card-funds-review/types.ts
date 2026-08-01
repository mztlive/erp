/** W13 卡券票款复核 · 客户端契约类型（对齐工作面文档 §5/§8，session-mock 语义）。 */

export type ReviewType = "OPENING" | "SYNC_DELTA"

export type WorkItemType = "CARD_FUNDS_REVIEW" | "CARD_FUNDS_DELTA_REVIEW"

export type ReviewResult = "APPROVED" | "REJECTED"

export type ApproveConclusion =
  | "NO_HISTORY_FROM_ZERO"
  | "RECORDED_FACTS_RECONCILED"

export type RejectReasonCode =
  | "EVIDENCE_INSUFFICIENT"
  | "FACTS_MISMATCH"
  | "COUNTERPARTY_UNCLEAR"
  | "OTHER"

export type FundsReliability =
  | "VERIFIED"
  | "UNRELIABLE_PENDING_REVIEW"
  | "STALE_FINGERPRINT"

export type ReceiptSummary = Readonly<{
  receiptId: string
  receiptNo: string
  receivedAt: string
  grossAmount: string
  allocatedToAccount: string
  otherAllocationSummary?: string
  reversed: boolean
}>

export type InvoiceSummary = Readonly<{
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

export type ReviewHistoryItem = Readonly<{
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

export type DiffChange = Readonly<{
  id: string
  field: string
  before: string
  after: string
  note?: string
  sourceObject?: string
  occurredAt?: string
}>

export type ReviewDifference = Readonly<{
  title: string
  baselineReviewNo?: number
  baselineSubjectHash?: string
  changes: readonly DiffChange[]
  invalidatedAt?: string
}>

export type EvidenceDraft = Readonly<{
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
    workItemType: WorkItemType
    completionAction: string
    subjectVersion: string
    subjectHash: string
    workItemStatus: "PENDING" | "IN_PROGRESS" | "COMPLETED"
    dueAt?: string
    claimedBy?: { userId: string; displayName: string }
    leaseVersion?: number
    leaseExpiresAt?: string
    allowedActions: readonly (
      | "CLAIM"
      | "CONFIRM_ZERO"
      | "APPROVE"
      | "REJECT"
      | "HOLD"
      | "TRANSFER"
      | "REGISTER_RECEIPT"
      | "REGISTER_INVOICE"
    )[]
    actionBlockers: readonly {
      action: string
      code: string
      message: string
    }[]
    held?: boolean
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
  scope: "mine" | "role_pool"
  q?: string
  due?: "all" | "today" | "overdue"
  /** pending = 有效队列；held = 已暂挂子集；completed 不混入正常队列 */
  status?: "pending" | "held"
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

export type WorkItemLease = Readonly<{
  workItemId: string
  claimedByLabel: string
  expiresAt: string
  leaseVersion: number
  claimToken: string
}>

/** CompleteWorkItemEnvelope<CardFundsReviewDecision> 的 decision 段 */
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
      /** 完成时三方校验：任务信封 / 当前记录 / 提交期望 */
      expectedSubjectHash: string
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
      expectedSubjectHash: string
    }

export type CardFundsReviewBusinessResult = Readonly<{
  receivableFundsReviewId: string
  receivableAccountId: string
  reviewNo: number
  accountReviewStatus: string
  workflowActionId: string
  operationId: string
  completedAt: string
  reviewResult: ReviewResult
  conclusion: ApproveConclusion | "REJECTED"
  subjectHash: string
  reference: string
  followUpConfiguration?: {
    status: "BLOCKED"
    blockerCode: "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
    collaborationMessage: string
    requiredRegistration: readonly (
      | "WORK_ITEM_TYPE"
      | "OWNER_POOL"
      | "HANDLER_KEY"
    )[]
  }
}>

export type FormalOutcome =
  | {
      kind: "APPROVED"
      business: CardFundsReviewBusinessResult
    }
  | {
      kind: "REJECTED"
      business: CardFundsReviewBusinessResult
    }
  | {
      kind: "HELD"
      workItemId: string
      workItemStatus: "PENDING" | "IN_PROGRESS"
      heldAt: string
      resumeHint: string
      reference: string
      nextWorkItemId?: string
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

export const REJECT_FOLLOW_UP_COLLABORATION =
  "Q5 未决：驳回仅形成本次 REJECTED 复核记录并完成当前任务，不创建或转交后继任务。请人工与财务负责人协作登记固定 work_item_type / 责任池 / handlerKey 后再启用驳回后继。"
