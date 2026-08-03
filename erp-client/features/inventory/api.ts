/**
 * W10 session-mock API：queryFn / mutationFn 纯函数。
 * available_quantity / 指标组合数一律来自服务端投影，禁止前端重算覆盖。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import {
  decodeInventoryCursor,
  encodeInventoryCursor,
} from "@/features/inventory/cursor"
import type {
  AdjustmentDraftView,
  AdjustmentReasonType,
  AdjustmentSubmitResponse,
  BalanceDetailView,
  InventoryListView,
  InventoryQuery,
  StockAdjustmentRow,
  StockBalanceRow,
  StockMovementRow,
  StockReservationRow,
} from "@/features/inventory/types"
import { AVAILABILITY_LABEL, VIEW_LABEL } from "@/features/inventory/types"
import {
  INVENTORY_ADJUSTMENT_SEED,
  INVENTORY_BALANCE_SEED,
  INVENTORY_DEMO_HAS_WAREHOUSE_SCOPE,
  INVENTORY_EXCLUDED_FULFILLMENT_KINDS,
  INVENTORY_MOVEMENT_SEED,
  INVENTORY_RESERVATION_SEED,
  INVENTORY_WAREHOUSES,
  sourceDocumentHref,
} from "@/mock/inventory"
import {
  createW10AdjustmentDraft,
  createW10ExportJob,
  getInventoryBalanceLockVersion,
  getW10AdjustmentDraft,
  getW10ExportJob,
  isInventoryPermissionRevoked,
  listW10SessionAdjustments,
  resolveW10AdjustmentUnknown,
  saveW10AdjustmentDraft,
  submitW10Adjustment,
  type W10ExportJob,
} from "@/mock/session-state"

const PERMISSION_VERSION = "pv-w10-demo-1"

const OPENING_STOCK_NOTE =
  "期初库存只能通过导入与期初的基准日实盘导入形成流水；旧商城 stock / total_stock 不作为 ERP 库存记录，本台账不会写入或展示旧商城库存字段。"

const EXCLUDED_NOTE = INVENTORY_EXCLUDED_FULFILLMENT_KINDS.map(
  (k) => `${k.label}：${k.reason}`
).join(" ")

function projectBalance(seed: (typeof INVENTORY_BALANCE_SEED)[number]): StockBalanceRow {
  const lockVersion = getInventoryBalanceLockVersion(
    seed.balanceId,
    seed.lockVersion
  )
  const canAdjust = !isInventoryPermissionRevoked()
  return {
    ...seed,
    lockVersion,
    allowedActions: canAdjust
      ? (["CREATE_ADJUSTMENT", "VIEW_SOURCE"] as const)
      : (["VIEW_SOURCE"] as const),
    actionBlockers: canAdjust
      ? []
      : [
          {
            action: "CREATE_ADJUSTMENT",
            code: "PERMISSION_REVOKED",
            message: "当前账号无库存调整权限或权限已被收回。",
          },
        ],
  }
}

function projectMovement(
  seed: (typeof INVENTORY_MOVEMENT_SEED)[number]
): StockMovementRow {
  const link = sourceDocumentHref(seed.sourceDocumentType, seed.sourceDocumentId)
  return {
    ...seed,
    sourceHref: link.href
      ? seed.sourceDocumentType === "PURCHASE_RECEIPT" ||
        seed.sourceDocumentType === "WAREHOUSE_DISPATCH"
        ? `/fulfillment?lane=warehouse&sourceDocId=${encodeURIComponent(seed.sourceDocumentId)}&warehouseId=${encodeURIComponent(seed.warehouseId)}&skuId=${encodeURIComponent(seed.skuId)}`
        : link.href
      : undefined,
  }
}

function projectReservation(
  seed: (typeof INVENTORY_RESERVATION_SEED)[number]
): StockReservationRow {
  return {
    ...seed,
    fulfillmentHref: `/fulfillment?lane=warehouse&type=warehouse_ship&scope=mine&warehouseId=${encodeURIComponent(seed.warehouseId)}&from=W10&returnTo=${encodeURIComponent(`/inventory?warehouseId=${seed.warehouseId}&skuId=${seed.skuId}`)}`,
  }
}

function sessionAdjustmentsAsRows(): StockAdjustmentRow[] {
  return listW10SessionAdjustments().map((d) => ({
    adjustmentId: d.stockAdjustmentId,
    adjustmentNo: d.adjustmentNo,
    balanceId: d.balanceId,
    warehouseId: d.warehouseId,
    warehouseName: d.warehouseName,
    skuId: d.skuId,
    skuCode: d.skuCode,
    skuName: d.skuName,
    baseUnit: d.baseUnit,
    reasonType: d.reasonType,
    reasonTypeLabel: d.reasonTypeLabel,
    direction: d.direction,
    quantity: d.quantity || "—",
    status: d.status,
    statusLabel: d.statusLabel,
    statusTone: d.statusTone,
    operatorLabel: d.operatorLabel,
    createdAt: d.createdAt,
    note: d.note || undefined,
  }))
}

function allAdjustments(): StockAdjustmentRow[] {
  const session = sessionAdjustmentsAsRows()
  const sessionIds = new Set(session.map((a) => a.adjustmentId))
  const seed = INVENTORY_ADJUSTMENT_SEED.filter(
    (a) => !sessionIds.has(a.adjustmentId)
  )
  return [...session, ...seed]
}

function matchSearch(
  q: string | undefined,
  parts: readonly (string | undefined)[]
): boolean {
  if (!q?.trim()) return true
  const needle = q.trim().toLowerCase()
  return parts.some((p) => p?.toLowerCase().includes(needle))
}

function filterSummary(query: InventoryQuery, total: number): string {
  const parts = [
    VIEW_LABEL[query.view],
    query.warehouseId
      ? (INVENTORY_WAREHOUSES.find((w) => w.id === query.warehouseId)?.name ??
        query.warehouseId)
      : "全部仓库",
    query.availability && query.availability !== "all"
      ? AVAILABILITY_LABEL[query.availability]
      : "全部状态",
  ]
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  if (query.skuId) parts.push(`SKU ${query.skuId}`)
  if (query.movementType?.length) {
    parts.push(`流水类型 ${query.movementType.join("、")}`)
  }
  if (query.view === "movement" && query.occurredFrom && query.occurredTo) {
    parts.push(`${query.occurredFrom} 至 ${query.occurredTo}`)
  }
  parts.push(`${total} 条`)
  return parts.join(" · ")
}

function sortRows<T>(
  rows: readonly T[],
  sort: readonly string[],
  valueOf: (row: T, field: string) => string | number | undefined
): T[] {
  return [...rows].sort((left, right) => {
    for (const token of sort) {
      const [field, rawDirection] = token.split(":")
      const direction = rawDirection === "desc" ? -1 : 1
      const leftValue = valueOf(left, field)
      const rightValue = valueOf(right, field)
      if (leftValue == null && rightValue == null) continue
      if (leftValue == null) return 1
      if (rightValue == null) return -1
      if (leftValue < rightValue) return -1 * direction
      if (leftValue > rightValue) return 1 * direction
    }
    return 0
  })
}

function defaultSort(view: InventoryQuery["view"]): string[] {
  if (view === "balance") return ["warehouseCode:asc", "skuCode:asc"]
  if (view === "movement") return ["occurredAt:desc", "movementId:desc"]
  if (view === "reservation") {
    return ["establishedAt:desc", "reservationId:desc"]
  }
  return ["createdAt:desc", "adjustmentId:desc"]
}

/**
 * 指标由服务端对当前筛选集合投影；只计组合数，不跨 SKU 汇总数量。
 */
function computeMetrics(
  balances: readonly StockBalanceRow[],
  adjustments: readonly StockAdjustmentRow[]
): InventoryListView["metrics"] {
  return {
    balanceDimensionCount: balances.length,
    reservedDimensionCount: balances.filter((b) => b.hasActiveReservation)
      .length,
    zeroAvailableDimensionCount: balances.filter(
      (b) => b.availableQuantity === "0"
    ).length,
    pendingAdjustmentCount: adjustments.filter(
      (a) =>
        a.status === "PENDING_WAREHOUSE_REVIEW" ||
        a.status === "PENDING_FINANCE" ||
        a.status === "DRAFT"
    ).length,
  }
}

export async function fetchInventoryList(
  query: InventoryQuery
): Promise<InventoryListView> {
  await mockDelay()

  const moduleAllowed = !isInventoryPermissionRevoked()
  const hasWarehouseScope = INVENTORY_DEMO_HAS_WAREHOUSE_SCOPE

  if (!moduleAllowed) {
    return {
      view: query.view,
      metrics: {
        balanceDimensionCount: 0,
        reservedDimensionCount: 0,
        zeroAvailableDimensionCount: 0,
        pendingAdjustmentCount: 0,
      },
      balances: [],
      movements: [],
      reservations: [],
      adjustments: [],
      total: 0,
      cursor: "",
      pageSize: query.pageSize,
      sort: query.sort,
      filterSummary: "权限已收回",
      permissionVersion: PERMISSION_VERSION,
      dataWatermark: "",
      lastMovementWatermark: "",
      queriedAt: new Date().toISOString(),
      hasWarehouseScope: false,
      moduleAllowed: false,
      canCreateAdjustment: false,
      canExport: false,
      emptyReason: "PERMISSION_REVOKED",
      excludedKindsNote: EXCLUDED_NOTE,
      openingStockNote: OPENING_STOCK_NOTE,
      warehouses: [...INVENTORY_WAREHOUSES],
    }
  }

  if (!hasWarehouseScope) {
    return {
      view: query.view,
      metrics: {
        balanceDimensionCount: 0,
        reservedDimensionCount: 0,
        zeroAvailableDimensionCount: 0,
        pendingAdjustmentCount: 0,
      },
      balances: [],
      movements: [],
      reservations: [],
      adjustments: [],
      total: 0,
      cursor: "",
      pageSize: query.pageSize,
      sort: query.sort,
      filterSummary: "未配置仓库数据范围",
      permissionVersion: PERMISSION_VERSION,
      dataWatermark: "",
      lastMovementWatermark: "",
      queriedAt: new Date().toISOString(),
      hasWarehouseScope: false,
      moduleAllowed: true,
      canCreateAdjustment: false,
      canExport: false,
      emptyReason: "NO_DATA_SCOPE",
      excludedKindsNote: EXCLUDED_NOTE,
      openingStockNote: OPENING_STOCK_NOTE,
      warehouses: [],
    }
  }

  let balances = INVENTORY_BALANCE_SEED.map(projectBalance)
  // 服务端按仓库范围过滤（演示：全部已授权仓）
  if (query.warehouseId) {
    balances = balances.filter((b) => b.warehouseId === query.warehouseId)
  }
  if (query.skuId) {
    balances = balances.filter((b) => b.skuId === query.skuId)
  }
  if (query.q?.trim()) {
    balances = balances.filter((b) =>
      matchSearch(query.q, [
        b.skuCode,
        b.skuName,
        b.specSummary,
        b.warehouseName,
        b.warehouseCode,
      ])
    )
  }
  if (query.availability && query.availability !== "all") {
    if (query.availability === "zero") {
      balances = balances.filter((b) => b.availableQuantity === "0")
    } else if (query.availability === "positive") {
      balances = balances.filter((b) => b.availableQuantity !== "0")
    } else if (query.availability === "reserved") {
      balances = balances.filter((b) => b.hasActiveReservation)
    }
  }
  if (query.balanceId) {
    balances = balances.filter((b) => b.balanceId === query.balanceId)
  }

  // 全量（仅仓库范围，无其它筛选）用于区分 NO_DATA vs FILTER_NO_RESULT
  const scopedAll = INVENTORY_BALANCE_SEED.map(projectBalance)
  const hasActiveFilters = Boolean(
    query.q?.trim() ||
      query.warehouseId ||
      query.skuId ||
      query.balanceId ||
      query.salesOrderLineId ||
      query.adjustmentId ||
      query.movementType?.length ||
      query.occurredFrom ||
      query.occurredTo ||
      (query.availability && query.availability !== "all")
  )

  let movements = INVENTORY_MOVEMENT_SEED.map(projectMovement)
  if (query.warehouseId) {
    movements = movements.filter((m) => m.warehouseId === query.warehouseId)
  }
  if (query.skuId) {
    movements = movements.filter((m) => m.skuId === query.skuId)
  }
  if (query.q?.trim()) {
    movements = movements.filter((m) =>
      matchSearch(query.q, [
        m.skuCode,
        m.skuName,
        m.warehouseName,
        m.sourceDocumentNo,
        m.movementTypeLabel,
      ])
    )
  }
  if (query.balanceId) {
    movements = movements.filter((m) => m.balanceId === query.balanceId)
  }
  if (query.movementType?.length) {
    const types = new Set(query.movementType)
    movements = movements.filter((movement) => types.has(movement.movementType))
  }
  if (query.occurredFrom) {
    movements = movements.filter(
      (movement) => movement.occurredAt.slice(0, 10) >= query.occurredFrom!
    )
  }
  if (query.occurredTo) {
    movements = movements.filter(
      (movement) => movement.occurredAt.slice(0, 10) <= query.occurredTo!
    )
  }

  let reservations = INVENTORY_RESERVATION_SEED.map(projectReservation)
  if (query.warehouseId) {
    reservations = reservations.filter(
      (r) => r.warehouseId === query.warehouseId
    )
  }
  if (query.skuId) {
    reservations = reservations.filter((r) => r.skuId === query.skuId)
  }
  if (query.salesOrderLineId) {
    reservations = reservations.filter(
      (r) => r.salesOrderLineId === query.salesOrderLineId
    )
  }
  if (query.q?.trim()) {
    reservations = reservations.filter((r) =>
      matchSearch(query.q, [
        r.skuCode,
        r.skuName,
        r.warehouseName,
        r.salesOrderNo,
        r.salesOrderLineLabel,
      ])
    )
  }
  if (query.balanceId) {
    reservations = reservations.filter((r) => r.balanceId === query.balanceId)
  }

  let adjustments = allAdjustments()
  if (query.warehouseId) {
    adjustments = adjustments.filter((a) => a.warehouseId === query.warehouseId)
  }
  if (query.skuId) {
    adjustments = adjustments.filter((a) => a.skuId === query.skuId)
  }
  if (query.adjustmentId) {
    adjustments = adjustments.filter(
      (a) => a.adjustmentId === query.adjustmentId
    )
  }
  if (query.q?.trim()) {
    adjustments = adjustments.filter((a) =>
      matchSearch(query.q, [
        a.skuCode,
        a.skuName,
        a.warehouseName,
        a.adjustmentNo,
        a.reasonTypeLabel,
      ])
    )
  }
  if (query.balanceId) {
    adjustments = adjustments.filter((a) => a.balanceId === query.balanceId)
  }

  // 指标：对“当前仓库/搜索筛选后、状态筛选前”的余额投影（与文档：筛选后指标）
  // 这里采用与余额表同一筛选集合，确保前端不自行加总。
  const metrics = computeMetrics(
    // metrics 使用与余额相同的筛选结果
    balances,
    allAdjustments().filter((a) => {
      if (query.warehouseId && a.warehouseId !== query.warehouseId) return false
      return true
    })
  )

  const sort = query.sort.length > 0 ? query.sort : defaultSort(query.view)
  balances = sortRows(balances, sort, (row, field) => {
    if (field === "warehouseCode") return row.warehouseCode
    if (field === "skuCode") return row.skuCode
    if (field === "lastMovementAt") return row.lastMovementAt
    return undefined
  })
  movements = sortRows(movements, sort, (row, field) => {
    if (field === "occurredAt") return row.occurredAt
    if (field === "recordedAt") return row.recordedAt
    if (field === "movementId") return row.movementId
    return undefined
  })
  reservations = sortRows(reservations, sort, (row, field) => {
    if (field === "establishedAt") return row.establishedAt
    if (field === "reservationId") return row.reservationId
    if (field === "salesOrderNo") return row.salesOrderNo
    return undefined
  })
  adjustments = sortRows(adjustments, sort, (row, field) => {
    if (field === "createdAt") return row.createdAt
    if (field === "adjustmentId") return row.adjustmentId
    if (field === "adjustmentNo") return row.adjustmentNo
    return undefined
  })

  let total = 0
  switch (query.view) {
    case "balance":
      total = balances.length
      break
    case "movement":
      total = movements.length
      break
    case "reservation":
      total = reservations.length
      break
    case "adjustment":
      total = adjustments.length
      break
  }

  let emptyReason: InventoryListView["emptyReason"]
  if (total === 0) {
    if (scopedAll.length === 0) emptyReason = "NO_DATA"
    else if (hasActiveFilters || query.view !== "balance")
      emptyReason = "FILTER_NO_RESULT"
    else emptyReason = "NO_DATA"
  }

  const lastMovementWatermark =
    INVENTORY_MOVEMENT_SEED.map((m) => m.recordedAt).sort().at(-1) ?? ""

  const pageSize = Math.min(100, Math.max(1, Math.trunc(query.pageSize)))
  const requestedOffset = decodeInventoryCursor(query.cursor, query.view)
  const offset = requestedOffset < total ? requestedOffset : 0
  const cursor = offset === 0 ? "" : encodeInventoryCursor(query.view, offset)
  const nextOffset = offset + pageSize
  const nextCursor =
    nextOffset < total
      ? encodeInventoryCursor(query.view, nextOffset)
      : undefined
  const previousOffset = Math.max(0, offset - pageSize)
  const previousCursor =
    offset > 0
      ? previousOffset === 0
        ? ""
        : encodeInventoryCursor(query.view, previousOffset)
      : undefined

  // 只返回当前视图的服务端页，避免浏览器拿到其它视图全量数据再本地切页。
  const pageBalances =
    query.view === "balance" ? balances.slice(offset, offset + pageSize) : []
  const pageMovements =
    query.view === "movement" ? movements.slice(offset, offset + pageSize) : []
  const pageReservations =
    query.view === "reservation"
      ? reservations.slice(offset, offset + pageSize)
      : []
  const pageAdjustments =
    query.view === "adjustment"
      ? adjustments.slice(offset, offset + pageSize)
      : []

  return {
    view: query.view,
    metrics,
    balances: pageBalances,
    movements: pageMovements,
    reservations: pageReservations,
    adjustments: pageAdjustments,
    total,
    cursor,
    nextCursor,
    previousCursor,
    pageSize,
    sort,
    filterSummary: filterSummary(query, total),
    permissionVersion: PERMISSION_VERSION,
    dataWatermark: lastMovementWatermark,
    lastMovementWatermark,
    queriedAt: new Date().toISOString(),
    hasWarehouseScope: true,
    moduleAllowed: true,
    canCreateAdjustment: true,
    canExport: true,
    emptyReason,
    excludedKindsNote: EXCLUDED_NOTE,
    openingStockNote: OPENING_STOCK_NOTE,
    warehouses: [...INVENTORY_WAREHOUSES],
  }
}

export async function fetchBalanceDetail(
  balanceId: string
): Promise<BalanceDetailView | null> {
  await mockDelay(60)
  if (isInventoryPermissionRevoked()) return null
  const seed = INVENTORY_BALANCE_SEED.find((b) => b.balanceId === balanceId)
  if (!seed) return null
  const balance = projectBalance(seed)
  const recentMovements = INVENTORY_MOVEMENT_SEED.filter(
    (m) => m.balanceId === balanceId
  )
    .map(projectMovement)
    .sort((a, b) => b.occurredAt.localeCompare(a.occurredAt))
    .slice(0, 8)

  const reservations = INVENTORY_RESERVATION_SEED.filter(
    (r) => r.balanceId === balanceId && r.status !== "RELEASED"
  ).map(projectReservation)

  const sourceMap = new Map<string, BalanceDetailView["sourceDocuments"][number]>()
  for (const m of recentMovements) {
    const key = `${m.sourceDocumentType}:${m.sourceDocumentId}`
    if (sourceMap.has(key)) continue
    const link = sourceDocumentHref(
      m.sourceDocumentType as Parameters<typeof sourceDocumentHref>[0],
      m.sourceDocumentId
    )
    sourceMap.set(key, {
      documentType: m.sourceDocumentType,
      documentId: m.sourceDocumentId,
      documentNo: m.sourceDocumentNo,
      label: link.label,
      href:
        m.sourceDocumentType === "PURCHASE_RECEIPT" ||
        m.sourceDocumentType === "WAREHOUSE_DISPATCH"
          ? `/fulfillment?lane=warehouse&sourceDocId=${encodeURIComponent(m.sourceDocumentId)}&warehouseId=${encodeURIComponent(m.warehouseId)}&skuId=${encodeURIComponent(m.skuId)}`
          : link.href,
      workspaceId: link.workspaceId,
    })
  }

  const pendingAdjustments = allAdjustments().filter(
    (a) =>
      a.balanceId === balanceId &&
      a.status !== "POSTED" &&
      a.status !== "REJECTED"
  )

  return {
    balance,
    recentMovements,
    reservations,
    sourceDocuments: [...sourceMap.values()],
    pendingAdjustments,
    queriedAt: new Date().toISOString(),
  }
}

export async function createAdjustmentDraft(input: {
  balanceId: string
}): Promise<AdjustmentDraftView> {
  await mockDelay(80)
  const seed = INVENTORY_BALANCE_SEED.find((b) => b.balanceId === input.balanceId)
  if (!seed) {
    throw new Error("余额不存在")
  }
  if (isInventoryPermissionRevoked()) {
    throw new Error("权限已收回，无法创建调整草稿")
  }
  const lockVersion = getInventoryBalanceLockVersion(
    seed.balanceId,
    seed.lockVersion
  )
  const draft = createW10AdjustmentDraft({
    balanceId: seed.balanceId,
    warehouseId: seed.warehouseId,
    warehouseName: seed.warehouseName,
    skuId: seed.skuId,
    skuCode: seed.skuCode,
    skuName: seed.skuName,
    baseUnit: seed.baseUnit,
    balanceLockVersion: lockVersion,
  })
  return toDraftView(draft)
}

export async function saveAdjustmentDraft(input: {
  stockAdjustmentId: string
  expectedEditVersion: number
  reasonType: AdjustmentReasonType
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  note: string
  occurredAt: string
}): Promise<AdjustmentDraftView> {
  await mockDelay(60)
  const draft = saveW10AdjustmentDraft(input)
  return toDraftView(draft)
}

export async function submitAdjustment(input: {
  stockAdjustmentId: string
  expectedBalanceLockVersion: number
  seedBalanceLockVersion: number
  reasonType: AdjustmentReasonType
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  note: string
  occurredAt: string
  idempotencyKey: string
  forceUnknown?: boolean
}): Promise<AdjustmentSubmitResponse> {
  await mockDelay(150)
  return submitW10Adjustment(input)
}

export async function resolveAdjustmentUnknown(input: {
  idempotencyKey: string
  settle?: boolean
  settlePayload?: Parameters<typeof submitAdjustment>[0]
}): Promise<AdjustmentSubmitResponse> {
  await mockDelay(80)
  return resolveW10AdjustmentUnknown(input)
}

export async function getAdjustmentDraft(
  stockAdjustmentId: string
): Promise<AdjustmentDraftView | null> {
  await mockDelay(40)
  const draft = getW10AdjustmentDraft(stockAdjustmentId)
  return draft ? toDraftView(draft) : null
}

export async function startInventoryExport(input: {
  total: number
  filterSummary: string
}): Promise<W10ExportJob> {
  await mockDelay(50)
  if (isInventoryPermissionRevoked()) {
    throw new Error("权限已收回，无法导出")
  }
  return createW10ExportJob(input)
}

export async function fetchExportJob(jobId: string): Promise<W10ExportJob | null> {
  await mockDelay(40)
  return getW10ExportJob(jobId)
}

function toDraftView(draft: NonNullable<ReturnType<typeof getW10AdjustmentDraft>>): AdjustmentDraftView {
  return {
    stockAdjustmentId: draft.stockAdjustmentId,
    adjustmentNo: draft.adjustmentNo,
    balanceId: draft.balanceId,
    warehouseId: draft.warehouseId,
    warehouseName: draft.warehouseName,
    skuId: draft.skuId,
    skuCode: draft.skuCode,
    skuName: draft.skuName,
    baseUnit: draft.baseUnit,
    reasonType: draft.reasonType,
    reasonTypeLabel: draft.reasonTypeLabel,
    direction: draft.direction,
    quantity: draft.quantity,
    note: draft.note,
    occurredAt: draft.occurredAt,
    status: draft.status,
    statusLabel: draft.statusLabel,
    balanceLockVersion: draft.balanceLockVersion,
    editVersion: draft.editVersion,
    operatorLabel: draft.operatorLabel,
    segregationNote:
      "经办提交后进入仓储复核与财务确认，经办本人不得复核或过账；余额仅在过账完成后由系统更新。",
  }
}
