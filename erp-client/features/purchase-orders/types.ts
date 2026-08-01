/** W08 采购单 · 客户端契约类型（对齐工作面文档 §5/§8，session-mock 语义）。 */

import type { StatusTone } from "@/components/ui/status-badge"

export type PurchaseType = "PHYSICAL" | "VIRTUAL" | "SERVICE"

export type FulfillmentResponsibility =
  | "WAREHOUSE"
  | "SUPPLIER_DIRECT"
  | "ELECTRONIC"
  | "SERVICE"

export type PurchaseOrderStatus =
  | "DRAFT"
  | "PENDING_REVIEW"
  | "EFFECTIVE"
  | "PARTIAL"
  | "COMPLETED"
  | "VOID"

export type PurchaseReviewStatus =
  | "NONE"
  | "PENDING"
  | "APPROVED"
  | "REJECTED"

export type PaymentGateState = "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"

export type ViewerRole = "procurement" | "finance" | "sales" | "warehouse"

export type ActionBlocker = {
  action: string
  code: string
  message: string
}

export type PurchaseOrderLineView = Readonly<{
  lineId: string
  lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
  procurementConfirmationLineId?: string
  itemName: string
  itemSku?: string
  quantity?: string
  unit?: string
  unitCostGross: string
  inputTaxRate: string
  /** 服务端按行舍入到分后的金额 */
  grossAmount: string
  netAmount: string
  taxAmount: string
  expectedDeliveryDate?: string
  logisticsFeeReason?: string
  salesAllocationLabel?: string
}>

export type PrepaymentGateView = Readonly<{
  state: PaymentGateState
  message: string
  required: string
  allocated: string
  gap: string
  updatedAt: string
}>

export type PayableSummaryView = Readonly<{
  payableOpenAmount: string
  paidAllocatedAmount: string
  purchaseInvoiceAllocatedAmount: string
}>

export type FulfillmentSummaryView = Readonly<{
  progressLabel: string
  progressTone: StatusTone
  inboundQty: string
  shippedQty: string
  remainingQty: string
  note?: string
}>

export type RelatedChangeView = Readonly<{
  changeId: string
  label: string
  statusLabel: string
  tone: StatusTone
  baseRevisionNo?: number
}>

export type WorkflowActionView = Readonly<{
  id: string
  actionLabel: string
  actorLabel: string
  at: string
  comment?: string
}>

export type PurchaseOrderListItem = Readonly<{
  purchaseOrderId: string
  purchaseNo?: string
  draftLabel?: string
  revisionNo?: number
  status: PurchaseOrderStatus
  statusLabel: string
  statusTone: StatusTone
  reviewStatus: PurchaseReviewStatus
  reviewLabel: string
  salesOrderId: string
  salesOrderNo: string
  supplierId: string
  supplierName: string
  purchaseType: PurchaseType
  fulfillmentResponsibility: FulfillmentResponsibility
  paymentTermCode: string
  paymentTermLabel: string
  ownerName: string
  /** 服务端汇总；无成本权限时为掩码标记 */
  grossAmount: string
  netAmount: string
  taxAmount: string
  costMasked: boolean
  paymentProgress: string
  invoiceProgress: string
  fulfillmentProgress: string
  paymentGate: PaymentGateState
  expectedDate?: string
  updatedAt: string
  allowedActions: readonly string[]
  actionBlockers: readonly ActionBlocker[]
}>

export type PurchaseOrderCenterView = Readonly<{
  identity: {
    purchaseOrderId: string
    purchaseNo?: string
    draftLabel?: string
    status: PurchaseOrderStatus
    statusLabel: string
    statusTone: StatusTone
    reviewStatus: PurchaseReviewStatus
    reviewLabel: string
    lockVersion: number
    currentSubmissionId?: string
    currentRevisionId?: string
    revisionNo?: number
    subjectHash?: string
  }
  header: {
    salesOrderId: string
    salesOrderNo: string
    supplierId: string
    supplierSnapshot: string
    purchaseType: PurchaseType
    fulfillmentResponsibility: FulfillmentResponsibility
    paymentTermCode: string
    paymentTermLabel: string
    ownerName: string
    submittedBy?: string
    submittedAt?: string
    expectedDate?: string
    creationBasisId?: string
  }
  progress: {
    payment: string
    invoice: string
    fulfillment: string
    prepaymentGate: PrepaymentGateView
  }
  currentContent: {
    source: "DRAFT" | "SUBMISSION" | "REVISION"
    version: number
    subjectHash?: string
    lines: readonly PurchaseOrderLineView[]
    totals: { gross: string; net: string; tax: string }
    costMasked: boolean
  }
  allocations: readonly {
    lineId: string
    salesOrderLineLabel: string
    allocatedQuantity: string
  }[]
  payableSummary?: PayableSummaryView
  fulfillmentSummary: FulfillmentSummaryView
  changes: readonly RelatedChangeView[]
  workflow: readonly WorkflowActionView[]
  allowedActions: readonly string[]
  actionBlockers: readonly ActionBlocker[]
  fieldVisibility: Record<string, "full" | "masked" | "hidden">
  /** 会话 mock 审核任务（仅待审核且存在提交时） */
  reviewWorkItem?: {
    workItemId: string
    subjectHash: string
    subjectVersion: string
    submittedBy: string
  }
}>

export type PurchaseCreationBasis = Readonly<{
  basisId: string
  salesOrderId: string
  salesOrderNo: string
  salesSubmissionId: string
  salesSubmissionNo: number
  supplierId: string
  supplierName: string
  purchaseType: PurchaseType
  fulfillmentResponsibility: FulfillmentResponsibility
  paymentTermCode: string
  paymentTermLabel: string
  /** 可拆入本单的已确认分行 */
  lines: readonly {
    procurementConfirmationLineId: string
    itemName: string
    itemSku?: string
    quantity: string
    unit: string
    unitCostGross: string
    inputTaxRate: string
    expectedDeliveryDate: string
    salesAllocationLabel: string
  }[]
  estimatedGross: string
  consumed: boolean
}>

export type SavePurchaseOrderDraftInput = {
  purchaseOrderId: string
  expectedLockVersion: number
  draftEditToken: string
  paymentTermCode: string
  lines: Array<{
    lineId: string
    lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
    quantity?: string
    unitCostGross?: string
    inputTaxRate: string
    logisticsFeeReason?: string
  }>
  /** demo: 模拟版本冲突 */
  simulateConflict?: boolean
  /** demo: 模拟结果未知 */
  simulateUnknown?: boolean
  idempotencyKey: string
}

export type SubmitPurchaseOrderInput = {
  purchaseOrderId: string
  expectedLockVersion: number
  expectedDraftContentHash: string
  draftEditToken: string
  idempotencyKey: string
  simulateUnknown?: boolean
}

export type ReviewPurchaseOrderInput = {
  purchaseOrderId: string
  submissionId: string
  workItemId: string
  expectedLockVersion: number
  reviewResult: "APPROVED" | "REJECTED"
  reasonCode?: string
  comment?: string
  idempotencyKey: string
  /** 诚实 mock：非完整 W02 envelope，仅保留关键字段 */
  claimToken?: string
  simulateUnknown?: boolean
}

export type CreatePurchaseOrderFromBasisInput = {
  basisId: string
  idempotencyKey: string
}

export type FormalActionResponse<T = unknown> =
  | { status: "succeeded"; data: T; reference: string }
  | { status: "failed"; message: string; code: string }
  | { status: "unknown"; message: string; idempotencyKey: string }

export const PURCHASE_TYPE_LABEL: Record<PurchaseType, string> = {
  PHYSICAL: "实物",
  VIRTUAL: "虚拟",
  SERVICE: "线下服务",
}

export const FULFILLMENT_RESPONSIBILITY_LABEL: Record<
  FulfillmentResponsibility,
  string
> = {
  WAREHOUSE: "入仓",
  SUPPLIER_DIRECT: "供应商直发",
  ELECTRONIC: "电子交付",
  SERVICE: "线下服务",
}

export const PO_STATUS_LABEL: Record<PurchaseOrderStatus, string> = {
  DRAFT: "草稿",
  PENDING_REVIEW: "待财务审核",
  EFFECTIVE: "已生效",
  PARTIAL: "部分执行",
  COMPLETED: "已完成",
  VOID: "已作废",
}

export const PO_STATUS_TONE: Record<PurchaseOrderStatus, StatusTone> = {
  DRAFT: "neutral",
  PENDING_REVIEW: "warning",
  EFFECTIVE: "success",
  PARTIAL: "info",
  COMPLETED: "success",
  VOID: "neutral",
}

export const REVIEW_STATUS_LABEL: Record<PurchaseReviewStatus, string> = {
  NONE: "—",
  PENDING: "待审核",
  APPROVED: "已通过",
  REJECTED: "已驳回",
}

export const REJECT_REASON_LABEL: Record<string, string> = {
  COST_TAX: "成本/税率不符",
  FEE: "费用行问题",
  PAYMENT_TERM: "付款条件问题",
  SUPPLIER: "供应商资料问题",
  ALLOCATION: "销售分配错误",
  OTHER: "其它",
}
