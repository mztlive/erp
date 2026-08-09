/** W07 采购二次确认 · 客户端契约类型（对齐工作面文档 §8）。 */

export type FulfillmentMode =
  | "WAREHOUSE"
  | "SUPPLIER_DIRECT"
  | "ELECTRONIC"
  | "SERVICE"

export type RejectReasonCode =
  | "UNFULFILLABLE"
  | "COST_INCREASE"
  | "DELIVERY_UNMET"
  | "QUALIFICATION_INVALID"

export type SubmissionOrigin = "INITIAL" | "CHANGED_TERMS_AFTER_REJECTION"

export type ConfirmationLineDraft = Readonly<{
  lineKey: string
  submissionLineId: string
  supplierId: string
  supplierName: string
  offeringRevisionId: string
  confirmedQuantity: string
  latestCostGross: string
  inputTaxRate: string
  expectedDeliveryDate: string
  fulfillmentMode: FulfillmentMode
  capabilityRevisionId: string
  capabilitySummary: string
  qualificationStatus: "VALID" | "INVALID" | "EXPIRING"
}>

export type SubmissionLineView = Readonly<{
  submissionLineId: string
  itemName: string
  itemSku: string
  specification?: string
  committedQuantity: string
  unit: string
  requestedDeliveryDate: string
  unitPriceGross?: string
  fulfillmentMode?: FulfillmentMode
  salesTaxRate?: string
  referenceSupplier?: string
  referenceCost?: string
  salesAmountGross: string
}>

export type CoverageByLine = Readonly<{
  submissionLineId: string
  itemName: string
  confirmed: string
  required: string
  complete: boolean
  gap: string
}>

export type BlockingIssue = Readonly<{
  code: string
  message: string
  lineId?: string
}>

export type ProcurementConfirmationTask = Readonly<{
  workItemId: string
  /** Server-filterable responsibility boundary for mine vs claimable role pool. */
  responsibilityScope: "mine" | "role_pool"
  status: "PENDING" | "IN_PROGRESS" | "COMPLETED"
  priority: number
  dueAt: string
  impactSummary: string
  subjectVersion: string
  subjectHash: string
  held?: boolean
  lease?: {
    claimedByLabel: string
  }
  salesSubmission: {
    salesOrderId: string
    salesOrderNo: string
    submissionId: string
    submissionNo: number
    subjectHash: string
    subjectHashSummary: string
    submittedAt: string
    submittedByLabel: string
    customerSnapshot: string
    contractId?: string
    contractSnapshot?: string
    settlementPartySnapshot?: string
    paymentTermLabel: string
    projectName?: string
    businessRemark?: string
    grossAmount: string
    netAmount?: string
    taxAmount?: string
    origin: SubmissionOrigin
    lines: readonly SubmissionLineView[]
    resubmissionContext?: {
      origin: Exclude<SubmissionOrigin, "INITIAL">
      previousRejectedConfirmationId: string
      previousRejectedSubmissionId: string
      previousRejectedSubjectHash: string
    }
  }
  confirmation: {
    confirmationId: string
    status: "PENDING" | "APPROVED" | "REJECTED"
    editVersion: number
    lines: readonly ConfirmationLineDraft[]
  }
  decisionSummary: {
    coverageByLine: readonly CoverageByLine[]
    estimatedPurchaseGross: string
    estimatedMargin?: string
    marginDelta?: string
    blockingIssues: readonly BlockingIssue[]
    warnings: readonly BlockingIssue[]
  }
  allowedActions: readonly string[]
  actionBlockers: readonly { action: string; code: string; message: string }[]
  riskLabel: string
  riskTone: "warning" | "destructive" | "success" | "info" | "neutral"
  riskDescription: string
}>

export type ProcurementQueueView = Readonly<{
  preferences: {
    autoNextDefault: boolean
    /** 未配置时前端不得写本地或服务端持久偏好 */
    preferenceScope?: "DEVICE" | "USER"
  }
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
  /** 当前筛选下的有效任务（不含已完成/已驳回终局项；暂挂仍在） */
  tasks: readonly ProcurementConfirmationTask[]
  current?: ProcurementConfirmationTask
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
}>

export type WorkItemLease = Readonly<{
  workItemId: string
  claimedByLabel: string
}>

export type ProcurementRecommendation = Readonly<{
  confirmationId: string
  policyVersion: string
  calculatedAt: string
  ready: boolean
  lines: readonly (ConfirmationLineDraft & {
    itemName: string
    itemSku: string
    landedGross: string
    freightAmount?: string
    serviceFeeAmount?: string
    recommendationReason: string
  })[]
  purchaseOrders: readonly {
    supplierId: string
    supplierName: string
    fulfillmentMode: FulfillmentMode
    lineCount: number
    estimatedGross: string
  }[]
  estimatedPurchaseGross: string
  salesGross: string
  estimatedGrossMargin: string
  blockingIssues: readonly BlockingIssue[]
  warnings: readonly BlockingIssue[]
}>

export type FormalOutcome =
  | {
      kind: "APPROVED_AND_SALES_EFFECTIVE"
      procurementConfirmationId: string
      salesOrderId: string
      salesOrderNo: string
      submissionId: string
      subjectHash: string
      salesOrderRevisionId: string
      receivableAccountId: string
      purchaseOrders: readonly {
        purchaseOrderId: string
        purchaseNo: string
      }[]
      reference: string
    }
  | {
      kind: "REJECTED_TO_SALES"
      procurementConfirmationId: string
      salesOrderId: string
      salesOrderNo: string
      rejectedSubmissionId: string
      rejectedSubjectHash: string
      workflowActionId: string
      nextSalesResolutions: readonly ["RESUBMIT_CHANGED_TERMS", "VOID_AFTER_REJECTION"]
      /** 契约：驳回事务不得创建后继任务 */
      successorWorkItemId?: never
      reference: string
      rejectReasonCode: RejectReasonCode
      comment: string
    }
  | {
      kind: "DEFERRED"
      workItemId: string
      workItemStatus: "PENDING" | "IN_PROGRESS"
      leaseDisposition: "RETAINED" | "RELEASED"
      nextWorkItemId?: string
      reference: string
    }

export type FormalActionResponse =
  | { status: "succeeded"; outcome: FormalOutcome }
  | { status: "failed"; message: string; code: string }

export const FULFILLMENT_MODE_LABEL: Record<FulfillmentMode, string> = {
  WAREHOUSE: "入仓",
  SUPPLIER_DIRECT: "供应商直发",
  ELECTRONIC: "电子交付",
  SERVICE: "线下服务",
}

export const REJECT_REASON_LABEL: Record<RejectReasonCode, string> = {
  UNFULFILLABLE: "无法履约",
  COST_INCREASE: "成本上涨",
  DELIVERY_UNMET: "交期不满足",
  QUALIFICATION_INVALID: "资质失效",
}

export const NEXT_SALES_RESOLUTION_COPY = [
  {
    code: "RESUBMIT_CHANGED_TERMS" as const,
    title: "改品 / 改价后重提",
    description:
      "销售与客户重新确认后，在销售单冻结新提交与新数据版本，再产生全新采购确认任务。",
  },
  {
    code: "VOID_AFTER_REJECTION" as const,
    title: "不做则作废",
    description: "由销售在销售单中原子作废生效前订单，不创建后继任务。",
  },
] as const
