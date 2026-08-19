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
import { stripDeliveryApprovalField } from "@/features/fulfillment-operations/lib/delivery-no-approval"
import { stripElectronicDeliveryApprovalField } from "@/features/fulfillment-operations/lib/electronic-delivery-no-approval"
import { stripPurchaseReceiptApprovalField } from "@/features/fulfillment-operations/lib/purchase-receipt-no-approval"
import { stripServiceFulfillmentApprovalField } from "@/features/fulfillment-operations/lib/service-fulfillment-no-approval"

// ─── backend DTO shapes ─────────────────────────────────────────────────────

/** PurchaseReceipt 为 NO_APPROVAL：创建/详情 DTO 不得携带审批绑定。 */
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

/** PurchaseReceipt 为 NO_APPROVAL：详情不得嵌入审批区。 */
export type BackendPurchaseReceiptDetail = {
    receipt: BackendPurchaseReceipt
    lines: BackendPurchaseReceiptLine[]
}

/** Delivery 为 NO_APPROVAL：创建/详情 DTO 不得携带审批绑定。 */
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

/** Delivery 为 NO_APPROVAL：详情不得嵌入审批区。 */
export type BackendDeliveryDetail = {
    delivery: BackendDelivery
    lines: BackendDeliveryLine[]
}

/** ElectronicDelivery 为 NO_APPROVAL：创建/详情 DTO 不得携带审批绑定。 */
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

/** ServiceFulfillment 为 NO_APPROVAL：创建/详情 DTO 不得携带审批绑定。 */
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

/**
 * 把采购收货草稿投影为入库工作单。PurchaseReceipt 为 NO_APPROVAL，丢弃误带的审批字段。
 *
 * @param r 采购收货 HTTP 载荷。
 */
export function receiptToOperation(
    r: BackendPurchaseReceipt,
): FulfillmentOperation {
    const receipt = stripPurchaseReceiptApprovalField(r)
    const dueAt = secsToIso(receipt.created_at) || nowIso()
    return baseOperation({
        operationId: receipt.id,
        operationType: "RECEIPT",
        editVersion: receipt.version,
        sourceVersion: String(receipt.version),
        dueAt,
        summary: receipt.receipt_no,
        source: {
            purchaseOrderId: receipt.purchase_order_id,
            purchaseNo: receipt.purchase_order_id,
            purchaseRevisionId: "",
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
            warehouseId: receipt.warehouse_id,
            warehouseLabel: receipt.warehouse_id,
        },
        draft: {
            type: "RECEIPT",
            warehouseId: receipt.warehouse_id,
            warehouseLabel: receipt.warehouse_id,
            occurredAt: nowIso().slice(0, 16),
            lines: [],
        },
    })
}

/**
 * 把仓发草稿投影为仓发或直发工作单。Delivery 为 NO_APPROVAL，丢弃误带的审批字段。
 *
 * @param d 仓发 HTTP 载荷。
 */
export function deliveryToOperation(d: BackendDelivery): FulfillmentOperation {
    const delivery = stripDeliveryApprovalField(d)
    const op: FulfillmentOperationType =
        delivery.delivery_type === "SUPPLIER_DIRECT"
            ? "SUPPLIER_DIRECT"
            : "WAREHOUSE_SHIP"
    const dueAt = secsToIso(delivery.created_at) || nowIso()
    if (op === "WAREHOUSE_SHIP") {
        return baseOperation({
            operationId: delivery.id,
            operationType: "WAREHOUSE_SHIP",
            editVersion: delivery.version,
            sourceVersion: String(delivery.version),
            dueAt,
            summary: delivery.delivery_no,
            source: {
                purchaseOrderId: delivery.purchase_order_id ?? undefined,
                salesOrderId: delivery.sales_order_id,
                salesOrderNo: delivery.sales_order_id,
                salesRevisionId: "",
                customerLabel: "",
                warehouseId: delivery.warehouse_id ?? undefined,
                warehouseLabel: delivery.warehouse_id ?? undefined,
            },
            gate: { state: "SATISFIED", message: "" },
            draft: {
                type: "WAREHOUSE_SHIP",
                warehouseId: delivery.warehouse_id ?? "",
                warehouseLabel: delivery.warehouse_id ?? "",
                carrier: delivery.carrier ?? "",
                trackingNo: delivery.tracking_no ?? "",
                shippedAt: nowIso().slice(0, 16),
                lines: [],
            },
        })
    }
    return baseOperation({
        operationId: delivery.id,
        operationType: "SUPPLIER_DIRECT",
        editVersion: delivery.version,
        sourceVersion: String(delivery.version),
        dueAt,
        summary: delivery.delivery_no,
        source: {
            purchaseOrderId: delivery.purchase_order_id ?? undefined,
            salesOrderId: delivery.sales_order_id,
            salesOrderNo: delivery.sales_order_id,
            salesRevisionId: "",
            customerLabel: "",
        },
        draft: {
            type: "SUPPLIER_DIRECT",
            carrier: delivery.carrier ?? "",
            trackingNo: delivery.tracking_no ?? "",
            shippedAt: nowIso().slice(0, 16),
            lines: [],
        },
    })
}

/**
 * 把电子交付草稿投影为电子交付工作单。ElectronicDelivery 为 NO_APPROVAL，丢弃误带的审批字段。
 *
 * @param e 电子交付 HTTP 载荷。
 */
export function electronicToOperation(
    e: BackendElectronicDelivery,
): FulfillmentOperation {
    const electronic = stripElectronicDeliveryApprovalField(e)
    return baseOperation({
        operationId: electronic.id,
        operationType: "ELECTRONIC",
        editVersion: electronic.version,
        sourceVersion: String(electronic.version),
        dueAt: secsToIso(electronic.occurred_at) || nowIso(),
        summary: electronic.fulfillment_no,
        source: {
            purchaseOrderId: electronic.purchase_order_id,
            purchaseNo: electronic.purchase_order_id,
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        lines: [
            emptySourceLine({
                lineId: electronic.sales_order_line_id,
                salesOrderLineId: electronic.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    electronic.purchase_line_sales_allocation_id,
                remainingQuantity: electronic.quantity,
                orderedQuantity: electronic.quantity,
            }),
        ],
        draft: {
            type: "ELECTRONIC",
            occurredAt:
                secsToIso(electronic.occurred_at).slice(0, 16) ||
                nowIso().slice(0, 16),
            recipientMasked: "",
            result:
                (electronic.result as "SUCCESS" | "PARTIAL" | "FAILED") ||
                "SUCCESS",
            lines: [
                {
                    salesOrderLineId: electronic.sales_order_line_id,
                    purchaseLineSalesAllocationId:
                        electronic.purchase_line_sales_allocation_id,
                    quantity: electronic.quantity,
                },
            ],
        },
    })
}

/**
 * 把服务履约草稿投影为线下服务工作单。ServiceFulfillment 为 NO_APPROVAL，丢弃误带的审批字段。
 *
 * @param s 服务履约 HTTP 载荷。
 */
export function serviceToOperation(
    s: BackendServiceFulfillment,
): FulfillmentOperation {
    const service = stripServiceFulfillmentApprovalField(s)
    return baseOperation({
        operationId: service.id,
        operationType: "SERVICE",
        editVersion: service.version,
        sourceVersion: String(service.version),
        dueAt: secsToIso(service.occurred_at) || nowIso(),
        summary: service.fulfillment_no,
        source: {
            purchaseOrderId: service.purchase_order_id,
            purchaseNo: service.purchase_order_id,
            salesOrderId: "",
            salesOrderNo: "",
            salesRevisionId: "",
            customerLabel: "",
        },
        lines: [
            emptySourceLine({
                lineId: service.sales_order_line_id,
                salesOrderLineId: service.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    service.purchase_line_sales_allocation_id,
                remainingQuantity: service.quantity,
                orderedQuantity: service.quantity,
            }),
        ],
        draft: {
            type: "SERVICE",
            startedAt:
                secsToIso(service.occurred_at).slice(0, 16) ||
                nowIso().slice(0, 16),
            endedAt:
                secsToIso(service.occurred_at).slice(0, 16) ||
                nowIso().slice(0, 16),
            serviceLocation: "",
            result:
                (service.result as "SUCCESS" | "PARTIAL" | "FAILED") ||
                "SUCCESS",
            completionNote: "",
            lines: [
                {
                    salesOrderLineId: service.sales_order_line_id,
                    purchaseLineSalesAllocationId:
                        service.purchase_line_sales_allocation_id,
                    quantity: service.quantity,
                },
            ],
        },
    })
}
