/**
 * W09 履约单据处理 · 正式单据确认后的结果投影。
 * 确认命令见 ./commands；这里只把后端单据转成给界面展示的正式结果。
 */

import type {
    FulfillmentDraft,
    FulfillmentFormalOutcome,
} from "@/features/fulfillment-operations/types"
import { nowIso, secsToIso } from "@/features/fulfillment-operations/lib/projection"
import type {
    BackendDelivery,
    BackendPurchaseReceipt,
} from "./documents"

export function formalFromReceipt(
    receipt: BackendPurchaseReceipt,
    draft: Extract<FulfillmentDraft, { type: "RECEIPT" }>,
    operationId: string,
): FulfillmentFormalOutcome {
    return {
        kind: "POSTED",
        operationId,
        factType: "PURCHASE_RECEIPT",
        factId: receipt.id,
        factNo: receipt.receipt_no,
        formalStatus: receipt.status || "POSTED",
        occurredAt:
            secsToIso(receipt.posted_at) || draft.occurredAt || nowIso(),
        operationType: "RECEIPT",
        inventoryDelta: [],
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: false,
        acceptanceNextStep:
            "入库不等于验收。合格的货已入库并按销售单留好；等发货之后，再由销售去登记客户验收。",
        inventoryImpactSummary: "单据已确认；库存影响以库存台账为准。",
        reference: receipt.receipt_no,
        salesOrderId: "",
        salesOrderNo: "",
    }
}

export function formalFromDelivery(
    delivery: BackendDelivery,
    draft: Extract<
        FulfillmentDraft,
        { type: "WAREHOUSE_SHIP" } | { type: "SUPPLIER_DIRECT" }
    >,
    operationId: string,
): FulfillmentFormalOutcome {
    const isWh = draft.type === "WAREHOUSE_SHIP"
    return {
        kind: "POSTED",
        operationId,
        factType: "DELIVERY",
        factId: delivery.id,
        factNo: delivery.delivery_no,
        formalStatus: delivery.status || "SHIPPED",
        occurredAt:
            secsToIso(delivery.shipped_at) || draft.shippedAt || nowIso(),
        operationType: draft.type,
        inventoryDelta: [],
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: true,
        acceptanceNextStep: isWh
            ? "仓发记录已确认。物流签收不等于客户验收；请销售在客户验收登记。"
            : "供应商直发记录已确认，不影响自有库存。请销售在客户验收登记（物流签收≠验收）。",
        inventoryImpactSummary: isWh
            ? "发货单已确认；库存与留货影响以库存台账为准。"
            : "供应商直发不影响自有库存。",
        reference: delivery.delivery_no,
        salesOrderId: delivery.sales_order_id,
        salesOrderNo: delivery.sales_order_id,
    }
}
