/**
 * W09 履约作业 · 客户端契约类型（对齐工作面文档 §5/§8 语义的 mock）。
 * Q1 未正式写回前：演示路径复用 workItemId + 会话租约；不宣称服务端已固化唯一任务事实源。
 */

export type FulfillmentOperationType =
  | "RECEIPT"
  | "WAREHOUSE_SHIP"
  | "SUPPLIER_DIRECT"
  | "ELECTRONIC"
  | "SERVICE"

export type GateState = "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"

export type FulfillmentResultCode = "SUCCESS" | "PARTIAL" | "FAILED"

export type DeferReasonCode =
  | "WAITING_SUPPLIER"
  | "WAITING_WAREHOUSE"
  | "WAITING_PAYMENT"
  | "NEED_CLARIFICATION"
  | "OTHER"

export const OPERATION_TYPE_LABEL: Record<FulfillmentOperationType, string> = {
  RECEIPT: "入库",
  WAREHOUSE_SHIP: "公司仓发",
  SUPPLIER_DIRECT: "供应商直发",
  ELECTRONIC: "电子交付",
  SERVICE: "线下服务",
}

export const OPERATION_TYPE_SHORT: Record<FulfillmentOperationType, string> = {
  RECEIPT: "入库",
  WAREHOUSE_SHIP: "仓发",
  SUPPLIER_DIRECT: "代发",
  ELECTRONIC: "电子",
  SERVICE: "服务",
}

export const DEFER_REASON_LABEL: Record<DeferReasonCode, string> = {
  WAITING_SUPPLIER: "等待供应商",
  WAITING_WAREHOUSE: "等待仓储配合",
  WAITING_PAYMENT: "等待付款核销",
  NEED_CLARIFICATION: "需业务澄清",
  OTHER: "其他",
}

export const RESULT_LABEL: Record<FulfillmentResultCode, string> = {
  SUCCESS: "成功",
  PARTIAL: "部分成功",
  FAILED: "失败",
}

/** URL type 参数 */
export const TYPE_SLUG: Record<FulfillmentOperationType, string> = {
  RECEIPT: "receipt",
  WAREHOUSE_SHIP: "warehouse_ship",
  SUPPLIER_DIRECT: "supplier_direct",
  ELECTRONIC: "electronic",
  SERVICE: "service",
}

export const SLUG_TO_TYPE: Record<string, FulfillmentOperationType> = {
  receipt: "RECEIPT",
  warehouse_ship: "WAREHOUSE_SHIP",
  supplier_direct: "SUPPLIER_DIRECT",
  electronic: "ELECTRONIC",
  service: "SERVICE",
}

export type FulfillmentSourceLine = Readonly<{
  lineId: string
  salesOrderLineId: string
  purchaseRevisionLineId?: string
  itemName: string
  skuCode: string
  unitCode: string
  orderedQuantity: string
  remainingQuantity: string
  /** 仓发：有效预占 */
  stockReservationId?: string
  reservedQuantity?: string
  availableOnHand?: string
  /** 直发/电子/服务：采购销售分配 */
  purchaseLineSalesAllocationId?: string
}>

export type ReceiptDraftLine = Readonly<{
  purchaseRevisionLineId: string
  receivedQuantity: string
  qualifiedQuantity: string
  rejectedQuantity: string
  qualityResult: string
  evidenceNote?: string
}>

export type ShipDraftLine = Readonly<{
  salesOrderLineId: string
  stockReservationId: string
  quantity: string
}>

export type DirectDraftLine = Readonly<{
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  quantity: string
}>

export type ElectronicDraftLine = Readonly<{
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  quantity: string
  evidenceNote?: string
}>

export type ServiceDraftLine = Readonly<{
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  quantity: string
  evidenceNote?: string
}>

export type FulfillmentDraft =
  | {
      type: "RECEIPT"
      warehouseId: string
      warehouseLabel: string
      occurredAt: string
      lines: ReceiptDraftLine[]
    }
  | {
      type: "WAREHOUSE_SHIP"
      warehouseId: string
      warehouseLabel: string
      carrier: string
      trackingNo: string
      shippedAt: string
      evidenceNote?: string
      lines: ShipDraftLine[]
    }
  | {
      type: "SUPPLIER_DIRECT"
      carrier: string
      trackingNo: string
      shippedAt: string
      evidenceNote?: string
      lines: DirectDraftLine[]
    }
  | {
      type: "ELECTRONIC"
      occurredAt: string
      recipientMasked: string
      result: FulfillmentResultCode
      evidenceNote?: string
      lines: ElectronicDraftLine[]
    }
  | {
      type: "SERVICE"
      startedAt: string
      endedAt: string
      serviceLocation: string
      result: FulfillmentResultCode
      completionNote: string
      evidenceNote?: string
      lines: ServiceDraftLine[]
    }

export type FulfillmentTask = Readonly<{
  workItemId: string
  operationType: FulfillmentOperationType
  priority: number
  dueAt: string
  dueLabel: string
  overdue: boolean
  held?: boolean
  statusLabel: string
  statusTone: "warning" | "info" | "success" | "destructive" | "neutral"
  responsibleLabel: string
  sourceVersion: string
  subjectHash: string
  editVersion: number
  source: {
    purchaseOrderId?: string
    purchaseNo?: string
    purchaseRevisionId?: string
    salesOrderId: string
    salesOrderNo: string
    salesRevisionId: string
    supplierLabel?: string
    customerLabel: string
    warehouseId?: string
    warehouseLabel?: string
  }
  gate: {
    state: GateState
    message: string
    effectivePaidAmount?: string
    requiredAmount?: string
  }
  lines: readonly FulfillmentSourceLine[]
  draft: FulfillmentDraft
  summary: string
  impact: string
  allowedActions: readonly string[]
  actionBlockers: readonly {
    action: string
    code: string
    message: string
  }[]
  lease?: {
    claimedByLabel: string
    expiresAt: string
    leaseVersion: number
  }
}>

export type FulfillmentQueueMetrics = ReadonlyArray<{
  operationType: FulfillmentOperationType
  label: string
  count: number
  visible: boolean
}>

export type FulfillmentQueueView = Readonly<{
  context: {
    queueContextId: string
    position: number
    total: number
    currentWorkItemId?: string
    previousWorkItemId?: string
    nextWorkItemId?: string
    filterSummary: string
    snapshotUpdatedAt: string
  }
  metrics: FulfillmentQueueMetrics
  tasks: readonly FulfillmentTask[]
  current?: FulfillmentTask
  emptyReason?: "NO_TASKS" | "FILTER_NO_RESULT" | "NO_DATA_SCOPE"
  preferences: { autoNextDefault: boolean }
}>

export type WorkItemLease = Readonly<{
  workItemId: string
  claimedByLabel: string
  expiresAt: string
  leaseVersion: number
  claimToken: string
}>

export type InventoryDelta = Readonly<{
  warehouseId: string
  warehouseLabel: string
  skuId: string
  skuLabel: string
  quantity: string
  direction: "INCREASE" | "DECREASE"
}>

export type ReservationDelta = Readonly<{
  reservationId: string
  quantity: string
  action: "CREATE" | "CONSUME"
  salesOrderLineId?: string
}>

export type FulfillmentFormalOutcome = Readonly<{
  kind: "POSTED"
  workItemId: string
  factType:
    | "PURCHASE_RECEIPT"
    | "DELIVERY"
    | "ELECTRONIC_DELIVERY"
    | "SERVICE_FULFILLMENT"
  factId: string
  factNo: string
  formalStatus: string
  occurredAt: string
  operationType: FulfillmentOperationType
  inventoryDelta: readonly InventoryDelta[]
  reservationDelta: readonly ReservationDelta[]
  remainingByLine: readonly {
    salesOrderLineId: string
    itemName: string
    quantity: string
  }[]
  acceptanceRequired: boolean
  acceptanceNextStep: string
  inventoryImpactSummary: string
  reference: string
  nextWorkItemId?: string
  salesOrderId: string
  salesOrderNo: string
}>

export type DeferOutcome = Readonly<{
  kind: "DEFERRED"
  workItemId: string
  workItemStatus: "PENDING" | "IN_PROGRESS"
  leaseDisposition: "RELEASED"
  reasonCode: DeferReasonCode
  reasonNote?: string
  nextWorkItemId?: string
  reference: string
}>

export type FormalActionResponse =
  | { status: "succeeded"; outcome: FulfillmentFormalOutcome | DeferOutcome }
  | { status: "failed"; message: string; code: string }
  | { status: "unknown"; message: string; idempotencyKey: string }

export const FACT_TYPE_LABEL: Record<
  FulfillmentFormalOutcome["factType"],
  string
> = {
  PURCHASE_RECEIPT: "采购入库",
  DELIVERY: "发货",
  ELECTRONIC_DELIVERY: "电子交付",
  SERVICE_FULFILLMENT: "服务履约",
}

/** 履约/物流 ≠ 客户验收 */
export const NOT_ACCEPTANCE_NOTICE =
  "本工作面只登记履约事实（入库/发货/交付/服务）。物流签收或履约完成不等于 W06 客户验收通过。"

export const CORRECTION_NOTICE =
  "已过账/已确认事实不可覆盖；纠错请使用冲正、退货、库存调整或新履约事实，不要在本页改写历史。"
