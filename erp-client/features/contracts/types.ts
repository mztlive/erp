import type { StatusTone } from "@/components/ui/status-badge"

/** 合同主状态：由服务端返回，前端不推导。 */
export type ContractStatus =
  | "EFFECTIVE"
  | "TERMINATED"
  | "EXPIRED"

export type ContractAction =
  | "UPLOAD_CONTRACT_PDF"
  | "PRINT"
  | "CREATE_SALES_ORDER"
  | "EXPORT"
  /** 预留能力：后端未下发该动作时前端不展示入口。 */
  | "TERMINATE"

type ActionBlocker = {
  action: string
  code: string
  message: string
}

type ObjectReference = {
  id: string
  displayName: string
  reference?: string
}

type PaymentTermView = {
  label: string
  days?: number
  description: string
}

type InvoiceRequirementView = {
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
  /** 安全检查状态仅作信息展示，不限制下载。 */
  securityState: "processing" | "done" | "quarantined"
  canDownload: boolean
}

type RelatedSalesOrderSummary = {
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

type AuditEventView = {
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
  allowedActions: ContractAction[]
  actionBlockers: ActionBlocker[]
  sourceAsOf: string
  relatedSalesOrdersAsOf: string
  queriedAt: string
  /** 是否可被新销售单选择器引用（服务端判定） */
  selectableForNewSalesOrder: boolean
  selectableBlocker?: string
}

export type UploadContractPdfInput = {
  pdfFile: File
  contractNo: string
  customerId?: string
  customerName: string
  settlementPartyName: string
  signedAt: string
  validFrom: string
  validTo: string
  paymentTerms: string
  idempotencyKey: string
}

export type UploadContractPdfResult = {
  contractId: string
  contractNo: string
  revisionId: string
  revisionNo: number
  uploadedAt: string
  fileName: string
  reference: string
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
  EFFECTIVE: "生效",
  TERMINATED: "终止",
  EXPIRED: "到期",
}

export const CONTRACT_STATUS_TONE: Record<ContractStatus, StatusTone> = {
  EFFECTIVE: "success",
  TERMINATED: "neutral",
  EXPIRED: "warning",
}

/** 负责人只显示姓名；归属语义（当前/历史参与人）不进入用户可见文案。 */
export function contractOwnerLabel(label: string): string {
  return label.split(" · ")[0] || label
}

/** 审计时间线动作 → 中文业务描述（枚举原值禁止上屏）。 */
export const CONTRACT_AUDIT_ACTION_LABEL: Record<string, string> = {
  UPLOAD_CONTRACT_PDF: "上传合同 PDF",
  DOWNLOAD_ATTACHMENT: "下载附件",
  CREATE_SALES_ORDER: "新建销售单",
  TERMINATE: "终止合同",
  EXPIRE: "到期结束",
}
