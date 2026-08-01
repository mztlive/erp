/** W10 库存台账 · 客户端契约类型 */

export type InventoryView =
  | "balance"
  | "movement"
  | "reservation"
  | "adjustment"

export type InventoryAvailability = "all" | "positive" | "zero" | "reserved"

export type InventoryQuery = {
  view: InventoryView
  q?: string
  warehouseId?: string
  skuId?: string
  salesOrderLineId?: string
  availability?: InventoryAvailability
  adjustmentId?: string
  balanceId?: string
}

export type AllowedAction =
  | "CREATE_ADJUSTMENT"
  | "EXPORT"
  | "VIEW_SOURCE"
  | "VIEW_ADJUSTMENT"

export type ActionBlocker = Readonly<{
  action: AllowedAction | string
  code: string
  message: string
}>

export type StockBalanceRow = Readonly<{
  balanceId: string
  warehouseId: string
  warehouseCode: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  specSummary: string
  baseUnit: string
  onHandQuantity: string
  reservedQuantity: string
  availableQuantity: string
  lockVersion: number
  lastMovementId: string
  lastMovementAt: string
  lastMovementTypeLabel: string
  availability: "positive" | "zero" | "reserved"
  statusLabel: string
  statusTone: "success" | "destructive" | "warning" | "info" | "neutral"
  hasActiveReservation: boolean
  stockKind: "OWN_PHYSICAL"
  allowedActions: readonly AllowedAction[]
  actionBlockers: readonly ActionBlocker[]
}>

export type StockMovementRow = Readonly<{
  movementId: string
  balanceId: string
  warehouseId: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  baseUnit: string
  movementType: string
  movementTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  occurredAt: string
  recordedAt: string
  recordedByLabel: string
  sourceDocumentType: string
  sourceDocumentId: string
  sourceDocumentNo: string
  sourceHref?: string
  reverseOfMovementId?: string
  reversedByMovementId?: string
}>

export type StockReservationRow = Readonly<{
  reservationId: string
  balanceId: string
  warehouseId: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  baseUnit: string
  salesOrderId: string
  salesOrderNo: string
  salesOrderLineId: string
  salesOrderLineLabel: string
  purchaseAllocationId?: string
  inboundSourceDocumentNo?: string
  establishedQuantity: string
  consumedQuantity: string
  releasedQuantity: string
  remainingQuantity: string
  status: string
  statusLabel: string
  statusTone: "success" | "warning" | "info" | "neutral" | "destructive"
  establishedAt: string
  fulfillmentHref?: string
}>

export type StockAdjustmentRow = Readonly<{
  adjustmentId: string
  adjustmentNo: string
  balanceId: string
  warehouseId: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  baseUnit: string
  reasonType: string
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  status: string
  statusLabel: string
  statusTone: "neutral" | "warning" | "info" | "success" | "destructive"
  operatorLabel: string
  warehouseReviewerLabel?: string
  financeConfirmerLabel?: string
  postedAt?: string
  createdAt: string
  note?: string
}>

export type InventoryMetrics = Readonly<{
  balanceDimensionCount: number
  reservedDimensionCount: number
  zeroAvailableDimensionCount: number
  pendingAdjustmentCount: number
}>

export type InventoryEmptyReason =
  | "NO_DATA"
  | "FILTER_NO_RESULT"
  | "NO_DATA_SCOPE"
  | "PERMISSION_REVOKED"

export type InventoryListView = Readonly<{
  view: InventoryView
  metrics: InventoryMetrics
  balances: readonly StockBalanceRow[]
  movements: readonly StockMovementRow[]
  reservations: readonly StockReservationRow[]
  adjustments: readonly StockAdjustmentRow[]
  total: number
  filterSummary: string
  permissionVersion: string
  dataWatermark: string
  lastMovementWatermark: string
  queriedAt: string
  hasWarehouseScope: boolean
  moduleAllowed: boolean
  canCreateAdjustment: boolean
  canExport: boolean
  emptyReason?: InventoryEmptyReason
  excludedKindsNote: string
  openingStockNote: string
  warehouses: readonly {
    id: string
    code: string
    name: string
  }[]
}>

export type BalanceDetailView = Readonly<{
  balance: StockBalanceRow
  recentMovements: readonly StockMovementRow[]
  reservations: readonly StockReservationRow[]
  sourceDocuments: readonly {
    documentType: string
    documentId: string
    documentNo: string
    label: string
    href?: string
    workspaceId?: string
  }[]
  pendingAdjustments: readonly StockAdjustmentRow[]
  queriedAt: string
}>

export type AdjustmentReasonType =
  | "COUNT_GAIN"
  | "COUNT_LOSS"
  | "DAMAGE"
  | "OTHER"

export type AdjustmentDraftView = Readonly<{
  stockAdjustmentId: string
  adjustmentNo: string
  balanceId: string
  warehouseId: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  baseUnit: string
  reasonType: AdjustmentReasonType
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  note: string
  occurredAt: string
  status: string
  statusLabel: string
  balanceLockVersion: number
  editVersion: number
  operatorLabel: string
  segregationNote: string
}>

export type AdjustmentSubmitResponse =
  | {
      status: "succeeded"
      outcome: {
        kind: "SUBMITTED_FOR_WAREHOUSE_REVIEW"
        stockAdjustmentId: string
        adjustmentNo: string
        nextResponsible: string
        reference: string
        submittedAt: string
        balanceLockVersion: number
      }
    }
  | {
      status: "failed"
      code: string
      message: string
      latestLockVersion?: number
    }
  | {
      status: "unknown"
      message: string
      idempotencyKey: string
    }

export const REASON_TYPE_OPTIONS: readonly {
  value: AdjustmentReasonType
  label: string
  direction: "increase" | "decrease"
}[] = [
  { value: "COUNT_GAIN", label: "盘盈", direction: "increase" },
  { value: "COUNT_LOSS", label: "盘亏", direction: "decrease" },
  { value: "DAMAGE", label: "损坏", direction: "decrease" },
  { value: "OTHER", label: "其他", direction: "decrease" },
] as const

export const AVAILABILITY_LABEL: Record<InventoryAvailability, string> = {
  all: "全部",
  positive: "有可用",
  zero: "零可用",
  reserved: "有预占",
}

export const VIEW_LABEL: Record<InventoryView, string> = {
  balance: "余额",
  movement: "流水",
  reservation: "销售预占",
  adjustment: "调整记录",
}
