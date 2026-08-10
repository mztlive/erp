/**
 * W10 库存台账 · 真实 HTTP API。
 * 对接 /admin/stock-balances|movements|reservations|adjustments。
 * 分页/排序/筛选由服务端完成；api 层将 Page{items,total,page,page_size}
 * 映射为前端 InventoryListView（含游标兼容）。
 */

import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
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
import {
  AVAILABILITY_LABEL,
  MOVEMENT_TYPE_LABEL,
  REASON_TYPE_OPTIONS,
  VIEW_LABEL,
} from "@/features/inventory/types"

// ─── backend DTO shapes (snake_case from services inventory/dto.rs) ─────────

type BackendPage<T> = Page<T>

type BackendStockBalance = {
  id: string
  warehouse_id: string
  warehouse_code: string
  warehouse_name: string
  sku_id: string
  sku_code: string
  sku_name: string
  spec_summary?: string | null
  on_hand_quantity: string
  reserved_quantity: string
  available_quantity: string
  version: number
  last_movement_id?: string | null
  last_movement_at?: number | null
  has_active_reservation: boolean
}

type BackendStockMovement = {
  id: string
  warehouse_id: string
  sku_id: string
  movement_type: string
  direction: string
  quantity: string
  source_document_id: string
  source_line_id?: string | null
  occurred_at: number
  recorded_at: number
}

type BackendStockReservation = {
  id: string
  warehouse_id: string
  sku_id: string
  sales_order_line_id: string
  reserved_quantity: string
  consumed_quantity: string
  released_quantity: string
  status: string
  version: number
}

type BackendStockAdjustment = {
  id: string
  adjustment_no: string
  warehouse_id: string
  reason_type: string
  status: string
  prepared_by: string
  reviewed_by?: string | null
  finance_reviewed_by?: string | null
  version: number
  created_at: number
}

type BackendStockAdjustmentLine = {
  id: string
  sku_id: string
  quantity: string
  direction: string
}

type BackendStockAdjustmentDetail = {
  adjustment: BackendStockAdjustment
  lines: BackendStockAdjustmentLine[]
  posted_movements: BackendStockMovement[]
}

type BackendStockBalanceDetail = {
  balance: BackendStockBalance
  recent_movements: BackendStockMovement[]
  active_reservations: BackendStockReservation[]
  pending_adjustments: BackendStockAdjustment[]
}

type BackendWarehouse = {
  id: string
  warehouse_code: string
  status?: string
  created_at?: number
  version?: number
}

type BackendBackgroundJob = {
  id: string
  job_no: string
  job_type: string
  status: string
  total_count: number
  processed_count: number
  success_count: number
  result_file_asset_id?: string | null
  created_at: number
}

/** 导出任务视图（保留签名供页面消费）。 */
export type InventoryExportJob = {
  jobId: string
  status: "queued" | "running" | "succeeded" | "failed"
  total: number
  completed: number
  filterSummary: string
  createdAt: string
  downloadLabel?: string
}

const OPENING_STOCK_NOTE =
  "期初库存只能通过导入与期初的基准日实盘导入形成流水；旧商城的库存数量不作为 ERP 库存记录，本台账不会写入或展示旧商城库存数据。"

const EXCLUDED_NOTE =
  "供应商直发、电子交付、线下服务与实体卡不进入本台账的自有实物库存。"

const SEGREGATION_NOTE =
  "经办提交后进入仓储复核与财务确认，经办本人不得复核或确认入账；余额仅在确认入账完成后由系统更新。"

// ─── pure display helpers ───────────────────────────────────────────────────

function secsToIso(secs: number | null | undefined): string {
  if (secs == null || secs === 0) return ""
  return new Date(secs * 1000).toISOString()
}

function isApiError(error: unknown): error is ApiError {
  return (
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error
  )
}

function movementTypeLabel(code: string): string {
  return (
    MOVEMENT_TYPE_LABEL[code] ??
    MOVEMENT_TYPE_LABEL[frontendMovementType(code)] ??
    code
  )
}

/** Backend SCREAMING → frontend display code used by filters/labels. */
function frontendMovementType(backend: string): string {
  switch (backend) {
    case "PURCHASE_RECEIPT_IN":
      return "PURCHASE_RECEIPT"
    case "WAREHOUSE_SHIP_OUT":
      return "WAREHOUSE_DISPATCH"
    case "INITIAL":
      return "OPENING_IMPORT"
    case "STOCK_GAIN":
    case "STOCK_LOSS":
    case "DAMAGE":
      return "STOCK_ADJUSTMENT"
    default:
      return backend
  }
}

function backendMovementTypeFilter(
  types: string[] | undefined
): string | undefined {
  if (!types?.length) return undefined
  // backend accepts single movement_type; pick first mapped value
  const t = types[0]
  switch (t) {
    case "PURCHASE_RECEIPT":
      return "PURCHASE_RECEIPT_IN"
    case "WAREHOUSE_DISPATCH":
      return "WAREHOUSE_SHIP_OUT"
    case "OPENING_IMPORT":
      return "INITIAL"
    case "STOCK_ADJUSTMENT":
      return "STOCK_GAIN"
    default:
      return t
  }
}

function directionFrontend(
  d: string
): "increase" | "decrease" {
  const u = d.toUpperCase()
  if (u === "DECREASE" || u === "decrease") return "decrease"
  return "increase"
}

function availabilityOf(row: BackendStockBalance): StockBalanceRow["availability"] {
  if (row.has_active_reservation) return "reserved"
  if (row.available_quantity === "0" || row.available_quantity === "0.00") {
    return "zero"
  }
  return "positive"
}

function balanceStatus(row: BackendStockBalance): {
  statusLabel: string
  statusTone: StockBalanceRow["statusTone"]
} {
  const a = availabilityOf(row)
  if (a === "zero") {
    return { statusLabel: "零可用", statusTone: "warning" }
  }
  if (a === "reserved") {
    return { statusLabel: "有预占", statusTone: "info" }
  }
  return { statusLabel: "有可用", statusTone: "success" }
}

function mapBalance(b: BackendStockBalance): StockBalanceRow {
  const { statusLabel, statusTone } = balanceStatus(b)
  return {
    balanceId: b.id,
    warehouseId: b.warehouse_id,
    warehouseCode: b.warehouse_code,
    warehouseName: b.warehouse_name,
    skuId: b.sku_id,
    skuCode: b.sku_code,
    skuName: b.sku_name,
    specSummary: b.spec_summary ?? "",
    baseUnit: "", // backend_gap: unit not on StockBalanceView
    onHandQuantity: b.on_hand_quantity,
    reservedQuantity: b.reserved_quantity,
    availableQuantity: b.available_quantity,
    lockVersion: b.version,
    lastMovementId: b.last_movement_id ?? "",
    lastMovementAt: secsToIso(b.last_movement_at),
    lastMovementTypeLabel: "", // backend_gap
    availability: availabilityOf(b),
    statusLabel,
    statusTone,
    hasActiveReservation: b.has_active_reservation,
    stockKind: "OWN_PHYSICAL",
    allowedActions: ["CREATE_ADJUSTMENT", "VIEW_SOURCE"],
    actionBlockers: [],
  }
}

function mapMovement(
  m: BackendStockMovement,
  labels?: { warehouseName?: string; skuCode?: string; skuName?: string }
): StockMovementRow {
  const mt = frontendMovementType(m.movement_type)
  const dir = directionFrontend(m.direction)
  const sourceDocType =
    mt === "PURCHASE_RECEIPT"
      ? "PURCHASE_RECEIPT"
      : mt === "WAREHOUSE_DISPATCH"
        ? "WAREHOUSE_DISPATCH"
        : mt === "OPENING_IMPORT"
          ? "OPENING_IMPORT"
          : "STOCK_ADJUSTMENT"
  return {
    movementId: m.id,
    balanceId: `${m.warehouse_id}:${m.sku_id}`,
    warehouseId: m.warehouse_id,
    warehouseName: labels?.warehouseName ?? m.warehouse_id,
    skuId: m.sku_id,
    skuCode: labels?.skuCode ?? m.sku_id,
    skuName: labels?.skuName ?? m.sku_id,
    baseUnit: "",
    movementType: mt,
    movementTypeLabel: movementTypeLabel(mt),
    direction: dir,
    quantity: m.quantity,
    occurredAt: secsToIso(m.occurred_at),
    recordedAt: secsToIso(m.recorded_at),
    recordedByLabel: "", // backend_gap
    sourceDocumentType: sourceDocType,
    sourceDocumentId: m.source_document_id,
    sourceDocumentNo: m.source_document_id,
    sourceHref:
      sourceDocType === "PURCHASE_RECEIPT" || sourceDocType === "WAREHOUSE_DISPATCH"
        ? `/fulfillment?lane=warehouse&sourceDocId=${encodeURIComponent(m.source_document_id)}&warehouseId=${encodeURIComponent(m.warehouse_id)}&skuId=${encodeURIComponent(m.sku_id)}`
        : undefined,
  }
}

function reservationStatusLabel(status: string): {
  statusLabel: string
  statusTone: StockReservationRow["statusTone"]
} {
  switch (status) {
    case "ACTIVE":
      return { statusLabel: "有效", statusTone: "success" }
    case "PARTIALLY_CONSUMED":
      return { statusLabel: "部分消耗", statusTone: "warning" }
    case "CONSUMED":
    case "FULLY_CONSUMED":
      return { statusLabel: "已消耗", statusTone: "neutral" }
    case "RELEASED":
      return { statusLabel: "已释放", statusTone: "neutral" }
    default:
      return { statusLabel: status, statusTone: "neutral" }
  }
}

function mapReservation(r: BackendStockReservation): StockReservationRow {
  const { statusLabel, statusTone } = reservationStatusLabel(r.status)
  // established ≈ reserved + consumed + released when backend only exposes remaining reserved
  const remaining = r.reserved_quantity
  return {
    reservationId: r.id,
    balanceId: `${r.warehouse_id}:${r.sku_id}`,
    warehouseId: r.warehouse_id,
    warehouseName: r.warehouse_id, // backend_gap: no warehouse/sku names on reservation view
    skuId: r.sku_id,
    skuCode: r.sku_id,
    skuName: r.sku_id,
    baseUnit: "",
    salesOrderId: "", // backend_gap
    salesOrderNo: "",
    salesOrderLineId: r.sales_order_line_id,
    salesOrderLineLabel: r.sales_order_line_id,
    establishedQuantity: remaining, // best-effort; full established not returned
    consumedQuantity: r.consumed_quantity,
    releasedQuantity: r.released_quantity,
    remainingQuantity: remaining,
    status: r.status === "CONSUMED" ? "FULLY_CONSUMED" : r.status,
    statusLabel,
    statusTone,
    establishedAt: "", // backend_gap: no established_at
    fulfillmentHref: `/fulfillment?lane=warehouse&type=warehouse_ship&scope=mine&warehouseId=${encodeURIComponent(r.warehouse_id)}&from=W10&returnTo=${encodeURIComponent(`/inventory?warehouseId=${r.warehouse_id}&skuId=${r.sku_id}`)}`,
  }
}

function reasonTypeFrontend(backend: string): string {
  switch (backend) {
    case "STOCK_GAIN":
      return "COUNT_GAIN"
    case "STOCK_LOSS":
      return "COUNT_LOSS"
    case "DAMAGE":
      return "DAMAGE"
    default:
      return backend
  }
}

function reasonTypeBackend(frontend: AdjustmentReasonType): string {
  switch (frontend) {
    case "COUNT_GAIN":
      return "STOCK_GAIN"
    case "COUNT_LOSS":
      return "STOCK_LOSS"
    case "DAMAGE":
      return "DAMAGE"
    case "OTHER":
      // backend has no OTHER; map to DAMAGE and rely on note — documented gap
      return "DAMAGE"
  }
}

function reasonTypeLabel(frontendOrBackend: string): string {
  const fe = reasonTypeFrontend(frontendOrBackend)
  return (
    REASON_TYPE_OPTIONS.find((o) => o.value === fe)?.label ?? frontendOrBackend
  )
}

function reasonDirection(
  reason: string
): "increase" | "decrease" {
  const fe = reasonTypeFrontend(reason) as AdjustmentReasonType
  return (
    REASON_TYPE_OPTIONS.find((o) => o.value === fe)?.direction ?? "decrease"
  )
}

function adjustmentStatusMap(status: string): {
  status: string
  statusLabel: string
  statusTone: StockAdjustmentRow["statusTone"]
} {
  switch (status) {
    case "DRAFT":
      return { status: "DRAFT", statusLabel: "草稿", statusTone: "neutral" }
    case "PENDING_WAREHOUSE_REVIEW":
      return {
        status: "PENDING_WAREHOUSE_REVIEW",
        statusLabel: "待仓储复核",
        statusTone: "warning",
      }
    case "PENDING_FINANCE_REVIEW":
      return {
        status: "PENDING_FINANCE",
        statusLabel: "待财务确认",
        statusTone: "info",
      }
    case "POSTED":
      return { status: "POSTED", statusLabel: "已过账", statusTone: "success" }
    case "REJECTED":
      return { status: "REJECTED", statusLabel: "驳回", statusTone: "destructive" }
    case "REVERSED":
      return { status: "REVERSED", statusLabel: "已冲正", statusTone: "neutral" }
    default:
      return { status, statusLabel: status, statusTone: "neutral" }
  }
}

function mapAdjustment(
  a: BackendStockAdjustment,
  line?: BackendStockAdjustmentLine
): StockAdjustmentRow {
  const st = adjustmentStatusMap(a.status)
  const direction = line
    ? directionFrontend(line.direction)
    : reasonDirection(a.reason_type)
  return {
    adjustmentId: a.id,
    adjustmentNo: a.adjustment_no,
    balanceId: line
      ? `${a.warehouse_id}:${line.sku_id}`
      : a.warehouse_id,
    warehouseId: a.warehouse_id,
    warehouseName: a.warehouse_id, // backend_gap
    skuId: line?.sku_id ?? "",
    skuCode: line?.sku_id ?? "",
    skuName: line?.sku_id ?? "",
    baseUnit: "",
    reasonType: reasonTypeFrontend(a.reason_type),
    reasonTypeLabel: reasonTypeLabel(a.reason_type),
    direction,
    quantity: line?.quantity ?? "",
    status: st.status,
    statusLabel: st.statusLabel,
    statusTone: st.statusTone,
    operatorLabel: a.prepared_by,
    warehouseReviewerLabel: a.reviewed_by ?? undefined,
    financeConfirmerLabel: a.finance_reviewed_by ?? undefined,
    createdAt: secsToIso(a.created_at),
  }
}

function filterSummary(query: InventoryQuery, total: number, warehouses: { id: string; name: string }[]): string {
  const parts = [
    VIEW_LABEL[query.view],
    query.warehouseId
      ? (warehouses.find((w) => w.id === query.warehouseId)?.name ??
        query.warehouseId)
      : "全部仓库",
    query.availability && query.availability !== "all"
      ? AVAILABILITY_LABEL[query.availability]
      : "全部状态",
  ]
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  if (query.skuId) parts.push(`SKU ${query.skuId}`)
  if (query.movementType?.length) {
    parts.push(
      `流水类型 ${query.movementType
        .map((t) => MOVEMENT_TYPE_LABEL[t] ?? t)
        .join("、")}`
    )
  }
  if (query.view === "movement" && query.occurredFrom && query.occurredTo) {
    parts.push(`${query.occurredFrom} 至 ${query.occurredTo}`)
  }
  parts.push(`${total} 条`)
  return parts.join(" · ")
}

function dateToUnixStart(date: string | undefined): number | undefined {
  if (!date) return undefined
  const t = Date.parse(`${date}T00:00:00`)
  return Number.isFinite(t) ? Math.floor(t / 1000) : undefined
}

function dateToUnixEnd(date: string | undefined): number | undefined {
  if (!date) return undefined
  const t = Date.parse(`${date}T23:59:59`)
  return Number.isFinite(t) ? Math.floor(t / 1000) : undefined
}

function sortTokenToBackend(
  sort: string[],
  view: InventoryQuery["view"]
): { sort_by?: string; sort_dir?: "asc" | "desc" } {
  const token = sort[0]
  if (!token) {
    if (view === "balance") return { sort_by: "sku_id", sort_dir: "asc" }
    if (view === "movement") return { sort_by: "occurred_at", sort_dir: "desc" }
    if (view === "reservation") return { sort_by: "created_at", sort_dir: "desc" }
    return { sort_by: "created_at", sort_dir: "desc" }
  }
  const [field, dir] = token.split(":")
  const sort_dir = dir === "asc" ? "asc" : "desc"
  // map frontend field names to backend whitelist
  const map: Record<string, string> = {
    warehouseCode: "sku_id",
    skuCode: "sku_id",
    lastMovementAt: "created_at",
    occurredAt: "occurred_at",
    recordedAt: "recorded_at",
    movementId: "created_at",
    establishedAt: "created_at",
    reservationId: "created_at",
    salesOrderNo: "created_at",
    createdAt: "created_at",
    adjustmentId: "created_at",
    adjustmentNo: "adjustment_no",
  }
  return { sort_by: map[field] ?? field, sort_dir }
}

function pageFromCursor(
  cursor: string | undefined,
  view: InventoryQuery["view"],
  pageSize: number
): number {
  const offset = decodeInventoryCursor(cursor, view)
  return Math.floor(offset / pageSize) + 1
}

function cursorsFromPage(
  view: InventoryQuery["view"],
  page: number,
  pageSize: number,
  total: number
): { cursor: string; nextCursor?: string; previousCursor?: string } {
  const offset = (page - 1) * pageSize
  const cursor = offset === 0 ? "" : encodeInventoryCursor(view, offset)
  const nextOffset = offset + pageSize
  const nextCursor =
    nextOffset < total ? encodeInventoryCursor(view, nextOffset) : undefined
  const previousOffset = Math.max(0, offset - pageSize)
  const previousCursor =
    offset > 0
      ? previousOffset === 0
        ? ""
        : encodeInventoryCursor(view, previousOffset)
      : undefined
  return { cursor, nextCursor, previousCursor }
}

async function fetchWarehouses(): Promise<
  { id: string; code: string; name: string }[]
> {
  try {
    const page = await apiGet<BackendPage<BackendWarehouse>>("/admin/warehouses", {
      page: 1,
      page_size: 100,
      sort_by: "warehouse_code",
      sort_dir: "asc",
    })
    return page.items.map((w) => ({
      id: w.id,
      code: w.warehouse_code,
      name: w.warehouse_code, // WarehouseView has no display name; use code
    }))
  } catch {
    return []
  }
}

// ─── public API (stable export signatures) ──────────────────────────────────

export async function fetchInventoryList(
  query: InventoryQuery
): Promise<InventoryListView> {
  const pageSize = Math.min(100, Math.max(1, Math.trunc(query.pageSize)))
  const page = pageFromCursor(query.cursor, query.view, pageSize)
  const sort = query.sort.length > 0 ? query.sort : []
  const { sort_by, sort_dir } = sortTokenToBackend(sort, query.view)
  const warehouses = await fetchWarehouses()
  const hasWarehouseScope = warehouses.length > 0

  const emptyBase = (
    emptyReason: InventoryListView["emptyReason"],
    extras: Partial<InventoryListView> = {}
  ): InventoryListView => ({
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
    pageSize,
    sort: query.sort,
    filterSummary: extras.filterSummary ?? "",
    permissionVersion: "pv-real",
    dataWatermark: "",
    lastMovementWatermark: "",
    queriedAt: new Date().toISOString(),
    hasWarehouseScope,
    moduleAllowed: true,
    canCreateAdjustment: true,
    canExport: true,
    emptyReason,
    excludedKindsNote: EXCLUDED_NOTE,
    openingStockNote: OPENING_STOCK_NOTE,
    warehouses,
    ...extras,
  })

  if (!hasWarehouseScope) {
    return emptyBase("NO_DATA_SCOPE", {
      filterSummary: "未配置仓库数据范围",
      moduleAllowed: true,
      canCreateAdjustment: false,
      canExport: false,
    })
  }

  // Metrics: use server totals where available (no qty recompute)
  let balanceDimensionCount = 0
  let reservedDimensionCount = 0
  let zeroAvailableDimensionCount = 0
  let pendingAdjustmentCount = 0

  try {
    const [balPage, pendingWh, pendingFin] = await Promise.all([
      apiGet<BackendPage<BackendStockBalance>>("/admin/stock-balances", {
        page: 1,
        page_size: 1,
        warehouse_id: query.warehouseId,
        sku_id: query.skuId,
      }),
      apiGet<BackendPage<BackendStockAdjustment>>("/admin/stock-adjustments", {
        page: 1,
        page_size: 1,
        warehouse_id: query.warehouseId,
        status: "PENDING_WAREHOUSE_REVIEW",
      }),
      apiGet<BackendPage<BackendStockAdjustment>>("/admin/stock-adjustments", {
        page: 1,
        page_size: 1,
        warehouse_id: query.warehouseId,
        status: "PENDING_FINANCE_REVIEW",
      }),
    ])
    balanceDimensionCount = balPage.total
    // reserved/zero metrics require availability filters the backend lacks
    reservedDimensionCount = 0
    zeroAvailableDimensionCount = 0
    pendingAdjustmentCount = pendingWh.total + pendingFin.total
  } catch (error) {
    if (isApiError(error) && error.status === 403) {
      return emptyBase("PERMISSION_REVOKED", {
        filterSummary: "权限已收回",
        moduleAllowed: false,
        canCreateAdjustment: false,
        canExport: false,
        hasWarehouseScope: false,
      })
    }
    throw error
  }

  let balances: StockBalanceRow[] = []
  let movements: StockMovementRow[] = []
  let reservations: StockReservationRow[] = []
  let adjustments: StockAdjustmentRow[] = []
  let total = 0
  let dataWatermark = ""

  if (query.view === "balance") {
    // availability filter not on backend — documented gap; still pass warehouse/sku
    const res = await apiGet<BackendPage<BackendStockBalance>>(
      "/admin/stock-balances",
      {
        page,
        page_size: pageSize,
        warehouse_id: query.warehouseId,
        sku_id: query.skuId,
        sort_by,
        sort_dir,
      }
    )
    balances = res.items.map(mapBalance)
    // client-side availability narrow only when backend can't — mark as gap adaptation
    if (query.availability && query.availability !== "all") {
      balances = balances.filter((b) => {
        if (query.availability === "zero") return b.availableQuantity === "0"
        if (query.availability === "positive") return b.availableQuantity !== "0"
        if (query.availability === "reserved") return b.hasActiveReservation
        return true
      })
    }
    if (query.balanceId) {
      balances = balances.filter((b) => b.balanceId === query.balanceId)
    }
    total = res.total
    if (query.q?.trim()) {
      const q = query.q.trim().toUpperCase()
      balances = balances.filter((b) =>
        [b.skuCode, b.skuName, b.specSummary, b.warehouseName, b.warehouseCode]
          .join(" ")
          .toUpperCase()
          .includes(q)
      )
      // q filter not on backend — total becomes page-local (gap)
      total = balances.length
    }
  } else if (query.view === "movement") {
    const res = await apiGet<BackendPage<BackendStockMovement>>(
      "/admin/stock-movements",
      {
        page,
        page_size: pageSize,
        warehouse_id: query.warehouseId,
        sku_id: query.skuId,
        movement_type: backendMovementTypeFilter(query.movementType),
        occurred_from: dateToUnixStart(query.occurredFrom),
        occurred_to: dateToUnixEnd(query.occurredTo),
        sort_by: sort_by ?? "occurred_at",
        sort_dir: sort_dir ?? "desc",
      }
    )
    const whMap = new Map(warehouses.map((w) => [w.id, w.name]))
    movements = res.items.map((m) =>
      mapMovement(m, { warehouseName: whMap.get(m.warehouse_id) })
    )
    total = res.total
    dataWatermark =
      movements.map((m) => m.recordedAt).sort().at(-1) ?? ""
  } else if (query.view === "reservation") {
    const res = await apiGet<BackendPage<BackendStockReservation>>(
      "/admin/stock-reservations",
      {
        page,
        page_size: pageSize,
        warehouse_id: query.warehouseId,
        sku_id: query.skuId,
        sales_order_line_id: query.salesOrderLineId,
        sort_by: sort_by ?? "created_at",
        sort_dir: sort_dir ?? "desc",
      }
    )
    reservations = res.items.map(mapReservation)
    total = res.total
  } else {
    // adjustment
    const res = await apiGet<BackendPage<BackendStockAdjustment>>(
      "/admin/stock-adjustments",
      {
        page,
        page_size: pageSize,
        warehouse_id: query.warehouseId,
        sort_by: sort_by ?? "created_at",
        sort_dir: sort_dir ?? "desc",
      }
    )
    // hydrate lines for quantity/sku when possible (N+1 limited to page)
    adjustments = await Promise.all(
      res.items.map(async (a) => {
        try {
          const detail = await apiGet<BackendStockAdjustmentDetail>(
            `/admin/stock-adjustments/${encodeURIComponent(a.id)}`
          )
          const line = detail.lines[0]
          return mapAdjustment(detail.adjustment, line)
        } catch {
          return mapAdjustment(a)
        }
      })
    )
    if (query.adjustmentId) {
      adjustments = adjustments.filter(
        (a) => a.adjustmentId === query.adjustmentId
      )
    }
    if (query.skuId) {
      adjustments = adjustments.filter((a) => a.skuId === query.skuId)
    }
    total = res.total
  }

  const { cursor, nextCursor, previousCursor } = cursorsFromPage(
    query.view,
    page,
    pageSize,
    total
  )

  let emptyReason: InventoryListView["emptyReason"]
  if (total === 0) {
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
    emptyReason =
      hasActiveFilters || query.view !== "balance"
        ? "FILTER_NO_RESULT"
        : "NO_DATA"
  }

  return {
    view: query.view,
    metrics: {
      balanceDimensionCount,
      reservedDimensionCount,
      zeroAvailableDimensionCount,
      pendingAdjustmentCount,
    },
    balances,
    movements,
    reservations,
    adjustments,
    total,
    cursor,
    nextCursor,
    previousCursor,
    pageSize,
    sort: query.sort,
    filterSummary: filterSummary(query, total, warehouses),
    permissionVersion: "pv-real",
    dataWatermark,
    lastMovementWatermark: dataWatermark,
    queriedAt: new Date().toISOString(),
    hasWarehouseScope: true,
    moduleAllowed: true,
    canCreateAdjustment: true,
    canExport: true,
    emptyReason,
    excludedKindsNote: EXCLUDED_NOTE,
    openingStockNote: OPENING_STOCK_NOTE,
    warehouses,
  }
}

export async function fetchBalanceDetail(
  balanceId: string
): Promise<BalanceDetailView | null> {
  try {
    const detail = await apiGet<BackendStockBalanceDetail>(
      `/admin/stock-balances/${encodeURIComponent(balanceId)}`
    )
    const balance = mapBalance(detail.balance)
    const recentMovements = detail.recent_movements.map((m) =>
      mapMovement(m, {
        warehouseName: balance.warehouseName,
        skuCode: balance.skuCode,
        skuName: balance.skuName,
      })
    )
    const reservations = detail.active_reservations.map((r) => {
      const row = mapReservation(r)
      return {
        ...row,
        warehouseName: balance.warehouseName,
        skuCode: balance.skuCode,
        skuName: balance.skuName,
        balanceId: balance.balanceId,
      }
    })
    const sourceMap = new Map<
      string,
      BalanceDetailView["sourceDocuments"][number]
    >()
    for (const m of recentMovements) {
      const key = `${m.sourceDocumentType}:${m.sourceDocumentId}`
      if (sourceMap.has(key)) continue
      sourceMap.set(key, {
        documentType: m.sourceDocumentType,
        documentId: m.sourceDocumentId,
        documentNo: m.sourceDocumentNo,
        label: m.movementTypeLabel,
        href: m.sourceHref,
        workspaceId:
          m.sourceDocumentType === "PURCHASE_RECEIPT" ||
          m.sourceDocumentType === "WAREHOUSE_DISPATCH"
            ? "W09"
            : undefined,
      })
    }
    const pendingAdjustments = detail.pending_adjustments.map((a) =>
      mapAdjustment(a, {
        id: "",
        sku_id: balance.skuId,
        quantity: "",
        direction: "INCREASE",
      })
    )

    return {
      balance,
      recentMovements,
      reservations,
      sourceDocuments: [...sourceMap.values()],
      pendingAdjustments,
      queriedAt: new Date().toISOString(),
    }
  } catch (error) {
    if (isApiError(error) && (error.status === 404 || error.status === 403)) {
      return null
    }
    throw error
  }
}

function toDraftView(
  detail: BackendStockAdjustmentDetail,
  balanceLockVersion: number
): AdjustmentDraftView {
  const a = detail.adjustment
  const line = detail.lines[0]
  const reasonFe = reasonTypeFrontend(a.reason_type) as AdjustmentReasonType
  const st = adjustmentStatusMap(a.status)
  return {
    stockAdjustmentId: a.id,
    adjustmentNo: a.adjustment_no,
    balanceId: line
      ? `${a.warehouse_id}:${line.sku_id}`
      : a.warehouse_id,
    warehouseId: a.warehouse_id,
    warehouseName: a.warehouse_id,
    skuId: line?.sku_id ?? "",
    skuCode: line?.sku_id ?? "",
    skuName: line?.sku_id ?? "",
    baseUnit: "",
    reasonType: reasonFe,
    reasonTypeLabel: reasonTypeLabel(a.reason_type),
    direction: line
      ? directionFrontend(line.direction)
      : reasonDirection(a.reason_type),
    quantity: line?.quantity ?? "",
    note: "",
    occurredAt: secsToIso(a.created_at).slice(0, 16),
    status: st.status,
    statusLabel: st.statusLabel,
    balanceLockVersion,
    editVersion: a.version,
    operatorLabel: a.prepared_by,
    segregationNote: SEGREGATION_NOTE,
  }
}

export async function createAdjustmentDraft(input: {
  balanceId: string
}): Promise<AdjustmentDraftView> {
  const detail = await fetchBalanceDetail(input.balanceId)
  if (!detail) {
    const err: ApiError = {
      kind: "Http",
      message: "余额不存在",
      status: 404,
    }
    throw err
  }
  const b = detail.balance
  const adjustmentNo = `TZ${Date.now().toString(36).toUpperCase()}`
  // Create draft with a placeholder line (quantity 0 not allowed — use "1" placeholder until save)
  // Backend requires quantity > 0 and lines 1–100; draft created with default COUNT_LOSS/DAMAGE direction decrease qty "1"
  const created = await apiPost<BackendStockAdjustment>(
    "/admin/stock-adjustments",
    {
      adjustment_no: adjustmentNo,
      warehouse_id: b.warehouseId,
      reason_type: "STOCK_LOSS",
      lines: [
        {
          sku_id: b.skuId,
          quantity: "1",
          direction: "DECREASE",
        },
      ],
    }
  )
  const full = await apiGet<BackendStockAdjustmentDetail>(
    `/admin/stock-adjustments/${encodeURIComponent(created.id)}`
  )
  const draft = toDraftView(full, b.lockVersion)
  return {
    ...draft,
    balanceId: b.balanceId,
    warehouseName: b.warehouseName,
    skuCode: b.skuCode,
    skuName: b.skuName,
    baseUnit: b.baseUnit,
    quantity: "", // clear placeholder so user fills
  }
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
}): Promise<AdjustmentSubmitResponse> {
  try {
    // ensure reason is saved before submit
    const detail = await apiGet<BackendStockAdjustmentDetail>(
      `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`
    )
    if (detail.adjustment.status === "DRAFT" || detail.adjustment.status === "REJECTED") {
      await apiPut<BackendStockAdjustment>(
        `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`,
        {
          version: detail.adjustment.version,
          reason_type: reasonTypeBackend(input.reasonType),
        }
      )
    }
    // Submit requires reviewed_by (warehouse reviewer identity)
    const submitted = await apiPost<BackendStockAdjustment>(
      `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}/submit`,
      {
        reviewed_by: "warehouse_reviewer",
      }
    )
    return {
      status: "succeeded",
      outcome: {
        kind: "SUBMITTED_FOR_WAREHOUSE_REVIEW",
        stockAdjustmentId: submitted.id,
        adjustmentNo: submitted.adjustment_no,
        nextResponsible: "仓储复核",
        reference: submitted.adjustment_no,
        submittedAt: new Date().toISOString(),
        balanceLockVersion: input.expectedBalanceLockVersion,
      },
    }
  } catch (error) {
    if (isApiError(error)) {
      if (error.status === 409) {
        return {
          status: "failed",
          code: "BALANCE_LOCK_CONFLICT",
          message: "数据已变更，请刷新后重试",
          latestLockVersion: input.expectedBalanceLockVersion,
        }
      }
      // OutcomeUnknown from backend maps to HTTP 500 with specific message
      if (
        error.status === 500 &&
        typeof error.message === "string" &&
        error.message.includes("暂无法确认")
      ) {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
      return {
        status: "failed",
        code: String(error.status ?? "ERROR"),
        message: error.message,
      }
    }
    throw error
  }
}

export async function resolveAdjustmentUnknown(input: {
  idempotencyKey: string
  stockAdjustmentId?: string
  expectedBalanceLockVersion?: number
}): Promise<AdjustmentSubmitResponse> {
  if (input.stockAdjustmentId) {
    try {
      const detail = await apiGet<BackendStockAdjustmentDetail>(
        `/admin/stock-adjustments/${encodeURIComponent(input.stockAdjustmentId)}`
      )
      const st = adjustmentStatusMap(detail.adjustment.status)
      if (
        st.status === "PENDING_WAREHOUSE_REVIEW" ||
        st.status === "PENDING_FINANCE" ||
        st.status === "POSTED"
      ) {
        return {
          status: "succeeded",
          outcome: {
            kind: "SUBMITTED_FOR_WAREHOUSE_REVIEW",
            stockAdjustmentId: detail.adjustment.id,
            adjustmentNo: detail.adjustment.adjustment_no,
            nextResponsible: "仓储复核",
            reference: detail.adjustment.adjustment_no,
            submittedAt: secsToIso(detail.adjustment.created_at),
            balanceLockVersion:
              input.expectedBalanceLockVersion ??
              detail.adjustment.version,
          },
        }
      }
    } catch {
      // fall through
    }
  }
  return {
    status: "failed",
    code: "NO_PENDING",
    message: "未找到该任务号对应的处理中请求",
  }
}

export async function startInventoryExport(input: {
  total: number
  filterSummary: string
}): Promise<InventoryExportJob> {
  const now = Math.floor(Date.now() / 1000)
  const requestId = `inv-export-${now}-${Math.random().toString(36).slice(2, 8)}`
  const jobNo = `INV-EXP-${now}`
  const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
    job_no: jobNo,
    job_type: "export",
    domain_job_type: "INVENTORY_LEDGER_EXPORT",
    request_id: requestId,
    total_count: Math.max(1, input.total || 1),
    items: [
      {
        object_type: "INVENTORY_LEDGER",
        object_id: "filter",
        expected_hash: input.filterSummary.slice(0, 128),
      },
    ],
  })
  const statusMap: Record<string, InventoryExportJob["status"]> = {
    queued: "queued",
    pending: "queued",
    running: "running",
    succeeded: "succeeded",
    completed: "succeeded",
    failed: "failed",
    cancelled: "failed",
  }
  return {
    jobId: job.job_no || job.id,
    status: statusMap[job.status?.toLowerCase?.() ?? ""] ?? "queued",
    total: job.total_count ?? input.total,
    completed: job.processed_count ?? 0,
    filterSummary: input.filterSummary,
    createdAt: secsToIso(job.created_at) || new Date().toISOString(),
    downloadLabel: job.result_file_asset_id
      ? `库存台账导出-${job.job_no}`
      : undefined,
  }
}
