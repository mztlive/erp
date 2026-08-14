/**
 * W09 履约单据处理 · 服务端 DTO 形状与「单据 → 工作单」的投影映射。
 * 只投影 DRAFT 单据，不得把单据 ID 冒充 work_item。
 * 队列请求见 ./queue，命令（保存/确认/复核）见 ./commands，明细补全见 ./hydrate。
 */

import type {
    FulfillmentOperation,
    FulfillmentOperationType,
} from "@/features/fulfillment-operations/types"
import {
    baseOperation,
    emptySourceLine,
    nowIso,
    secsToIso,
} from "@/features/fulfillment-operations/lib/projection"

// ─── backend DTO shapes ─────────────────────────────────────────────────────

export type BackendPurchaseReceipt = {
    id: string
    receipt_no: string
    purchase_order_id: string
    warehouse_id: string
    status: string
    posted_at?: number | null
    version: number
    created_at: number
}

export type BackendPurchaseReceiptLine = {
    id: string
    line_no: number
    purchase_order_revision_line_id: string
    received_quantity: string
    qualified_quantity: string
    rejected_quantity: string
    quality_result: string
}

export type BackendPurchaseReceiptDetail = {
    receipt: BackendPurchaseReceipt
    lines: BackendPurchaseReceiptLine[]
}

export type BackendDelivery = {
    id: string
    delivery_no: string
    delivery_type: string
    sales_order_id: string
    purchase_order_id?: string | null
    warehouse_id?: string | null
    status: string
    carrier?: string | null
    tracking_no?: string | null
    shipped_at?: number | null
    version: number
    created_at: number
}

export type BackendDeliveryLine = {
    id: string
    line_no: number
    sales_order_line_id: string
    quantity: string
    stock_reservation_id?: string | null
    purchase_line_sales_allocation_id?: string | null
}

export type BackendDeliveryDetail = {
    delivery: BackendDelivery
    lines: BackendDeliveryLine[]
}

export type BackendElectronicDelivery = {
    id: string
    fulfillment_no: string
    sales_order_line_id: string
    purchase_order_id: string
    purchase_line_sales_allocation_id: string
    quantity: string
    result: string
    status: string
    occurred_at: number
    recorded_at: number
    version: number
}

export type BackendServiceFulfillment = {
    id: string
    fulfillment_no: string
    sales_order_line_id: string
    purchase_order_id: string
    purchase_line_sales_allocation_id: string
    quantity: string
    result: string
    status: string
    occurred_at: number
    recorded_at: number
    version: number
}

export type BackendWarehouse = {
    id: string
    warehouse_code: string
}

// ─── DTO → 工作单投影 ───────────────────────────────────────────────────────

export function receiptToOperation(r: BackendPurchaseReceipt): FulfillmentOperation {
    const dueAt = secsToIso(r.created_at) || nowIso()
    return baseOperation({
        operationId: r.id,
        operationType: "RECEIPT",
        editVersion: r.version,
        sourceVersion: String(r.version),
        dueAt,
        summary: r.receipt_no,
        source: {
            purchaseOrderId: r.purchase_order_id,
            purchaseNo: r.purchase_order_id,
            purchaseRevisionId: "",
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
            warehouseId: r.warehouse_id,
            warehouseLabel: r.warehouse_id,
        },
        draft: {
            type: "RECEIPT",
            warehouseId: r.warehouse_id,
            warehouseLabel: r.warehouse_id,
            occurredAt: nowIso().slice(0, 16),
            lines: [],
        },
    })
}

export function deliveryToOperation(d: BackendDelivery): FulfillmentOperation {
    const op: FulfillmentOperationType =
        d.delivery_type === "SUPPLIER_DIRECT"
            ? "SUPPLIER_DIRECT"
            : "WAREHOUSE_SHIP"
    const dueAt = secsToIso(d.created_at) || nowIso()
    if (op === "WAREHOUSE_SHIP") {
        return baseOperation({
            operationId: d.id,
            operationType: "WAREHOUSE_SHIP",
            editVersion: d.version,
            sourceVersion: String(d.version),
            dueAt,
            summary: d.delivery_no,
            source: {
                purchaseOrderId: d.purchase_order_id ?? undefined,
                salesOrderId: d.sales_order_id,
                salesOrderNo: d.sales_order_id,
                salesRevisionId: "",
                customerLabel: "",
                warehouseId: d.warehouse_id ?? undefined,
                warehouseLabel: d.warehouse_id ?? undefined,
            },
            gate: { state: "SATISFIED", message: "" },
            draft: {
                type: "WAREHOUSE_SHIP",
                warehouseId: d.warehouse_id ?? "",
                warehouseLabel: d.warehouse_id ?? "",
                carrier: d.carrier ?? "",
                trackingNo: d.tracking_no ?? "",
                shippedAt: nowIso().slice(0, 16),
                lines: [],
            },
        })
    }
    return baseOperation({
        operationId: d.id,
        operationType: "SUPPLIER_DIRECT",
        editVersion: d.version,
        sourceVersion: String(d.version),
        dueAt,
        summary: d.delivery_no,
        source: {
            purchaseOrderId: d.purchase_order_id ?? undefined,
            salesOrderId: d.sales_order_id,
            salesOrderNo: d.sales_order_id,
            salesRevisionId: "",
            customerLabel: "",
        },
        draft: {
            type: "SUPPLIER_DIRECT",
            carrier: d.carrier ?? "",
            trackingNo: d.tracking_no ?? "",
            shippedAt: nowIso().slice(0, 16),
            lines: [],
        },
    })
}

export function electronicToOperation(
    e: BackendElectronicDelivery,
): FulfillmentOperation {
    return baseOperation({
        operationId: e.id,
        operationType: "ELECTRONIC",
        editVersion: e.version,
        sourceVersion: String(e.version),
        dueAt: secsToIso(e.occurred_at) || nowIso(),
        summary: e.fulfillment_no,
        source: {
            purchaseOrderId: e.purchase_order_id,
            purchaseNo: e.purchase_order_id,
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        lines: [
            emptySourceLine({
                lineId: e.sales_order_line_id,
                salesOrderLineId: e.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    e.purchase_line_sales_allocation_id,
                remainingQuantity: e.quantity,
                orderedQuantity: e.quantity,
            }),
        ],
        draft: {
            type: "ELECTRONIC",
            occurredAt:
                secsToIso(e.occurred_at).slice(0, 16) || nowIso().slice(0, 16),
            recipientMasked: "",
            result: (e.result as "SUCCESS" | "PARTIAL" | "FAILED") || "SUCCESS",
            lines: [
                {
                    salesOrderLineId: e.sales_order_line_id,
                    purchaseLineSalesAllocationId:
                        e.purchase_line_sales_allocation_id,
                    quantity: e.quantity,
                },
            ],
        },
    })
}

export function serviceToOperation(
    s: BackendServiceFulfillment,
): FulfillmentOperation {
    return baseOperation({
        operationId: s.id,
        operationType: "SERVICE",
        editVersion: s.version,
        sourceVersion: String(s.version),
        dueAt: secsToIso(s.occurred_at) || nowIso(),
        summary: s.fulfillment_no,
        source: {
            purchaseOrderId: s.purchase_order_id,
            purchaseNo: s.purchase_order_id,
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        lines: [
            emptySourceLine({
                lineId: s.sales_order_line_id,
                salesOrderLineId: s.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    s.purchase_line_sales_allocation_id,
                remainingQuantity: s.quantity,
                orderedQuantity: s.quantity,
            }),
        ],
        draft: {
            type: "SERVICE",
            startedAt:
                secsToIso(s.occurred_at).slice(0, 16) || nowIso().slice(0, 16),
            endedAt:
                secsToIso(s.occurred_at).slice(0, 16) || nowIso().slice(0, 16),
            serviceLocation: "",
            result: (s.result as "SUCCESS" | "PARTIAL" | "FAILED") || "SUCCESS",
            completionNote: "",
            lines: [
                {
                    salesOrderLineId: s.sales_order_line_id,
                    purchaseLineSalesAllocationId:
                        s.purchase_line_sales_allocation_id,
                    quantity: s.quantity,
                },
            ],
        },
    })
}
