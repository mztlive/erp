/**
 * W10 库存台账 · 后端 DTO → 前端视图映射。
 * 只做形状/文案转换，不发请求；文案映射见 display.ts。
 */

import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
    type DocumentApprovalViewDto,
} from "@/features/approval-workflow/types"
import type {
    AdjustmentDetailView,
    AdjustmentDraftView,
    AdjustmentReasonType,
    StockAdjustmentRow,
    StockBalanceRow,
    StockMovementRow,
    StockReservationRow,
} from "@/features/inventory/types"
import {
    adjustmentStatusMap,
    directionFrontend,
    frontendMovementType,
    movementTypeLabel,
    reasonDirection,
    reasonTypeFrontend,
    reasonTypeLabel,
    reservationStatusLabel,
    secsToIso,
    SEGREGATION_NOTE,
} from "@/features/inventory/api/display"
import type {
    BackendStockAdjustment,
    BackendStockAdjustmentDetail,
    BackendStockAdjustmentLine,
    BackendStockBalance,
    BackendStockMovement,
    BackendStockReservation,
} from "@/features/inventory/api/dto"

function availabilityOf(
    row: BackendStockBalance,
): StockBalanceRow["availability"] {
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

export function mapBalance(b: BackendStockBalance): StockBalanceRow {
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

export function mapMovement(
    m: BackendStockMovement,
    labels?: { warehouseName?: string; skuCode?: string; skuName?: string },
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
        sourceDocumentNo: m.source_document_no ?? m.source_document_id,
        sourceHref:
            sourceDocType === "PURCHASE_RECEIPT" ||
            sourceDocType === "WAREHOUSE_DISPATCH"
                ? `/fulfillment?lane=warehouse&sourceDocId=${encodeURIComponent(m.source_document_id)}&warehouseId=${encodeURIComponent(m.warehouse_id)}&skuId=${encodeURIComponent(m.sku_id)}`
                : undefined,
    }
}

export function mapReservation(
    r: BackendStockReservation,
): StockReservationRow {
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

/**
 * 把单据详情上的只读审批结构转成通用审批区投影。
 *
 * 缺省时返回未绑定的空结构，禁止前端补默认审批人或节点。
 */
export function mapAdjustmentApproval(
    dto?: DocumentApprovalViewDto | null,
): DocumentApprovalView {
    if (!dto) {
        return {
            requirement: "PROCESS_REQUIRED",
            recentHistory: [],
            historyHasMore: false,
            allowedActions: [],
        }
    }
    return mapDocumentApprovalViewDto(dto)
}

export function mapAdjustment(
    a: BackendStockAdjustment,
    line?: BackendStockAdjustmentLine,
    approval?: DocumentApprovalView,
): StockAdjustmentRow {
    const st = adjustmentStatusMap(a.status)
    const direction = line
        ? directionFrontend(line.direction)
        : reasonDirection(a.reason_type)
    return {
        adjustmentId: a.id,
        adjustmentNo: a.adjustment_no,
        balanceId: line ? `${a.warehouse_id}:${line.sku_id}` : a.warehouse_id,
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
        currentNodeLabel:
            approval?.instance?.currentNodeName ??
            approval?.instance?.currentNode,
        currentAssigneeLabel:
            approval?.instance?.currentAssigneeName ??
            approval?.instance?.currentAssignee,
        createdAt: secsToIso(a.created_at),
    }
}

export function toDraftView(
    detail: BackendStockAdjustmentDetail,
    balanceLockVersion: number,
): AdjustmentDraftView {
    const a = detail.adjustment
    const line = detail.lines[0]
    const reasonFe = reasonTypeFrontend(a.reason_type) as AdjustmentReasonType
    const st = adjustmentStatusMap(a.status)
    return {
        stockAdjustmentId: a.id,
        adjustmentNo: a.adjustment_no,
        balanceId: line ? `${a.warehouse_id}:${line.sku_id}` : a.warehouse_id,
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
        approval: mapAdjustmentApproval(detail.approval),
    }
}

/**
 * 把调整单详情转成页面只读视图，审批事实只透传服务端投影。
 */
export function toAdjustmentDetailView(
    detail: BackendStockAdjustmentDetail,
): AdjustmentDetailView {
    const approval = mapAdjustmentApproval(detail.approval)
    return {
        adjustment: mapAdjustment(detail.adjustment, detail.lines[0], approval),
        approval,
        queriedAt: new Date().toISOString(),
    }
}
