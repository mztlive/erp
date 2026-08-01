/**
 * W10 库存台账 seed。
 * 仅含 ERP 自有实物库存；卡券实体卡 / 供应商直发 / 电子交付 / 线下服务不入账。
 * 数量字段一律服务端预计算，前端禁止重算覆盖。
 */

export type InventoryAvailability = "all" | "positive" | "zero" | "reserved"

export type InventoryView =
  | "balance"
  | "movement"
  | "reservation"
  | "adjustment"

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
  /** 服务端字段，前端不得本地重算 */
  onHandQuantity: string
  reservedQuantity: string
  availableQuantity: string
  lockVersion: number
  lastMovementId: string
  lastMovementAt: string
  lastMovementTypeLabel: string
  /** 服务端状态投影：positive | zero | reserved（有预占） */
  availability: "positive" | "zero" | "reserved"
  statusLabel: string
  statusTone: "success" | "destructive" | "warning" | "info" | "neutral"
  hasActiveReservation: boolean
  /** 排除说明：本余额属于自有实物库存 */
  stockKind: "OWN_PHYSICAL"
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
  sourceDocumentType:
    | "PURCHASE_RECEIPT"
    | "WAREHOUSE_DISPATCH"
    | "STOCK_ADJUSTMENT"
    | "OPENING_IMPORT"
    | "SALES_RESERVATION_CONSUME"
  sourceDocumentId: string
  sourceDocumentNo: string
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
  status: "ACTIVE" | "PARTIALLY_CONSUMED" | "FULLY_CONSUMED" | "RELEASED"
  statusLabel: string
  statusTone: "success" | "warning" | "info" | "neutral" | "destructive"
  establishedAt: string
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
  reasonType: "COUNT_GAIN" | "COUNT_LOSS" | "DAMAGE" | "OTHER"
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  status:
    | "DRAFT"
    | "PENDING_WAREHOUSE_REVIEW"
    | "PENDING_FINANCE"
    | "POSTED"
    | "REJECTED"
  statusLabel: string
  statusTone: "neutral" | "warning" | "info" | "success" | "destructive"
  operatorLabel: string
  warehouseReviewerLabel?: string
  financeConfirmerLabel?: string
  postedAt?: string
  createdAt: string
  note?: string
}>

export type SourceDocumentLink = Readonly<{
  documentType: StockMovementRow["sourceDocumentType"]
  documentId: string
  documentNo: string
  label: string
  /** 本地路由注册表目标；无权限时前端禁用 */
  href?: string
  workspaceId?: string
}>

/** 演示：无仓库数据范围时的空响应开关（默认有范围） */
export const INVENTORY_DEMO_HAS_WAREHOUSE_SCOPE = true

/** 演示：模块权限是否可用（默认可用；可被 session revoke 覆盖） */
export const INVENTORY_DEMO_MODULE_ALLOWED = true

export const INVENTORY_WAREHOUSES = [
  { id: "wh_east_1", code: "WH-E01", name: "华东一号仓" },
  { id: "wh_south_1", code: "WH-S01", name: "华南中转仓" },
  { id: "wh_north_1", code: "WH-N01", name: "华北保税仓" },
] as const

/**
 * 8 条自有实物库存余额，覆盖正常/零可用/有预占/多仓库。
 * 刻意不包含：实体卡、供应商直发量、电子交付、线下服务。
 */
export const INVENTORY_BALANCE_SEED: readonly StockBalanceRow[] = [
  {
    balanceId: "bal_1",
    warehouseId: "wh_east_1",
    warehouseCode: "WH-E01",
    warehouseName: "华东一号仓",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒",
    specSummary: "混合坚果 1.2kg / 盒",
    baseUnit: "盒",
    onHandQuantity: "860",
    reservedQuantity: "220",
    availableQuantity: "640",
    lockVersion: 12,
    lastMovementId: "mov_8",
    lastMovementAt: "2026-07-30T14:22:00+08:00",
    lastMovementTypeLabel: "采购入库",
    availability: "reserved",
    statusLabel: "有预占",
    statusTone: "info",
    hasActiveReservation: true,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_2",
    warehouseId: "wh_east_1",
    warehouseCode: "WH-E01",
    warehouseName: "华东一号仓",
    skuId: "sku_tea_09",
    skuCode: "SKU-TEA-09",
    skuName: "礼盒红茶",
    specSummary: "正山小种 500g / 盒",
    baseUnit: "盒",
    onHandQuantity: "48",
    reservedQuantity: "30",
    availableQuantity: "18",
    lockVersion: 7,
    lastMovementId: "mov_5",
    lastMovementAt: "2026-07-28T10:05:00+08:00",
    lastMovementTypeLabel: "仓发出库",
    availability: "reserved",
    statusLabel: "有预占",
    statusTone: "info",
    hasActiveReservation: true,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_3",
    warehouseId: "wh_east_1",
    warehouseCode: "WH-E01",
    warehouseName: "华东一号仓",
    skuId: "sku_cup_03",
    skuCode: "SKU-CUP-03",
    skuName: "陶瓷保温杯",
    specSummary: "420ml 米白",
    baseUnit: "个",
    onHandQuantity: "12",
    reservedQuantity: "12",
    availableQuantity: "0",
    lockVersion: 4,
    lastMovementId: "mov_3",
    lastMovementAt: "2026-07-26T16:40:00+08:00",
    lastMovementTypeLabel: "销售预占消耗",
    availability: "zero",
    statusLabel: "零可用",
    statusTone: "destructive",
    hasActiveReservation: true,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_4",
    warehouseId: "wh_south_1",
    warehouseCode: "WH-S01",
    warehouseName: "华南中转仓",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒",
    specSummary: "混合坚果 1.2kg / 盒",
    baseUnit: "盒",
    onHandQuantity: "120",
    reservedQuantity: "0",
    availableQuantity: "120",
    lockVersion: 3,
    lastMovementId: "mov_6",
    lastMovementAt: "2026-07-29T09:12:00+08:00",
    lastMovementTypeLabel: "采购入库",
    availability: "positive",
    statusLabel: "正常",
    statusTone: "success",
    hasActiveReservation: false,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_5",
    warehouseId: "wh_south_1",
    warehouseCode: "WH-S01",
    warehouseName: "华南中转仓",
    skuId: "sku_bag_11",
    skuCode: "SKU-BAG-11",
    skuName: "帆布礼袋",
    specSummary: "大号 / 米白",
    baseUnit: "个",
    onHandQuantity: "0",
    reservedQuantity: "0",
    availableQuantity: "0",
    lockVersion: 2,
    lastMovementId: "mov_2",
    lastMovementAt: "2026-07-20T11:00:00+08:00",
    lastMovementTypeLabel: "盘亏调整",
    availability: "zero",
    statusLabel: "零可用",
    statusTone: "destructive",
    hasActiveReservation: false,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_6",
    warehouseId: "wh_north_1",
    warehouseCode: "WH-N01",
    warehouseName: "华北保税仓",
    skuId: "sku_wine_07",
    skuCode: "SKU-WINE-07",
    skuName: "进口红酒礼盒",
    specSummary: "750ml × 2",
    baseUnit: "盒",
    onHandQuantity: "36",
    reservedQuantity: "8",
    availableQuantity: "28",
    lockVersion: 5,
    lastMovementId: "mov_7",
    lastMovementAt: "2026-07-29T15:30:00+08:00",
    lastMovementTypeLabel: "采购入库",
    availability: "reserved",
    statusLabel: "有预占",
    statusTone: "info",
    hasActiveReservation: true,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_7",
    warehouseId: "wh_north_1",
    warehouseCode: "WH-N01",
    warehouseName: "华北保税仓",
    skuId: "sku_snack_22",
    skuCode: "SKU-SNACK-22",
    skuName: "坚果小包装",
    specSummary: "25g × 40 袋",
    baseUnit: "箱",
    onHandQuantity: "200",
    reservedQuantity: "0",
    availableQuantity: "200",
    lockVersion: 1,
    lastMovementId: "mov_1",
    lastMovementAt: "2026-07-15T08:00:00+08:00",
    lastMovementTypeLabel: "期初导入",
    availability: "positive",
    statusLabel: "正常",
    statusTone: "success",
    hasActiveReservation: false,
    stockKind: "OWN_PHYSICAL",
  },
  {
    balanceId: "bal_8",
    warehouseId: "wh_east_1",
    warehouseCode: "WH-E01",
    warehouseName: "华东一号仓",
    skuId: "sku_pen_15",
    skuCode: "SKU-PEN-15",
    skuName: "商务签字笔套装",
    specSummary: "黑/蓝各 6 支",
    baseUnit: "套",
    onHandQuantity: "5",
    reservedQuantity: "5",
    availableQuantity: "0",
    lockVersion: 6,
    lastMovementId: "mov_4",
    lastMovementAt: "2026-07-27T13:18:00+08:00",
    lastMovementTypeLabel: "销售预占建立",
    availability: "zero",
    statusLabel: "零可用",
    statusTone: "destructive",
    hasActiveReservation: true,
    stockKind: "OWN_PHYSICAL",
  },
]

export const INVENTORY_MOVEMENT_SEED: readonly StockMovementRow[] = [
  {
    movementId: "mov_8",
    balanceId: "bal_1",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒",
    baseUnit: "盒",
    movementType: "PURCHASE_RECEIPT",
    movementTypeLabel: "采购入库",
    direction: "increase",
    quantity: "200",
    occurredAt: "2026-07-30T14:22:00+08:00",
    recordedAt: "2026-07-30T14:25:11+08:00",
    recordedByLabel: "仓储·李明",
    sourceDocumentType: "PURCHASE_RECEIPT",
    sourceDocumentId: "rcpt_1008",
    sourceDocumentNo: "RK-20260730-008",
  },
  {
    movementId: "mov_7",
    balanceId: "bal_6",
    warehouseId: "wh_north_1",
    warehouseName: "华北保税仓",
    skuId: "sku_wine_07",
    skuCode: "SKU-WINE-07",
    skuName: "进口红酒礼盒",
    baseUnit: "盒",
    movementType: "PURCHASE_RECEIPT",
    movementTypeLabel: "采购入库",
    direction: "increase",
    quantity: "20",
    occurredAt: "2026-07-29T15:30:00+08:00",
    recordedAt: "2026-07-29T15:32:00+08:00",
    recordedByLabel: "仓储·王芳",
    sourceDocumentType: "PURCHASE_RECEIPT",
    sourceDocumentId: "rcpt_1007",
    sourceDocumentNo: "RK-20260729-007",
  },
  {
    movementId: "mov_6",
    balanceId: "bal_4",
    warehouseId: "wh_south_1",
    warehouseName: "华南中转仓",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒",
    baseUnit: "盒",
    movementType: "PURCHASE_RECEIPT",
    movementTypeLabel: "采购入库",
    direction: "increase",
    quantity: "120",
    occurredAt: "2026-07-29T09:12:00+08:00",
    recordedAt: "2026-07-29T09:15:00+08:00",
    recordedByLabel: "仓储·陈晨",
    sourceDocumentType: "PURCHASE_RECEIPT",
    sourceDocumentId: "rcpt_1006",
    sourceDocumentNo: "RK-20260729-006",
  },
  {
    movementId: "mov_5",
    balanceId: "bal_2",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_tea_09",
    skuCode: "SKU-TEA-09",
    skuName: "礼盒红茶",
    baseUnit: "盒",
    movementType: "WAREHOUSE_DISPATCH",
    movementTypeLabel: "仓发出库",
    direction: "decrease",
    quantity: "12",
    occurredAt: "2026-07-28T10:05:00+08:00",
    recordedAt: "2026-07-28T10:08:22+08:00",
    recordedByLabel: "仓储·李明",
    sourceDocumentType: "WAREHOUSE_DISPATCH",
    sourceDocumentId: "disp_204",
    sourceDocumentNo: "CK-20260728-204",
  },
  {
    movementId: "mov_4",
    balanceId: "bal_8",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_pen_15",
    skuCode: "SKU-PEN-15",
    skuName: "商务签字笔套装",
    baseUnit: "套",
    movementType: "RESERVATION_ESTABLISH",
    movementTypeLabel: "销售预占建立",
    direction: "decrease",
    quantity: "5",
    occurredAt: "2026-07-27T13:18:00+08:00",
    recordedAt: "2026-07-27T13:18:40+08:00",
    recordedByLabel: "系统",
    sourceDocumentType: "SALES_RESERVATION_CONSUME",
    sourceDocumentId: "so_line_pen_1",
    sourceDocumentNo: "SO-2026-0812 / 行 1",
  },
  {
    movementId: "mov_3",
    balanceId: "bal_3",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_cup_03",
    skuCode: "SKU-CUP-03",
    skuName: "陶瓷保温杯",
    baseUnit: "个",
    movementType: "RESERVATION_CONSUME",
    movementTypeLabel: "销售预占消耗",
    direction: "decrease",
    quantity: "8",
    occurredAt: "2026-07-26T16:40:00+08:00",
    recordedAt: "2026-07-26T16:41:05+08:00",
    recordedByLabel: "系统",
    sourceDocumentType: "WAREHOUSE_DISPATCH",
    sourceDocumentId: "disp_198",
    sourceDocumentNo: "CK-20260726-198",
  },
  {
    movementId: "mov_2",
    balanceId: "bal_5",
    warehouseId: "wh_south_1",
    warehouseName: "华南中转仓",
    skuId: "sku_bag_11",
    skuCode: "SKU-BAG-11",
    skuName: "帆布礼袋",
    baseUnit: "个",
    movementType: "STOCK_ADJUSTMENT",
    movementTypeLabel: "盘亏调整",
    direction: "decrease",
    quantity: "6",
    occurredAt: "2026-07-20T11:00:00+08:00",
    recordedAt: "2026-07-20T11:12:00+08:00",
    recordedByLabel: "仓储·陈晨",
    sourceDocumentType: "STOCK_ADJUSTMENT",
    sourceDocumentId: "adj_seed_1",
    sourceDocumentNo: "TZ-20260720-001",
  },
  {
    movementId: "mov_1",
    balanceId: "bal_7",
    warehouseId: "wh_north_1",
    warehouseName: "华北保税仓",
    skuId: "sku_snack_22",
    skuCode: "SKU-SNACK-22",
    skuName: "坚果小包装",
    baseUnit: "箱",
    movementType: "OPENING_IMPORT",
    movementTypeLabel: "期初导入",
    direction: "increase",
    quantity: "200",
    occurredAt: "2026-07-15T08:00:00+08:00",
    recordedAt: "2026-07-15T08:05:00+08:00",
    recordedByLabel: "导入作业",
    sourceDocumentType: "OPENING_IMPORT",
    sourceDocumentId: "imp_open_1",
    sourceDocumentNo: "IMP-OPEN-20260715",
  },
]

export const INVENTORY_RESERVATION_SEED: readonly StockReservationRow[] = [
  {
    reservationId: "rsv_1",
    balanceId: "bal_1",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒",
    baseUnit: "盒",
    salesOrderId: "so_demo_1",
    salesOrderNo: "SO-2026-0801",
    salesOrderLineId: "sol_ny_1",
    salesOrderLineLabel: "行 1 · 新春坚果礼盒",
    purchaseAllocationId: "alloc_88",
    inboundSourceDocumentNo: "RK-20260730-008",
    establishedQuantity: "220",
    consumedQuantity: "0",
    releasedQuantity: "0",
    remainingQuantity: "220",
    status: "ACTIVE",
    statusLabel: "有效",
    statusTone: "info",
    establishedAt: "2026-07-30T14:26:00+08:00",
  },
  {
    reservationId: "rsv_2",
    balanceId: "bal_2",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_tea_09",
    skuCode: "SKU-TEA-09",
    skuName: "礼盒红茶",
    baseUnit: "盒",
    salesOrderId: "so_demo_2",
    salesOrderNo: "SO-2026-0788",
    salesOrderLineId: "sol_tea_1",
    salesOrderLineLabel: "行 2 · 礼盒红茶",
    purchaseAllocationId: "alloc_72",
    inboundSourceDocumentNo: "RK-20260720-003",
    establishedQuantity: "40",
    consumedQuantity: "10",
    releasedQuantity: "0",
    remainingQuantity: "30",
    status: "PARTIALLY_CONSUMED",
    statusLabel: "部分消耗",
    statusTone: "warning",
    establishedAt: "2026-07-20T16:00:00+08:00",
  },
  {
    reservationId: "rsv_3",
    balanceId: "bal_3",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_cup_03",
    skuCode: "SKU-CUP-03",
    skuName: "陶瓷保温杯",
    baseUnit: "个",
    salesOrderId: "so_demo_3",
    salesOrderNo: "SO-2026-0750",
    salesOrderLineId: "sol_cup_1",
    salesOrderLineLabel: "行 1 · 陶瓷保温杯",
    establishedQuantity: "12",
    consumedQuantity: "0",
    releasedQuantity: "0",
    remainingQuantity: "12",
    status: "ACTIVE",
    statusLabel: "有效",
    statusTone: "info",
    establishedAt: "2026-07-25T09:30:00+08:00",
  },
  {
    reservationId: "rsv_4",
    balanceId: "bal_6",
    warehouseId: "wh_north_1",
    warehouseName: "华北保税仓",
    skuId: "sku_wine_07",
    skuCode: "SKU-WINE-07",
    skuName: "进口红酒礼盒",
    baseUnit: "盒",
    salesOrderId: "so_demo_4",
    salesOrderNo: "SO-2026-0799",
    salesOrderLineId: "sol_wine_1",
    salesOrderLineLabel: "行 1 · 进口红酒礼盒",
    inboundSourceDocumentNo: "RK-20260729-007",
    establishedQuantity: "8",
    consumedQuantity: "0",
    releasedQuantity: "0",
    remainingQuantity: "8",
    status: "ACTIVE",
    statusLabel: "有效",
    statusTone: "info",
    establishedAt: "2026-07-29T15:40:00+08:00",
  },
  {
    reservationId: "rsv_5",
    balanceId: "bal_8",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_pen_15",
    skuCode: "SKU-PEN-15",
    skuName: "商务签字笔套装",
    baseUnit: "套",
    salesOrderId: "so_demo_5",
    salesOrderNo: "SO-2026-0812",
    salesOrderLineId: "sol_pen_1",
    salesOrderLineLabel: "行 1 · 商务签字笔套装",
    establishedQuantity: "5",
    consumedQuantity: "0",
    releasedQuantity: "0",
    remainingQuantity: "5",
    status: "ACTIVE",
    statusLabel: "有效",
    statusTone: "info",
    establishedAt: "2026-07-27T13:18:00+08:00",
  },
]

export const INVENTORY_ADJUSTMENT_SEED: readonly StockAdjustmentRow[] = [
  {
    adjustmentId: "adj_seed_1",
    adjustmentNo: "TZ-20260720-001",
    balanceId: "bal_5",
    warehouseId: "wh_south_1",
    warehouseName: "华南中转仓",
    skuId: "sku_bag_11",
    skuCode: "SKU-BAG-11",
    skuName: "帆布礼袋",
    baseUnit: "个",
    reasonType: "COUNT_LOSS",
    reasonTypeLabel: "盘亏",
    direction: "decrease",
    quantity: "6",
    status: "POSTED",
    statusLabel: "已过账",
    statusTone: "success",
    operatorLabel: "仓储·陈晨",
    warehouseReviewerLabel: "仓储复核·赵强",
    financeConfirmerLabel: "财务·周敏",
    postedAt: "2026-07-20T11:12:00+08:00",
    createdAt: "2026-07-20T10:30:00+08:00",
    note: "月度盘点差异",
  },
  {
    adjustmentId: "adj_pending_1",
    adjustmentNo: "TZ-20260731-002",
    balanceId: "bal_2",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_tea_09",
    skuCode: "SKU-TEA-09",
    skuName: "礼盒红茶",
    baseUnit: "盒",
    reasonType: "DAMAGE",
    reasonTypeLabel: "损坏",
    direction: "decrease",
    quantity: "2",
    status: "PENDING_WAREHOUSE_REVIEW",
    statusLabel: "待仓储复核",
    statusTone: "warning",
    operatorLabel: "仓储·李明",
    createdAt: "2026-07-31T09:40:00+08:00",
    note: "外箱挤压破损，待复核",
  },
  {
    adjustmentId: "adj_pending_2",
    adjustmentNo: "TZ-20260731-003",
    balanceId: "bal_6",
    warehouseId: "wh_north_1",
    warehouseName: "华北保税仓",
    skuId: "sku_wine_07",
    skuCode: "SKU-WINE-07",
    skuName: "进口红酒礼盒",
    baseUnit: "盒",
    reasonType: "COUNT_GAIN",
    reasonTypeLabel: "盘盈",
    direction: "increase",
    quantity: "1",
    status: "PENDING_FINANCE",
    statusLabel: "待财务确认",
    statusTone: "info",
    operatorLabel: "仓储·王芳",
    warehouseReviewerLabel: "仓储复核·赵强",
    createdAt: "2026-07-31T11:05:00+08:00",
  },
  {
    adjustmentId: "adj_pending_3",
    adjustmentNo: "TZ-20260801-001",
    balanceId: "bal_1",
    warehouseId: "wh_east_1",
    warehouseName: "华东一号仓",
    skuId: "sku_ny_box_01",
    skuCode: "SKU-NY-BOX-01",
    skuName: "新春坚果礼盒",
    baseUnit: "盒",
    reasonType: "OTHER",
    reasonTypeLabel: "其他",
    direction: "decrease",
    quantity: "3",
    status: "PENDING_WAREHOUSE_REVIEW",
    statusLabel: "待仓储复核",
    statusTone: "warning",
    operatorLabel: "仓储·李明",
    createdAt: "2026-08-01T08:20:00+08:00",
    note: "样品领用待复核",
  },
]

/** 明确排除清单（仅文档/说明用，不进余额表） */
export const INVENTORY_EXCLUDED_FULFILLMENT_KINDS = [
  {
    kind: "CARD_PHYSICAL",
    label: "卡券实体卡",
    reason: "实体卡不进入 ERP 自有库存，不在库存台账展示卡号/卡密。",
  },
  {
    kind: "SUPPLIER_DIRECT",
    label: "供应商直发",
    reason: "外部供给进入外部商品供给，不与自有库存合并。",
  },
  {
    kind: "ELECTRONIC",
    label: "电子交付",
    reason: "非实物库存履约，不形成 stock_balance。",
  },
  {
    kind: "OFFLINE_SERVICE",
    label: "线下服务",
    reason: "服务履约不产生自有库存流水。",
  },
] as const

export function sourceDocumentHref(
  type: StockMovementRow["sourceDocumentType"],
  documentId: string
): SourceDocumentLink {
  switch (type) {
    case "PURCHASE_RECEIPT":
    case "WAREHOUSE_DISPATCH":
      return {
        documentType: type,
        documentId,
        documentNo: documentId,
        label: type === "PURCHASE_RECEIPT" ? "采购入库" : "仓发作业",
        href: `/fulfillment?sourceDocId=${encodeURIComponent(documentId)}`,
        workspaceId: "W09",
      }
    case "STOCK_ADJUSTMENT":
      return {
        documentType: type,
        documentId,
        documentNo: documentId,
        label: "库存调整单",
        href: `/inventory?view=adjustment&adjustmentId=${encodeURIComponent(documentId)}`,
        workspaceId: "W10",
      }
    case "OPENING_IMPORT":
      return {
        documentType: type,
        documentId,
        documentNo: documentId,
        label: "期初导入",
        href: `/governance/imports?batchId=${encodeURIComponent(documentId)}`,
        workspaceId: "W18",
      }
    case "SALES_RESERVATION_CONSUME":
      return {
        documentType: type,
        documentId,
        documentNo: documentId,
        label: "销售预占来源",
        href: `/sales/orders`,
        workspaceId: "W05",
      }
  }
}
