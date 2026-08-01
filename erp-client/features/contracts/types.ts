import type { StatusTone } from "@/components/ui/status-badge"

/** 合同主状态：由服务端返回，前端不推导。 */
export type ContractStatus =
  | "DRAFT"
  | "EFFECTIVE"
  | "TERMINATED"
  | "EXPIRED"

export type ContractAction =
  | "CREATE_CONTRACT"
  | "EDIT_DRAFT"
  | "ACTIVATE"
  | "REVISE"
  | "TERMINATE"
  | "UPLOAD_ATTACHMENT"
  | "PRINT"
  | "CREATE_SALES_ORDER"
  | "EXPORT"

export type ActionBlocker = {
  action: string
  code: string
  message: string
}

export type ObjectReference = {
  id: string
  displayName: string
  reference?: string
}

export type PaymentTermView = {
  label: string
  days?: number
  description: string
}

export type InvoiceRequirementView = {
  titleType: string
  taxIdMasked?: string
  contentSummary: string
  remark?: string
}

export type ContractAttachmentView = {
  id: string
  name: string
  contentType: string
  revisionNo?: number
  uploadedBy: string
  uploadedAt: string
  /** 安全检查状态：仅 done 允许下载演示 */
  securityState: "processing" | "done" | "quarantined"
  canDownload: boolean
}

export type RelatedSalesOrderSummary = {
  salesOrderId: string
  documentNumber: string
  natureLabel: string
  /** 销售单引用的合同修订号 */
  contractRevisionNo: number
  primaryStatus: { label: string; tone: StatusTone }
  amountGross: string
  fulfillmentLabel: string
  collectionLabel: string
  invoicingLabel: string
}

export type ContractRevisionSummary = {
  revisionId: string
  revisionNo: number
  validFrom: string
  validTo: string
  changeReason?: string
  effectiveAt?: string
  isCurrent: boolean
  /** 结构化 diff 摘要（历史版本） */
  diffSummary?: Array<{ field: string; before: string; after: string }>
}

export type AuditEventView = {
  id: string
  action: string
  actorLabel: string
  at: string
  summary: string
}

export type ContractListRow = {
  contractId: string
  contractNo: string
  customer: {
    customerId: string
    customerNo: string
    displayName: string
  }
  settlementParty: { partyId: string; displayName: string }
  status: ContractStatus
  statusLabel: string
  statusTone: StatusTone
  revisionNo: number
  signedAt?: string
  validFrom: string
  validTo: string
  /** 服务端标记：30 日内到期且仍生效 */
  expiringWithin30Days: boolean
  salesOrderCount: number
  activeSalesOrderCount: number
  ownerLabel: string
  ownerKind: "current_customer_owner" | "historical_participant"
  allowedActions: ContractAction[]
  actionBlockers: ActionBlocker[]
}

export type ContractCenterView = {
  contractId: string
  contractNo: string
  status: ContractStatus
  statusLabel: string
  statusTone: StatusTone
  lockVersion: number
  customer: ObjectReference
  ownerLabel: string
  ownerKind: "current_customer_owner" | "historical_participant"
  currentRevision: {
    revisionId: string
    revisionNo: number
    settlementParty: ObjectReference
    paymentTermSnapshot: PaymentTermView
    invoiceRequirementSnapshot: InvoiceRequirementView
    validFrom: string
    validTo: string
    signedAt?: string
    effectiveAt?: string
    termsSummary: string
  }
  attachments: ContractAttachmentView[]
  relatedSalesOrders: RelatedSalesOrderSummary[]
  revisionTimeline: ContractRevisionSummary[]
  auditTimeline: AuditEventView[]
  /**
   * 缺失时 fail-closed：allowedActions 不含 REVISE。
   * 前端不得自行选择修订模式。
   */
  contractRevisionPolicy?: {
    policyVersion: string
    mode: "DIRECT_REVISION" | "CHANGE_REQUEST"
    requiredEvidenceCodes: string[]
  }
  allowedActions: ContractAction[]
  actionBlockers: ActionBlocker[]
  sourceAsOf: string
  relatedSalesOrdersAsOf: string
  queriedAt: string
  /** 是否可被新销售单选择器引用（服务端判定） */
  selectableForNewSalesOrder: boolean
  selectableBlocker?: string
}

export type CreateContractDraftResult = {
  contractId: string
  contractNo: string
  revisionNo: number
  createdAt: string
  reference: string
}

export type ActivateContractResult = {
  contractId: string
  contractNo: string
  revisionNo: number
  effectiveAt: string
  reference: string
  nextStep: string
}

export type ReviseContractResult = {
  contractId: string
  contractNo: string
  workingRevisionNo: number
  baseRevisionNo: number
  createdAt: string
  reference: string
  nextStep: string
}

export type ContractExportJob = {
  jobId: string
  status: "queued" | "running" | "succeeded" | "failed"
  rowCount: number
  permissionVersion: string
  filterSnapshotLabel: string
  createdAt: string
  downloadLabel: string
}

export const CONTRACT_STATUS_LABEL: Record<ContractStatus, string> = {
  DRAFT: "草稿",
  EFFECTIVE: "生效",
  TERMINATED: "终止",
  EXPIRED: "到期",
}

export const CONTRACT_STATUS_TONE: Record<ContractStatus, StatusTone> = {
  DRAFT: "neutral",
  EFFECTIVE: "success",
  TERMINATED: "neutral",
  EXPIRED: "warning",
}
