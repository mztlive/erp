import type { StatusTone } from "@/components/ui/status-badge"

export type SalesOrderNature = "physical_service" | "card_voucher"
export type SalesOrderOrigin = "erp" | "mall"

export type SalesOrderCreateIntent = "SAVE_DRAFT" | "SUBMIT"

export type SalesOrderDraftLineInput = {
  rowKey: string
  name: string
  sku: string
  /** 公司商品池返回并由销售单锁定的精确 SKU 修订 ID。 */
  skuRevisionId: string
  quantity: string
  unit: string
  unitPriceGross: string
  fulfillmentMode: string
  dueDate: string
  faceValue: string
  giftRate: string
  cardForm: string
}

/** M5 建单仅引用已有有效合同的当前修订；新合同先经 W04 上传 Dialog 归档。 */
export type SalesOrderContractInput = {
  contractId: string
  requestedContractRevisionId: string
}

/** M5 建单输入；合同必须选择已有有效版本。 */
export type CreateSalesOrderInput = {
  contract: SalesOrderContractInput
  nature: SalesOrderNature
  /** 负责销售用户 id（当前登录用户）。 */
  ownerUserId: string
  /** 负责销售展示名。 */
  ownerName: string
  welfareScene: string
  paymentTerms: string
  fulfillmentDeadline: string
  taxRatePercent: string
  remark: string
  lineItems: SalesOrderDraftLineInput[]
  intent: SalesOrderCreateIntent
  idempotencyKey: string
}

export type CreateSalesOrderResult = {
  salesOrderId: string
  documentNumber: string
  statusLabel: string
  createdAt: string
  reference: string
  /** 工作副本乐观锁版本；草稿意图时用于后续 `saveSalesOrderDraft` 续接编辑。 */
  workingCopyVersion?: number
}

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
  reviewStatus: "REJECTED" | "RESOLVED" | "VOIDED"
  draftDifference: {
    changedItemOrService: boolean
    changedSalesPrice: boolean
    commercialTermsUnchanged: boolean
    diffSummary: StructuredDiffItem[]
  }
  estimatedCost?: string
  estimatedMarginPercent?: string
  fixedResolutions: readonly ["RESUBMIT_CHANGED_TERMS", "VOID_AFTER_REJECTION"]
  allowedActions: Array<"RESUBMIT_CHANGED_TERMS" | "VOID_AFTER_REJECTION">
  actionBlockers: ActionBlocker[]
  /** 已解决时的正式结果摘要（会话内） */
  resolutionOutcome?: {
    outcome: "CHANGED_TERMS_RESUBMITTED" | "VOIDED_AFTER_PROCUREMENT_REJECTION"
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
  /** 创建来源：商城（MALL）或本系统（ERP），创建后恒不变 */
  originSystem: SalesOrderOrigin
  /**
   * `code` 是服务端权威阶段码，与 `filter-orders.ts::SALES_ORDER_STATUS_OPTIONS` 对齐。
   * `ownerRole`/`ownerUserId`/`ownerUserName`/`dueAt` 是当前阶段命中的待办责任人与
   * 时限（审核轨在途时才有值，服务端整页批量解析，列表与详情共用同一来源）。
   */
  primaryStatus: {
    code: string
    label: string
    tone: StatusTone
    ownerRole?: string | null
    ownerUserId?: string | null
    ownerUserName?: string | null
    /** 预计完成时限（秒级时间戳）。 */
    dueAt?: number | null
  }
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
  /** 商城开单（origin=mall）同步期间商业字段只读 */
  commercialReadOnly: boolean
  commercialReadOnlyReason?: string
  revisions: readonly SalesOrderRevisionSnapshot[]
  procurementRejection?: ProcurementRejectionResolution | null
  activeCardSalesApproval?: CardSalesApproval | null
  activeChangeOrder?: SalesChangeOrderSummary | null
  allowedActions: FormalAllowedAction[]
  actionBlockers: ActionBlocker[]
}
