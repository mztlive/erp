/**
 * W01 履约任务作业面 · 当前单据的明细补全（队列列表投影 → 可编辑草稿）。
 * 补全失败时保留列表投影，不让明细缺失阻断整个队列。
 */

import type { FulfillmentOperation } from "@/features/fulfillment-operations/types"
import { stripDeliveryApprovalField } from "@/features/fulfillment-operations/lib/delivery-no-approval"
import { stripPurchaseReceiptApprovalField } from "@/features/fulfillment-operations/lib/purchase-receipt-no-approval"
import {
    emptySourceLine,
    nowIso,
} from "@/features/fulfillment-operations/lib/projection"
import { apiGet } from "@/lib/api"

import {
    deliveryToOperation,
    receiptToOperation,
    type BackendDeliveryDetail,
    type BackendPurchaseReceiptDetail,
} from "./documents"
import { enrichOperationDisplay } from "./hydrate-source"

function applyPurchaseReceiptDetail(
    operation: FulfillmentOperation,
    detail: BackendPurchaseReceiptDetail,
): FulfillmentOperation {
    // PurchaseReceipt 为 NO_APPROVAL，明细补全丢弃误带的审批字段。
    const receipt = stripPurchaseReceiptApprovalField(detail.receipt)
    const lines = detail.lines.map((line) =>
        emptySourceLine({
            lineId: line.id,
            salesOrderLineId: line.purchase_order_revision_line_id,
            purchaseRevisionLineId: line.purchase_order_revision_line_id,
            remainingQuantity: line.received_quantity,
            orderedQuantity: line.received_quantity,
        }),
    )
    const draftLines = detail.lines.map((line) => ({
        purchaseRevisionLineId: line.purchase_order_revision_line_id,
        receivedQuantity: line.received_quantity,
        qualifiedQuantity: line.qualified_quantity,
        rejectedQuantity: line.rejected_quantity,
        qualityResult: line.quality_result,
    }))
    return {
        ...operation,
        editVersion: receipt.version,
        sourceVersion: String(receipt.version),
        lines,
        draft: {
            type: "RECEIPT",
            warehouseId: receipt.warehouse_id,
            warehouseLabel: "",
            occurredAt:
                operation.draft.type === "RECEIPT"
                    ? operation.draft.occurredAt
                    : nowIso().slice(0, 16),
            lines: draftLines,
        },
    }
}

function applyDeliveryDetail(
    operation: FulfillmentOperation,
    detail: BackendDeliveryDetail,
): FulfillmentOperation {
    // Delivery 为 NO_APPROVAL，明细补全丢弃误带的审批字段。
    const delivery = stripDeliveryApprovalField(detail.delivery)
    const lines = detail.lines.map((line) =>
        emptySourceLine({
            lineId: line.id,
            salesOrderLineId: line.sales_order_line_id,
            remainingQuantity: line.quantity,
            orderedQuantity: line.quantity,
            stockReservationId: line.stock_reservation_id ?? undefined,
            reservedQuantity: line.stock_reservation_id
                ? line.quantity
                : undefined,
            purchaseLineSalesAllocationId:
                line.purchase_line_sales_allocation_id ?? undefined,
        }),
    )
    if (operation.operationType === "WAREHOUSE_SHIP") {
        return {
            ...operation,
            editVersion: delivery.version,
            sourceVersion: String(delivery.version),
            lines,
            draft: {
                type: "WAREHOUSE_SHIP",
                warehouseId: delivery.warehouse_id ?? "",
                warehouseLabel: "",
                carrier: delivery.carrier ?? "",
                trackingNo: delivery.tracking_no ?? "",
                shippedAt: nowIso().slice(0, 16),
                lines: detail.lines.map((line) => ({
                    salesOrderLineId: line.sales_order_line_id,
                    stockReservationId: line.stock_reservation_id ?? "",
                    quantity: line.quantity,
                })),
            },
        }
    }
    return {
        ...operation,
        editVersion: delivery.version,
        sourceVersion: String(delivery.version),
        lines,
        draft: {
            type: "SUPPLIER_DIRECT",
            carrier: delivery.carrier ?? "",
            trackingNo: delivery.tracking_no ?? "",
            shippedAt: nowIso().slice(0, 16),
            lines: detail.lines.map((line) => ({
                salesOrderLineId: line.sales_order_line_id,
                purchaseLineSalesAllocationId:
                    line.purchase_line_sales_allocation_id ?? "",
                quantity: line.quantity,
            })),
        },
    }
}

/** 把采购收货详情直接投影为 W01 可执行作业。 */
export async function receiptDetailToOperation(
    detail: BackendPurchaseReceiptDetail,
): Promise<FulfillmentOperation> {
    return enrichOperationDisplay(
        applyPurchaseReceiptDetail(receiptToOperation(detail.receipt), detail),
    )
}

/** 把发货详情直接投影为 W01 可执行作业。 */
export async function deliveryDetailToOperation(
    detail: BackendDeliveryDetail,
): Promise<FulfillmentOperation> {
    return enrichOperationDisplay(
        applyDeliveryDetail(deliveryToOperation(detail.delivery), detail),
    )
}

/**
 * 按作业类型补全当前单据明细。所有履约单据均为 NO_APPROVAL，明细不得携带审批绑定。
 *
 * @param operation 队列列表投影。
 * @returns 可编辑草稿；补全失败时保留原投影。
 */
export async function hydrateOperationDetail(
    operation: FulfillmentOperation,
): Promise<FulfillmentOperation> {
    try {
        if (operation.operationType === "RECEIPT") {
            const detail = await apiGet<BackendPurchaseReceiptDetail>(
                `/admin/purchase-receipts/${encodeURIComponent(operation.operationId)}`,
            )
            return enrichOperationDisplay(
                applyPurchaseReceiptDetail(operation, detail),
            )
        }
        if (
            operation.operationType === "WAREHOUSE_SHIP" ||
            operation.operationType === "SUPPLIER_DIRECT"
        ) {
            const detail = await apiGet<BackendDeliveryDetail>(
                `/admin/deliveries/${encodeURIComponent(operation.operationId)}`,
            )
            return enrichOperationDisplay(
                applyDeliveryDetail(operation, detail),
            )
        }
    } catch {
        // 保留列表投影，队列仍可继续展示。
    }
    return enrichOperationDisplay(operation)
}
