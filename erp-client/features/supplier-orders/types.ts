/**
 * W26 · 供应商订单 · 客户端契约类型
 * 对齐 docs/ui-workspaces/w26-supplier-orders.md §5/§7/§8
 *
 * 本文件刻意保持单文件形态：内容全部是类型声明与声明式数据表
 * （枚举 → 中文标签/色调映射、状态数组），无可拆分的运行时逻辑。
 */

import type { StatusTone } from "@/components/ui/status-badge"
import type {
    WorkItemAllowedAction,
    WorkItemProcessingState,
    WorkItemStatus,
} from "@/features/work-items/types"

/** 履约主状态：九个正式枚举，不得把取消/退款折入 */
export type SupplierFulfillmentStatus =
    | "RECEIVED"
    | "SUBMITTING"
    | "ACCEPTED"
    | "REJECTED"
    | "RESULT_UNKNOWN"
    | "FULFILLING"
    | "SHIPPED"
    | "COMPLETED"
    | "EXCEPTION"

export type CancelStatus =
    | "NONE"
    | "CANCEL_PENDING"
    | "CANCELED"
    | "FAILED"
    | "MANUAL"

export type RefundStatus =
    | "NONE"
    | "REFUND_PENDING"
    | "PARTIAL"
    | "REFUNDED"
    | "REFUND_FAILED"
    | "MANUAL"

export type ListView = "actionable" | "all" | "recent_completed"

export type OrderSection =
    | "overview"
    | "items"
    | "fulfillment"
    | "aftersales"
    | "costs"
    | "audit"

type InvestigationOutcome =
    | "VERIFIED_TERMINAL"
    | "VERIFIED_NO_RESULT"
    | "RESULT_UNKNOWN"

type ActionBlocker = {
    action: string
    code: string
    message: string
    destinationWorkspaceId?: string
}

export const FULFILLMENT_STATUS_LABEL: Record<
    SupplierFulfillmentStatus,
    string
> = {
    RECEIVED: "已接收",
    SUBMITTING: "提交中",
    ACCEPTED: "已接单",
    REJECTED: "明确拒绝",
    RESULT_UNKNOWN: "结果未知",
    FULFILLING: "履约中",
    SHIPPED: "已发货",
    COMPLETED: "已完成",
    EXCEPTION: "异常",
}

export const FULFILLMENT_STATUS_TONE: Record<
    SupplierFulfillmentStatus,
    StatusTone
> = {
    RECEIVED: "neutral",
    SUBMITTING: "info",
    ACCEPTED: "info",
    REJECTED: "destructive",
    RESULT_UNKNOWN: "warning",
    FULFILLING: "info",
    SHIPPED: "info",
    COMPLETED: "success",
    EXCEPTION: "destructive",
}

export const CANCEL_STATUS_LABEL: Record<CancelStatus, string> = {
    NONE: "未发起",
    CANCEL_PENDING: "处理中",
    CANCELED: "已取消",
    FAILED: "失败",
    MANUAL: "待人工",
}

export const CANCEL_STATUS_TONE: Record<CancelStatus, StatusTone> = {
    NONE: "neutral",
    CANCEL_PENDING: "info",
    CANCELED: "success",
    FAILED: "destructive",
    MANUAL: "warning",
}

export const REFUND_STATUS_LABEL: Record<RefundStatus, string> = {
    NONE: "未发起",
    REFUND_PENDING: "处理中",
    PARTIAL: "部分",
    REFUNDED: "全部",
    REFUND_FAILED: "失败",
    MANUAL: "待人工",
}

export const REFUND_STATUS_TONE: Record<RefundStatus, StatusTone> = {
    NONE: "neutral",
    REFUND_PENDING: "info",
    PARTIAL: "info",
    REFUNDED: "success",
    REFUND_FAILED: "destructive",
    MANUAL: "warning",
}

export const VIEW_LABEL: Record<ListView, string> = {
    actionable: "可操作",
    all: "全部",
    recent_completed: "最近完成",
}

export const SECTION_LABEL: Record<OrderSection, string> = {
    overview: "概览",
    items: "商品明细",
    fulfillment: "履约与物流",
    aftersales: "售后",
    costs: "成本与结算",
    audit: "动作与审计",
}

/** 关联任务类型中文映射（任务类型码不在界面出现） */
export const WORK_ITEM_TYPE_LABEL: Record<
    "INTEGRATION_RESULT_UNKNOWN" | "BUSINESS_EXCEPTION",
    string
> = {
    INTEGRATION_RESULT_UNKNOWN: "接口结果待确认",
    BUSINESS_EXCEPTION: "业务异常",
}

/** 任务状态中文映射（枚举原值不上屏） */
export const WORK_ITEM_STATUS_LABEL: Record<WorkItemStatus, string> = {
    OPEN: "待处理",
    COMPLETED: "已完成",
    CLOSED: "已关闭",
}

/** 版本代号（SV-/PV- 前缀）转业务口径，如 SV-12 → 12 */
export function codeVersion(value?: string): string {
    if (!value) return "—"
    return value.replace(/^[A-Z]+-/, "")
}

export const SECTIONS: OrderSection[] = [
    "overview",
    "items",
    "fulfillment",
    "aftersales",
    "costs",
    "audit",
]

export const FULFILLMENT_STATUSES: SupplierFulfillmentStatus[] = [
    "RECEIVED",
    "SUBMITTING",
    "ACCEPTED",
    "REJECTED",
    "RESULT_UNKNOWN",
    "FULFILLING",
    "SHIPPED",
    "COMPLETED",
    "EXCEPTION",
]

export const CANCEL_STATUSES: CancelStatus[] = [
    "NONE",
    "CANCEL_PENDING",
    "CANCELED",
    "FAILED",
    "MANUAL",
]

export const REFUND_STATUSES: RefundStatus[] = [
    "NONE",
    "REFUND_PENDING",
    "PARTIAL",
    "REFUNDED",
    "REFUND_FAILED",
    "MANUAL",
]

export type SupplierOrderListQuery = {
    view: ListView
    q?: string
    supplierId?: string
    fulfillmentStatuses?: SupplierFulfillmentStatus[]
    cancelStatuses?: CancelStatus[]
    refundStatuses?: RefundStatus[]
    paidFrom?: string
    paidTo?: string
    page: number
    pageSize: number
    /** 售后待处理快捷筛选（与指标口径一致：取消/退款异常态任一命中） */
    aftersalePending?: boolean
    sortBy?: "orderNo" | "mallOrderNo" | "externalOrderNo" | "lastBusinessAt"
    sortDir?: "asc" | "desc"
}

export type SupplierOrderListRow = {
    orderId: string
    orderNo: string
    mallOrderId: string
    mallOrderNo: string
    supplierId: string
    supplierName: string
    externalOrderNo?: string
    fulfillmentStatus: SupplierFulfillmentStatus
    fulfillmentLabel: string
    fulfillmentTone: StatusTone
    cancelStatus: CancelStatus
    cancelLabel: string
    cancelTone: StatusTone
    refundStatus: RefundStatus
    refundLabel: string
    refundTone: StatusTone
    paidAt: string
    updatedAt: string
    lastBusinessAt: string
    errorSummary?: string
    /** 商品明细行数 */
    itemCount: number
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
    priority: number
}

export type SupplierOrderMetric = {
    key: string
    label: string
    value: number
    /** 指标一键筛选：写入 fulfillmentStatuses 或 view */
    fulfillmentStatuses?: SupplierFulfillmentStatus[]
    fulfillmentStatus?: SupplierFulfillmentStatus
    view?: ListView
    aftersalePending?: boolean
}

export type SupplierOrderListResult = {
    rows: SupplierOrderListRow[]
    pageInfo: { page: number; pageSize: number; total: number }
    metrics: SupplierOrderMetric[]
    permissionVersion: string
    sourceAsOf: string
    queriedAt: string
    filterSummary: string
}

export type ExportCommand = {
    selectionSnapshotId: string
    fieldSetId: string
    requestId: string
    rowCount: number
    filterSummary: string
}

export type ExportJobResult = {
    jobId: string
    requestId: string
    rowCount: number
    permissionVersion: string
    fieldSetId: string
    maskDisclaimer: string
    expiresAt: string
    downloadLabel: string
    status: "queued" | "succeeded"
}

type SupplierOrderItemView = {
    itemId: string
    mallLineId: string
    productName: string
    skuCode: string
    quantity: string
    unit: string
    supplierProductId: string
    supplierProductName: string
    publicationVersion: string
    supplyVersion: string
    /** 下单成本快照；无字段权限时为 null */
    unitCostGross: string | null
    unitCostNet: string | null
    inputTaxRate: string | null
    /** 快照不可变提示 */
    snapshotImmutable: true
}

type LogisticsView = {
    carrier?: string
    trackingNo?: string
    acceptedAt?: string
    shippedAt?: string
    completedAt?: string
}

type StatusHistoryItem = {
    id: string
    at: string
    track: "fulfillment" | "cancel" | "refund"
    fromLabel: string
    toLabel: string
    source: string
    note?: string
}

type AfterSalesTrackView = {
    requestId: string
    requestNo: string
    mallRequestRef: string
    scope: string
    requestedAt: string
    /** 商城退款记录 */
    mallRefund: {
        status: "NONE" | "PENDING" | "PARTIAL" | "FULL" | "FAILED"
        statusLabel: string
        amount?: string | null
        gapNote?: string
    }
    /** 卡券/余额恢复记录 */
    cardRestore: {
        status: "NONE" | "PENDING" | "DONE" | "NOT_APPLICABLE" | "FAILED"
        statusLabel: string
        gapNote?: string
    }
    /** 供应商退款记录 */
    supplierRefund: {
        status: RefundStatus
        statusLabel: string
        amount?: string | null
        gapNote?: string
    }
    cancelStatus: CancelStatus
    cancelLabel: string
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
}

type CostView = {
    cumulativeCostGross: string | null
    cumulativeCostNet: string | null
    costSource: string
    costVariance?: string | null
    settlementId?: string
    settlementNo?: string
    payableEntryLabel?: string
}

type SupplierActionView = {
    actionId: string
    actionType:
        | "PLACE"
        | "QUERY_RESULT"
        | "REPLAY"
        | "CANCEL"
        | "REFUND"
        | "NOTE"
    actionLabel: string
    at: string
    actor: string
    outcomeLabel: string
    outcomeTone: StatusTone
    /** 幂等键尾部摘要，非完整键 */
    idempotencyKeyTail: string
    attemptCount: number
    /** 技术摘要仅管理员可见；从不展示密钥/完整报文 */
    techSummary?: string
    operationId?: string
}

type AddressRevealView = {
    masked: string
    /** 完整地址仅在短时揭示会话中返回 */
    revealed?: string
    phoneMasked: string
    phoneRevealed?: string
    recipientMasked: string
    recipientRevealed?: string
    canReveal: boolean
    revealExpiresAt?: string
    auditNote?: string
}

type InvestigationEvidenceView = {
    evidenceId: string
    targetSupplierActionId: string
    outcome: InvestigationOutcome
    outcomeLabel: string
    recordedAt: string
    canSafeRetry: boolean
    externalOrderNo?: string
    summary: string
    verifiedSupplierActionResultId?: string
    verifiedResolution?: SupplierOrderTerminalResolution
}

type WorkItemView = {
    workItemId: string
    taskVersion: string
    workItemType: "INTEGRATION_RESULT_UNKNOWN" | "BUSINESS_EXCEPTION"
    businessObjectType: "SUPPLIER_FULFILLMENT_ORDER"
    businessObjectId: string
    subjectVersion: string
    processingState: WorkItemProcessingState
    ownerUser?: { id: string; displayName: string }
    allowedTaskActions: readonly WorkItemAllowedAction[]
    actionBlockers: readonly string[]
    workItemStatus: WorkItemStatus
}

export type SupplierOrderDetailView = {
    order: {
        id: string
        orderNo: string
        mallOrderId: string
        mallOrderNo: string
        paidAt: string
        paymentFactKey: string
        fulfillmentChain: "ERP_AUTOMATED"
        supplierId: string
        supplierName: string
        connectionCode: string
        connectionEnvironment: string
        supplyVersion: string
        publicationVersion: string
        externalOrderNo?: string
        fulfillmentStatus: SupplierFulfillmentStatus
        fulfillmentLabel: string
        fulfillmentTone: StatusTone
        cancelStatus: CancelStatus
        cancelLabel: string
        cancelTone: StatusTone
        refundStatus: RefundStatus
        refundLabel: string
        refundTone: StatusTone
        lockVersion: number
        /** 始终强调：商城支付已发生 */
        paymentOccurredNotice: string
        errorSummary?: string
    }
    items: SupplierOrderItemView[]
    logistics: LogisticsView
    statusHistory: StatusHistoryItem[]
    afterSales: AfterSalesTrackView[]
    costs: CostView
    actions: SupplierActionView[]
    address: AddressRevealView
    workItem?: WorkItemView
    workItemBlocker?: ActionBlocker
    lastInvestigation?: InvestigationEvidenceView
    /** 原下单动作 id，查询/重放目标 */
    placeActionId: string
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
    freshness: { updatedAt: string; state: "fresh" | "stale" }
}

export type FormalActionResponse<T = unknown> = {
    status: "succeeded" | "failed" | "unknown" | "blocked"
    message: string
    reference?: string
    operationId?: string
    data?: T
}

type SupplierOrderObjectInvestigationCommand = {
    commandKind: "OBJECT"
    orderId: string
    expectedLockVersion: number
    action: "QUERY_RESULT" | "REPLAY"
    operationId: string
    targetSupplierActionId: string
    idempotencyKey: string
}

type SupplierOrderTaskInvestigationCommand = {
    commandKind: "TASK"
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    action: {
        type: "QUERY_RESULT" | "REPLAY"
        orderId: string
        expectedOrderLockVersion: number
        targetSupplierActionId: string
        operationId: string
    }
    idempotencyKey: string
}

export type QueryResultInput =
    | (SupplierOrderObjectInvestigationCommand & { action: "QUERY_RESULT" })
    | (SupplierOrderTaskInvestigationCommand & {
          action: SupplierOrderTaskInvestigationCommand["action"] & {
              type: "QUERY_RESULT"
          }
      })

export type QueryResultData = {
    evidence: InvestigationEvidenceView
    lockVersion: number
    workItemStatus?: "OPEN"
    taskVersion?: string
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
}

export type ReplayInput =
    | (SupplierOrderObjectInvestigationCommand & { action: "REPLAY" })
    | (SupplierOrderTaskInvestigationCommand & {
          action: SupplierOrderTaskInvestigationCommand["action"] & {
              type: "REPLAY"
          }
      })

export type ReplayResultData = {
    evidence: InvestigationEvidenceView
    lockVersion: number
    workItemStatus?: "OPEN"
    taskVersion?: string
    externalOrderNo?: string
    fulfillmentStatus: SupplierFulfillmentStatus
    allowedActions: string[]
    actionBlockers: ActionBlocker[]
}

export type SupplierOrderTerminalResolution =
    | "ORDER_ACCEPTED"
    | "ORDER_REJECTED"
    | "ORDER_COMPLETED"
    | "CANCELED"
    | "REFUNDED"

export type CompleteSupplierOrderTaskInput = {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    decision: {
        type: "CONFIRM_VERIFIED_TERMINAL_RESULT"
        orderId: string
        expectedOrderLockVersion: number
        verifiedSupplierActionResultId: string
        resolution: SupplierOrderTerminalResolution
    }
    idempotencyKey: string
}

export type CompleteSupplierOrderTaskResult = {
    operationId: string
    workItemId: string
    workItemStatus: "COMPLETED"
    taskVersion: string
    lockVersion: number
    resolution: SupplierOrderTerminalResolution
}

export type AfterSalesActionInput = {
    orderId: string
    expectedLockVersion: number
    action: "CANCEL" | "REFUND"
    operationId: string
    idempotencyKey: string
    afterSalesRequestId: string
    reasonCode?: string
    comment?: string
}

export type AfterSalesActionResult = {
    lockVersion: number
    cancelStatus: CancelStatus
    refundStatus: RefundStatus
    actionRecordId: string
    note: string
}

export type RevealAddressInput = {
    orderId: string
    reason: string
}

export type RevealAddressResult = {
    address: AddressRevealView
    auditEventId: string
}

export type NoteInput = {
    orderId: string
    expectedLockVersion: number
    comment: string
    idempotencyKey: string
}
