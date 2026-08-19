/**
 * W09 履约单据处理 · 正式单据确认后的结果投影。
 * 确认命令见 ./commands；这里只把后端单据转成给界面展示的正式结果。
 */

import type {
    FulfillmentDraft,
    FulfillmentFormalOutcome,
} from "@/features/fulfillment-operations/types"
import {
    nowIso,
    secsToIso,
} from "@/features/fulfillment-operations/lib/projection"
import { stripDeliveryApprovalField } from "@/features/fulfillment-operations/lib/delivery-no-approval"
import { stripPurchaseReceiptApprovalField } from "@/features/fulfillment-operations/lib/purchase-receipt-no-approval"
import type { BackendDelivery, BackendPurchaseReceipt } from "./documents"

/**
 * 把已确认的采购收货投影为创建结果。PurchaseReceipt 为 NO_APPROVAL，
 * 丢弃误带的审批字段，结果面板不得渲染绑定卡或审批历史。
 *
 * @param receipt 采购收货 HTTP 载荷。
 * @param draft 入库草稿。
 * @param operationId 当前工作单 ID。
 */
export function formalFromReceipt(
    receipt: BackendPurchaseReceipt,
    draft: Extract<FulfillmentDraft, { type: "RECEIPT" }>,
    operationId: string,
): FulfillmentFormalOutcome {
    const posted = stripPurchaseReceiptApprovalField(receipt)
    return {
        kind: "POSTED",
        operationId,
        factType: "PURCHASE_RECEIPT",
        factId: posted.id,
        factNo: posted.receipt_no,
        formalStatus: posted.status || "POSTED",
        occurredAt: secsToIso(posted.posted_at) || draft.occurredAt || nowIso(),
        operationType: "RECEIPT",
        inventoryDelta: [],
        reservationDelta: [],
        remainingByLine: [],
        acceptanceRequired: false,
        acceptanceNextStep:
            "入库不等于验收。合格的货已入库并按销售单留好；等发货之后，再由销售去登记客户验收。",
        inventoryImpactSummary: "单据已确认；库存影响以库存台账为准。",
        reference: posted.receipt_no,
        salesOrderId: "",
        salesOrderNo: "",
    }
}

/**
 * 把已确认的仓发投影为创建结果。Delivery 为 NO_APPROVAL，
 * 丢弃误带的审批字段，结果面板不得渲染绑定卡或审批历史。
 *
 * @param delivery 仓发 HTTP 载荷。
 * @param draft 仓发或直发草稿。
 * @param operationId 当前工作单 ID。
 */
export function formalFromDelivery(
    delivery: BackendDelivery,
    draft: Extract<
        FulfillmentDraft,
        { type: "WAREHOUSE_SHIP" } | { type: "SUPPLIER_DIRECT" }
    >,
    operationId: string,
): FulfillmentFormalOutcome {
    const posted = stripDeliveryApprovalField(delivery)
    const isWh = draft.type === "WAREHOUSE_SHIP"
    return {
        kind: "POSTED",
        operationId,
        factType: "DELIVERY",
        factId: posted.id,
        factNo: posted.delivery_no,
        formalStatus: posted.status || "SHIPPED",
        occurredAt: secsToIso(posted.shipped_at) || draft.shippedAt || nowIso(),
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
        reference: posted.delivery_no,
        salesOrderId: posted.sales_order_id,
        salesOrderNo: posted.sales_order_id,
    }
}
