/**
 * W09 履约作业 · 客户端契约类型（对齐工作面文档 §5/§8）。
 */

export type FulfillmentOperationType =
  | "RECEIPT"
  | "WAREHOUSE_SHIP"
  | "SUPPLIER_DIRECT"
  | "ELECTRONIC"
  | "SERVICE"

type GateState = "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"

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

/** 主按钮：按作业类型说人话，不说「过账」 */
export const OPERATION_ACTION_LABEL: Record<FulfillmentOperationType, string> = {
  RECEIPT: "确认入库",
  WAREHOUSE_SHIP: "确认发货",
  SUPPLIER_DIRECT: "确认发货",
  ELECTRONIC: "确认交付",
  SERVICE: "确认完成",
}

/** 确认弹窗标题 */
export const OPERATION_CONFIRM_TITLE: Record<FulfillmentOperationType, string> =
  {
    RECEIPT: "确认入库？",
    WAREHOUSE_SHIP: "确认发货？",
    SUPPLIER_DIRECT: "确认发货？",
    ELECTRONIC: "确认交付？",
    SERVICE: "确认服务完成？",
  }

/** 完成后的状态说法 */
export const OPERATION_DONE_LABEL: Record<FulfillmentOperationType, string> = {
  RECEIPT: "已入库",
  WAREHOUSE_SHIP: "已发货",
  SUPPLIER_DIRECT: "已发货",
  ELECTRONIC: "已交付",
  SERVICE: "已完成",
}

/** 队列空态：说清「今天这类活干完了」 */
export const OPERATION_CLEARED_LABEL: Record<
  FulfillmentOperationType,
  string
> = {
  RECEIPT: "今天的入库都干完了",
  WAREHOUSE_SHIP: "今天的发货都干完了",
  SUPPLIER_DIRECT: "今天的直发都干完了",
  ELECTRONIC: "今天的电子交付都干完了",
  SERVICE: "今天的服务都干完了",
}

export const DEFER_REASON_LABEL: Record<DeferReasonCode, string> = {
  WAITING_SUPPLIER: "等待供应商",
  WAITING_WAREHOUSE: "等待仓储配合",
  WAITING_PAYMENT: "等待付款核销",
  NEED_CLARIFICATION: "需业务澄清",
  OTHER: "其他",
}

/** 跳过后的任务状态 → 中文（禁止 PENDING 等枚举原值上屏）。 */
export const WORK_ITEM_STATUS_LABEL: Record<string, string> = {
  PENDING: "待处理",
  IN_PROGRESS: "处理中",
}

export const RESULT_LABEL: Record<FulfillmentResultCode, string> = {
  SUCCESS: "成功",
  PARTIAL: "部分成功",
  FAILED: "失败",
}

/** 电子交付 / 线下服务共用的履约结果选项 */
export const RESULT_OPTIONS: ReadonlyArray<{
  value: FulfillmentResultCode
  label: string
}> = (
  Object.keys(RESULT_LABEL) as FulfillmentResultCode[]
).map((value) => ({ value, label: RESULT_LABEL[value] }))

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

type ShipDraftLine = Readonly<{
  salesOrderLineId: string
  stockReservationId: string
  quantity: string
}>

type DirectDraftLine = Readonly<{
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  quantity: string
}>

type ElectronicDraftLine = Readonly<{
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  quantity: string
  evidenceNote?: string
}>

type ServiceDraftLine = Readonly<{
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
  }
}>

type FulfillmentQueueMetrics = ReadonlyArray<{
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
    /** 仓筛选可选值，按权限范围全量投影去重（不随当前队列收缩） */
    warehouseOptions: ReadonlyArray<{ value: string; label: string }>
    /** 当前角色能看到的作业类型；类型分段与指标只渲染这些 */
    visibleTypes: readonly FulfillmentOperationType[]
    roleLabel: string
    /** 当前登录人；只读角色为 undefined */
    viewerLabel?: string
    canExecute: boolean
    snapshotUpdatedAt: string
  }
  metrics: FulfillmentQueueMetrics
  tasks: readonly FulfillmentTask[]
  current?: FulfillmentTask
  emptyReason?:
    | "NO_TASKS"
    | "FILTER_NO_RESULT"
    | "NO_DATA_SCOPE"
    | "NO_PERMISSION"
  preferences: { autoNextDefault: boolean }
}>

export type WorkItemLease = Readonly<{
  workItemId: string
  claimedByLabel: string
}>

type InventoryDelta = Readonly<{
  warehouseId: string
  warehouseLabel: string
  skuId: string
  skuLabel: string
  quantity: string
  direction: "INCREASE" | "DECREASE"
}>

type ReservationDelta = Readonly<{
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

/** 正式记录状态码 → 人话（界面不出现 POSTED/SHIPPED 这类原值） */
export const FORMAL_STATUS_LABEL: Record<string, string> = {
  POSTED: "已入库",
  SHIPPED: "已发出",
  CONFIRMED: "已确认",
  FAILED: "失败",
}

export const FACT_TYPE_LABEL: Record<
  FulfillmentFormalOutcome["factType"],
  string
> = {
  PURCHASE_RECEIPT: "采购入库",
  DELIVERY: "发货",
  ELECTRONIC_DELIVERY: "电子交付",
  SERVICE_FULFILLMENT: "服务履约",
}

/**
 * 物流签收 ≠ 客户验收。
 * 只在确认成功后的结果面板出现 —— 那时才有「去客户验收」这一步，常驻横幅只会被无视。
 */
export const NOT_ACCEPTANCE_NOTICE =
  "客户签收不等于验收通过。要等销售在客户验收里登记，这一单才算验收完成。"

/**
 * 记录不可覆盖。
 * 只在确认弹窗的不可逆区出现 —— 那时才是用户真的要下决定的时刻。
 */
export const CORRECTION_NOTICE =
  "确认之后这条记录就不能改了。写错了要走冲正、退货或库存调整，不能回来改这一条。"
