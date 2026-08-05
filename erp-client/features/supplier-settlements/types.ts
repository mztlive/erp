/**
 * W27 · API 供应商结算 · 客户端契约
 * 对齐 docs/ui-workspaces/w27-api-settlement.md §5/§7/§8
 */

import type { StatusTone } from "@/components/ui/status-badge"

export type DemoRole =
  | "finance_prep"
  | "finance_review"
  | "procurement"
  | "manager"

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
export const RESOLUTION_TO_STATUS: Record<
  DifferenceResolution,
  DifferenceStatus
> = {
  SUPPLIER_ACCEPTED: "SUPPLIER_ACCEPTED",
  ERP_ACCEPTED: "ERP_ACCEPTED",
  COMPENSATED: "COMPENSATED",
  CLOSED_NO_ADJUSTMENT: "CLOSED",
}

export type SettlementSection =
  | "overview"
  | "items"
  | "differences"
  | "review"
  | "payable"
  | "audit"

export type EmptyReason =
  | "NO_PERMISSION"
  | "NO_SCOPE"
  | "NO_STATEMENTS"
  | "FILTER_NO_RESULT"

export type ActionBlocker = {
  action: string
  code: string
  message: string
  destinationWorkspaceId?: string
}

export type ActorView = {
  userId: string
  displayName: string
}

export type PeriodPolicyView =
  | {
      state: "CONFIGURED"
      policyId: string
      policyVersion: string
      timezone: string
      selectablePeriods: Array<{
        periodStart: string
        periodEnd: string
        label: string
      }>
    }
  | {
      state: "UNCONFIGURED"
      blocker: ActionBlocker
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

export type SettlementListTotals = {
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
  periodPolicy: PeriodPolicyView
  suppliers: Array<{ supplierId: string; supplierName: string }>
  emptyReason?: EmptyReason
  hasModulePermission: boolean
  hasDataScope: boolean
  viewerRole: DemoRole
  viewerRoleLabel: string
  viewerUserId: string
  permissionVersion: string
  sourceAsOf: string
  queriedAt: string
  filterSummary: string
}

export type SettlementItemView = {
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
  supplierBillLineGross?: string
  /** 只读：页面不得改写 */
  readOnly: true
}

export type DifferenceEvidence = {
  evidenceId: string
  kind: "PROCUREMENT_OPINION" | "SUPPLIER_CONFIRM" | "TICKET" | "ATTACHMENT"
  label: string
  comment?: string
  by: ActorView
  at: string
}

export type DifferenceResolutionRecord = {
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
  leftFields: Array<{ id: string; field: string; before: string; after: string; note?: string }>
}

export type ReviewRecordView = {
  recordId: string
  action: "SUBMIT" | "REJECT" | "CONFIRM"
  actionLabel: string
  by: ActorView
  at: string
  comment?: string
  reasonCode?: string
}

export type AuditEventView = {
  eventId: string
  at: string
  actor: string
  action: string
  summary: string
  auditNo?: string
}

export type SettlementTotalsView = {
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

export type PayableLinkView = {
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
    sourceSnapshotHash: string
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
    workItemType: "SUPPLIER_SETTLEMENT_REVIEW"
    subjectVersion: string
    subjectHash: string
    claimedBy?: ActorView
  }
  periodPolicy: PeriodPolicyView
  auditEvents: AuditEventView[]
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  freshness: {
    immutableFactsAsOf: string
    externalBillAsOf?: string
    w26ProjectionUpdatedAt?: string
    queriedAt: string
  }
  viewerRole: DemoRole
  viewerRoleLabel: string
  viewerUserId: string
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
  periodPolicyId: string
  expectedPeriodPolicyVersion: string
  role: DemoRole
  requestId: string
  idempotencyKey: string
}

export type RefreshDraftInput = {
  statementId: string
  expectedLockVersion: number
  expectedSourceSnapshotHash: string
  role: DemoRole
  requestId: string
  idempotencyKey: string
}

export type AppendEvidenceInput = {
  statementId: string
  differenceId: string
  expectedDifferenceVersion: number
  opinionCode?: string
  comment?: string
  role: DemoRole
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
  role: DemoRole
  operationId: string
  idempotencyKey: string
}

export type SubmitReviewInput = {
  statementId: string
  expectedLockVersion: number
  subjectHash: string
  role: DemoRole
  operationId: string
  idempotencyKey: string
  comment?: string
}

export type ReviewDecisionInput = {
  statementId: string
  workItemId: string
  expectedSubjectVersion: string
  expectedLockVersion: number
  action: "REJECT" | "CONFIRM"
  role: DemoRole
  operationId: string
  idempotencyKey: string
  reasonCode?: string
  comment?: string
  forceUnknown?: boolean
}

export const DEMO_ROLE_LABEL: Record<DemoRole, string> = {
  finance_prep: "财务经办",
  finance_review: "财务复核",
  procurement: "采购",
  manager: "管理层（只读）",
}

export const STATUS_LABEL: Record<SettlementStatus, string> = {
  DRAFT: "草稿",
  PENDING_RECONCILE: "待对账",
  HAS_DIFFERENCE: "有差异",
  PENDING_REVIEW: "待复核",
  CONFIRMED: "已确认",
  VOIDED: "已作废",
}

export const STATUS_TONE: Record<SettlementStatus, StatusTone> = {
  DRAFT: "neutral",
  PENDING_RECONCILE: "info",
  HAS_DIFFERENCE: "warning",
  PENDING_REVIEW: "warning",
  CONFIRMED: "success",
  VOIDED: "neutral",
}

export const VIEW_LABEL: Record<SettlementView, string> = {
  pending: "待处理",
  prepared_by_me: "我经办",
  review_by_me: "我复核",
  confirmed: "已确认",
}

export const DIFF_TYPE_LABEL: Record<DifferenceType, string> = {
  MISSING_ORDER: "漏单",
  DUPLICATE: "重复",
  AMOUNT: "金额差异",
  REFUND: "退款差异",
  STATUS: "状态差异",
}

export const DIFF_STATUS_LABEL: Record<DifferenceStatus, string> = {
  PENDING: "待处理",
  SUPPLIER_ACCEPTED: "供应商认可",
  ERP_ACCEPTED: "ERP 认可",
  COMPENSATED: "已补偿",
  CLOSED: "关闭",
}

export const RESOLUTION_LABEL: Record<DifferenceResolution, string> = {
  SUPPLIER_ACCEPTED: "供应商认可",
  ERP_ACCEPTED: "ERP 认可",
  COMPENSATED: "已补偿",
  CLOSED_NO_ADJUSTMENT: "关闭（无需调整）",
}

export const SECTION_LABEL: Record<SettlementSection, string> = {
  overview: "概览",
  items: "结算明细",
  differences: "差异处理",
  review: "复核记录",
  payable: "应付与票款",
  audit: "审计",
}

export const SECTIONS: SettlementSection[] = [
  "overview",
  "items",
  "differences",
  "review",
  "payable",
  "audit",
]

/** 审计动作中文映射（动作码只在代码与审计数据结构中使用） */
export const AUDIT_ACTION_LABEL: Record<string, string> = {
  CREATE_DRAFT: "创建结算草稿",
  REFRESH_TRIAL: "刷新试算",
  RESOLVE_DIFFERENCE: "登记差异结论",
  APPEND_EVIDENCE: "追加采购证据",
  SUBMIT_REVIEW: "提交复核",
  CONFIRM: "确认结算",
  REJECT: "驳回复核",
}

/** 复核/结论原因码中文映射（原因码原值不上屏） */
export const REASON_CODE_LABEL: Record<string, string> = {
  NEEDS_MORE_EVIDENCE: "证据不足",
  AMOUNT_MISMATCH: "金额仍不一致",
  OTHER: "其他",
  BILL_ALIGNED: "账单已对齐",
  ACCEPT_BILL: "接受供应商账单",
  NO_BUSINESS_IMPACT: "无需业务调整",
  COMPENSATED_ELSEWHERE: "已另行补偿",
}

export const ACTORS = {
  prep: { userId: "u_finance_prep", displayName: "李经办" },
  review: { userId: "u_finance_review", displayName: "王复核" },
  procurement: { userId: "u_procurement", displayName: "赵采购" },
  manager: { userId: "u_manager", displayName: "陈总" },
} as const

export function roleToUserId(role: DemoRole): string {
  switch (role) {
    case "finance_prep":
      return ACTORS.prep.userId
    case "finance_review":
      return ACTORS.review.userId
    case "procurement":
      return ACTORS.procurement.userId
    case "manager":
      return ACTORS.manager.userId
  }
}

export function roleToActor(role: DemoRole): ActorView {
  switch (role) {
    case "finance_prep":
      return { ...ACTORS.prep }
    case "finance_review":
      return { ...ACTORS.review }
    case "procurement":
      return { ...ACTORS.procurement }
    case "manager":
      return { ...ACTORS.manager }
  }
}
