import type { StatusTone } from "@/components/ui/status-badge"

export type SalesOrderNature = "physical_service" | "card_voucher"
export type SalesOrderOwner = "erp" | "mall"

export type ProgressTrack = {
  label: string
  tone: StatusTone
}

export type SalesOrderLineItem = {
  id: string
  name: string
  sku?: string
  /** 数量，十进制字符串 */
  quantity: string
  unit: string
  /** 含税单价 */
  unitPriceGross: string
  /** 含税小计 */
  amountGross: string
  /** 卡券：面额 */
  faceValue?: string
  /** 卡券：配赠率展示，如 5.00 */
  giftRate?: string
  /** 卡券：电子卡 / 实体卡 */
  cardForm?: string
  /** 实物服务：履约方式 */
  fulfillmentMode?: string
  /** 明细履约期限（实物） */
  dueDate?: string
}

export type SalesOrderRelatedSummary = {
  purchaseOrders: number
  fulfillments: number
  receipts: number
  invoices: number
}

/** 关闭资格：仅履约完成 + 应收结清；开票不阻塞。 */
export type CloseEligibility = {
  fulfillmentComplete: boolean
  receivableSettled: boolean
  invoiceComplete: boolean
  /** 两项主条件均满足时由服务端自动关闭 */
  eligibleToClose: boolean
  blockers: string[]
  note: string
}

export type StructuredDiffItem = {
  field: string
  before: string
  after: string
}

export type ActionBlocker = {
  action: string
  reason: string
}

export type ProcurementRejectionResolution = {
  rejectedProcurementConfirmationId: string
  rejectedProcurementWorkItemId: string
  rejectedSubmissionId: string
  rejectedSubmissionNo: number
  rejectedSubjectHash: string
  rejectReasonCode: string
  rejectComment: string
  rejectedByLabel: string
  rejectedAt: string
  reviewStatus: "REJECTED" | "PENDING_LOW_MARGIN_MANAGER" | "RESOLVED" | "VOIDED"
  draftDifference: {
    changedItemOrService: boolean
    changedSalesPrice: boolean
    commercialTermsUnchanged: boolean
    diffSummary: StructuredDiffItem[]
  }
  estimatedCost?: string
  estimatedMarginPercent?: string
  lowMarginSubmission?: {
    submissionId: string
    submissionNo: number
    subjectHash: string
    acceptanceReason: string
    commercialTermsMatchRejectedSubmission: boolean
  }
  activeLowMarginManagerTask?: {
    workItemId: string
    workItemType: "LOW_MARGIN_MANAGER_CONFIRMATION"
    workItemStatus: "UNCLAIMED" | "CLAIMED" | "COMPLETED"
    claimedByLabel?: string
    leaseExpiresAt?: string
    subjectHash: string
    allowedActions: Array<"CLAIM" | "APPROVE" | "REJECT">
    actionBlockers: ActionBlocker[]
  }
  fixedResolutions: readonly [
    "RESUBMIT_CHANGED_TERMS",
    "REQUEST_LOW_MARGIN_ACCEPTANCE",
    "VOID_AFTER_REJECTION",
  ]
  allowedActions: Array<
    | "RESUBMIT_CHANGED_TERMS"
    | "REQUEST_LOW_MARGIN_ACCEPTANCE"
    | "VOID_AFTER_REJECTION"
  >
  actionBlockers: ActionBlocker[]
  /** 已解决时的正式结果摘要（会话内） */
  resolutionOutcome?: {
    outcome:
      | "CHANGED_TERMS_RESUBMITTED"
      | "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED"
      | "VOIDED_AFTER_PROCUREMENT_REJECTION"
      | "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
      | "LOW_MARGIN_REJECTED_TO_SALES"
    reference: string
    detail: string
    newSubmissionNo?: number
    newSubjectHash?: string
    newWorkItemId?: string
  }
}

export type CardSalesApproval = {
  workItemId: string
  workItemType: "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL"
  workItemStatus: "UNCLAIMED" | "CLAIMED" | "COMPLETED"
  subjectVersion: string
  subjectHash: string
  claimedByLabel?: string
  leaseExpiresAt?: string
  /** 冻结提交摘要（只读） */
  frozenSubmissionSummary: string
  expectedReviewStatus: "PENDING_SALES_LEAD" | "PENDING_OPERATIONS"
  allowedActions: Array<"CLAIM" | "APPROVE" | "REJECT">
  actionBlockers: ActionBlocker[]
}

export type SalesOrderRevisionSnapshot = {
  revisionNo: number
  effectiveAt: string
  /** 合同精确修订标签，如 HT-2026-0312@v3 */
  contractRevisionLabel: string
  /** 客户主数据快照（不被当前值覆盖） */
  customerSnapshot: string
  amountGross: string
  lineSummary: string
  changeOrderId?: string
  note: string
}

export type SalesChangeOrderSummary = {
  id: string
  statusLabel: string
  statusTone: StatusTone
  baseRevisionNo: number
  createdAt: string
  impactPath: "procurement" | "operations"
}

export type FormalAllowedAction =
  | "START_SALES_CHANGE"
  | "REGISTER_ACCEPTANCE"
  | "VIEW_CLOSE_CONDITIONS"
  | "PRINT"
  | "EXPORT"
  | "RESOLVE_PROCUREMENT_REJECTION"
  | "HANDLE_CARD_APPROVAL"

export type SalesOrderListItem = {
  id: string
  documentNumber: string
  customerName: string
  contractNumber: string
  /** 合同精确修订（快照），创建后随版本固定 */
  contractRevisionLabel: string
  nature: SalesOrderNature
  /** 创建来源：与当前主责分列 */
  originSystem: SalesOrderOwner
  /** 当前写入主责：任一时点唯一 */
  ownerSystem: SalesOrderOwner
  primaryStatus: { label: string; tone: StatusTone }
  fulfillment: ProgressTrack
  collection: ProgressTrack
  invoicing: ProgressTrack
  /** 含税成交金额 */
  amountGross: string
  /** 不含税金额 */
  amountNet: string
  /** 税额 */
  taxAmount: string
  /** 已回款（含税口径展示） */
  receivedAmount: string
  /** 已开票 */
  invoicedAmount: string
  ownerName: string
  submittedAt: string
  welfareScene: string
  remark?: string
  version: number
  lockVersion: number
  settlementEntity: string
  sellerEntity: string
  paymentTerms: string
  /** 表头履约期限（卡券全单；实物为摘要文案） */
  fulfillmentDeadline: string
  customerContact?: string
  lineItems: readonly SalesOrderLineItem[]
  related: SalesOrderRelatedSummary
  closeEligibility: CloseEligibility
  /** 业务性质创建后不可修改 */
  natureLocked: true
  /** 商城主责时商业字段只读 */
  commercialReadOnly: boolean
  commercialReadOnlyReason?: string
  revisions: readonly SalesOrderRevisionSnapshot[]
  procurementRejection?: ProcurementRejectionResolution | null
  activeCardSalesApproval?: CardSalesApproval | null
  activeChangeOrder?: SalesChangeOrderSummary | null
  allowedActions: FormalAllowedAction[]
  actionBlockers: ActionBlocker[]
}
