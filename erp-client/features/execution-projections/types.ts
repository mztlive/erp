/**
 * W23 销售单执行投影 · 客户端契约（对齐 docs/ui-workspaces/w23-execution-projection.md §5/§8）。
 * 投影不是第二张销售单；字段仅含商城执行白名单，不含成交金额/配赠/税率/开票/应收/玩法。
 */

import type { StatusTone } from "@/components/ui/status-badge"

/** 商城接收 / 投递状态 */
export type DeliveryStatus =
  | "PENDING"
  | "SENDING"
  | "RETRYING"
  | "ACKED"
  | "FAILED"
  | "ESCALATED_MANUAL"
  | "UNKNOWN"

export type ProjectionSource = "MIGRATION_BASELINE" | "ERP_SALES_REVISION"

export type LatencyBand = "normal" | "near_sla" | "over_sla"

export type ReconciliationStatus = "MATCHED" | "VERSION_MISMATCH" | "NONE"

type DeliveryCommandResultCode =
  | "ACKED"
  | "FAILED"
  | "STILL_UNKNOWN"
  | "RETRY_SCHEDULED"
  | "ESCALATED"

export const DELIVERY_STATUS_LABEL: Record<DeliveryStatus, string> = {
  PENDING: "待发送",
  SENDING: "发送中",
  RETRYING: "重试中",
  ACKED: "已确认",
  FAILED: "失败",
  ESCALATED_MANUAL: "转人工",
  UNKNOWN: "结果未知",
}

export const DELIVERY_STATUS_TONE: Record<DeliveryStatus, StatusTone> = {
  PENDING: "neutral",
  SENDING: "info",
  RETRYING: "warning",
  ACKED: "success",
  FAILED: "destructive",
  ESCALATED_MANUAL: "warning",
  UNKNOWN: "warning",
}

export const SOURCE_LABEL: Record<ProjectionSource, string> = {
  MIGRATION_BASELINE: "迁移基线",
  ERP_SALES_REVISION: "ERP 销售版本",
}

export const LATENCY_LABEL: Record<LatencyBand, string> = {
  normal: "正常",
  near_sla: "接近超时",
  over_sla: "已超时",
}

export const RECONCILIATION_LABEL: Record<ReconciliationStatus, string> = {
  MATCHED: "版本一致",
  VERSION_MISMATCH: "版本差异",
  NONE: "无对账",
}

type ActionBlocker = {
  action: string
  code: string
  message: string
}

/** §5.2 服务端白名单执行字段；前端不得增补商业字段 */
export type ProjectionWhitelistContent = {
  customerExternalIdentity: string
  /** 短引用；策略未配置时不可复制完整值 */
  customerExternalIdentityCopyable: boolean
  voucherCategoryExternalIdentity: string
  voucherCategoryErpName: string
  voucherExpiryAt: string
  faceValue: string
  cardCount: string
  cardForm: string
  effectiveAt: string
  contentHash: string
}

type ExecutionProjectionDelivery = {
  deliveryId: string
  status: DeliveryStatus
  statusLabel: string
  statusTone: StatusTone
  attemptCount: number
  lastAttemptAt?: string
  nextAttemptAt?: string
  mallAckAt?: string
  mallExecutionBaseline?: string
  errorCode?: string
  errorSummary?: string
  workItemId?: string
  errorTaskId?: string
}

export type ExecutionProjectionRow = {
  projectionId: string
  projectionNo: string
  projectionRevisionId: string
  projectionRevisionNo: number
  projectionSource: ProjectionSource
  salesOrderId: string
  salesOrderNo: string
  salesOrderRevisionId: string
  salesOrderRevisionNo: number
  salesOrderStatus: string
  salesOrderStatusTone: StatusTone
  customerLabel: string
  targetMallId: string
  targetMallName: string
  currentAckedRevisionNo?: number
  delivery: ExecutionProjectionDelivery
  latencyBand: LatencyBand
  reconciliationStatus: ReconciliationStatus
  pendingDurationLabel: string
  ownerLabel: string
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  objectVersion: string
  /** 列表预览用的白名单摘要（仍不重组装） */
  whitelistPreview: Pick<
    ProjectionWhitelistContent,
    | "voucherCategoryErpName"
    | "faceValue"
    | "cardCount"
    | "cardForm"
    | "voucherExpiryAt"
  >
}

export type ExecutionProjectionMetricKey =
  | "pending_send"
  | "inflight"
  | "timeout"
  | "fail_manual"
  | "acked"

export type ExecutionProjectionMetric = {
  key: ExecutionProjectionMetricKey
  label: string
  value: number
  detail?: string
}

export type ExecutionProjectionListQuery = {
  q?: string
  mallId?: string
  /** 逗号分隔或单值；结果未知/失败/转人工等 */
  deliveryStatus?: string
  source?: ProjectionSource | "all"
  latency?: LatencyBand | "all"
  reconciliation?: ReconciliationStatus | "all"
  metric?: ExecutionProjectionMetricKey | "all"
  page?: number
  pageSize?: number
}

export type ExecutionProjectionListResult = {
  rows: ExecutionProjectionRow[]
  pageInfo: { page: number; pageSize: number; total: number }
  metrics: ExecutionProjectionMetric[]
  malls: Array<{ id: string; name: string }>
  permissionVersion: string
  sourceFactsAsOf: string
  projectionUpdatedAt: string
  deliveryStatusUpdatedAt: string
  queriedAt: string
  filterSummary: string
  /** 默认运营关注：未确认；销售从 W05 进入可不套此默认 */
  defaultViewNote: string
}

type RevisionLink = {
  salesOrderRevisionId: string
  salesOrderRevisionNo: number
  projectionRevisionId: string
  projectionRevisionNo: number
  deliveryStatus: DeliveryStatus
  deliveryStatusLabel: string
  mallAckAt?: string
  /** 历史投影固定来源销售版本，不被销售单当前版覆盖 */
  sourceSalesRevisionNo: number
  isCurrentSelection: boolean
}

export type ExecutionProjectionView = {
  identity: {
    projectionId: string
    projectionNo: string
    salesOrderId: string
    salesOrderNo: string
    targetMallId: string
    targetMallName: string
  }
  tracks: {
    salesFact: { label: string; tone: StatusTone; description: string }
    projectionDelivery: {
      label: string
      tone: StatusTone
      description: string
    }
    mallConfirm: { label: string; tone: StatusTone; description: string }
  }
  selectedRevision: {
    projectionRevisionId: string
    revisionNo: number
    projectionSource: ProjectionSource
    salesOrderRevisionId: string
    salesOrderRevisionNo: number
    content: ProjectionWhitelistContent
  }
  currentAckedRevisionNo?: number
  revisionLinks: RevisionLink[]
  deliveries: ExecutionProjectionDelivery[]
  salesOrderStatus: string
  salesOrderStatusTone: StatusTone
  ownerLabel: string
  pendingDurationLabel: string
  latencyBand: LatencyBand
  reconciliationStatus: ReconciliationStatus
  allowedActions: string[]
  actionBlockers: ActionBlocker[]
  fieldPermissions: Record<string, "full" | "masked" | "hidden">
  objectVersion: string
  sourceFactsAsOf: string
  projectionUpdatedAt: string
  deliveryStatusUpdatedAt: string
  queriedAt: string
  boundaryNotice: string
}

/** W05 协同子区只读摘要（无需先开 W23） */
export type SalesOrderCollaborationSummary = {
  salesOrderId: string
  salesOrderNo: string
  hasProjection: boolean
  projectionId?: string
  projectionNo?: string
  salesOrderRevisionNo?: number
  projectionRevisionNo?: number
  targetMallName?: string
  tracks?: ExecutionProjectionView["tracks"]
  delivery?: ExecutionProjectionDelivery
  whitelistPreview?: ExecutionProjectionRow["whitelistPreview"]
  currentAckedRevisionNo?: number
  reconciliationStatus?: ReconciliationStatus
  historyCount: number
  w23Href?: string
  historyHref?: string
  note: string
}

export type ProjectionDeliveryCommandResult = {
  operationId: string
  deliveryId: string
  projectionId: string
  salesOrderNo: string
  result: DeliveryCommandResultCode
  resultLabel: string
  workItemId?: string
  errorTaskId?: string
  occurredAt: string
  nextAction: string
  /** 结果未知时 true：前端不得标成功 / 跳过 / 计入已确认 */
  stillUnknown: boolean
  objectVersion: string
}

export type BulkItemOutcome = {
  projectionId: string
  salesOrderNo: string
  deliveryId: string
  outcome: "succeeded" | "skipped" | "failed" | "still_unknown"
  reason: string
}

export type BulkProjectionJob = {
  jobId: string
  action: "BULK_QUERY" | "BULK_RETRY"
  status: "queued" | "running" | "succeeded" | "partial" | "failed"
  total: number
  completed: number
  succeeded: number
  skipped: number
  failed: number
  stillUnknown: number
  selectionSnapshotId: string
  items: BulkItemOutcome[]
  startedAt: string
  finishedAt?: string
  nextAction: string
}
