"use client"

import type { Ref } from "react"
import type { FulfillmentDeliveryFormHandle } from "./fulfillment-delivery-form"

import type {
    FulfillmentDraft,
    FulfillmentOperationType,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

import { QuantityValue } from "@/components/business"
import {
    RESULT_LABEL,
    SERVICE_RESULT_LABEL,
} from "@/features/fulfillment-operations/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/**
 * 切到下一条时把光标放在「第一个真的要动手填的框」上。
 * 入库的数量是带出来的，聚焦后全选即可直接改写；
 * 仓发/直发的物流单号是空的；服务先选成功或失败。
 */
export const FIRST_INPUT_ID: Record<FulfillmentOperationType, string> = {
    RECEIPT: "fulfillment-operations-receipt-form-received-quantity-0",
    WAREHOUSE_SHIP: "",
    SUPPLIER_DIRECT: "",
    ELECTRONIC: "fulfillment-operations-electronic-form-quantity-0",
    SERVICE: "fulfillment-operations-service-form-result",
}

export function getFirstInputId(operation: FulfillmentOperation): string {
    switch (operation.operationType) {
        case "RECEIPT": {
            const first = operation.lines[0] as
                | { purchaseRevisionLineId?: string }
                | undefined
            const segment = first?.purchaseRevisionLineId
                ? toAutomationIdSegment(first.purchaseRevisionLineId)
                : "0"
            return `fulfillment-operations-receipt-form-received-quantity-${segment}`
        }
        case "WAREHOUSE_SHIP":
        case "SUPPLIER_DIRECT": {
            const draft = operation.draft
            if (
                (draft.type === "WAREHOUSE_SHIP" ||
                    draft.type === "SUPPLIER_DIRECT") &&
                draft.packing &&
                draft.addressSnapshot
            ) {
                const parcel = draft.packing.packages[0]
                if (parcel)
                    return `fulfillment-packing-package-${toAutomationIdSegment(parcel.id)}-carrier`
                const destination = draft.addressSnapshot.destinations.find(
                    (row) => !row.pending_address && row.items.length,
                )
                return destination
                    ? `fulfillment-packing-${toAutomationIdSegment(destination.key)}-${toAutomationIdSegment(destination.items[0].delivery_line_id)}-quantity`
                    : "fulfillment-packing-form"
            }
            const kind =
                operation.operationType === "WAREHOUSE_SHIP" ? "ship" : "direct"
            const line = operation.lines[0]
            return line
                ? `fulfillment-operations-${kind}-form-line-${toAutomationIdSegment(line.salesOrderLineId)}-${operation.lines.length === 1 ? "tracking-no" : "toggle"}`
                : ""
        }
        case "ELECTRONIC": {
            const first = operation.lines[0] as
                | { salesOrderLineId?: string }
                | undefined
            const segment = first?.salesOrderLineId
                ? toAutomationIdSegment(first.salesOrderLineId)
                : "0"
            return `fulfillment-operations-electronic-form-quantity-${segment}`
        }
        case "SERVICE":
            return "fulfillment-operations-service-form-result"
        default:
            return FIRST_INPUT_ID[operation.operationType] ?? ""
    }
}
import { FulfillmentDirectForm } from "./fulfillment-direct-form"
import { FulfillmentElectronicForm } from "./fulfillment-electronic-form"
import { FulfillmentReceiptForm } from "./fulfillment-receipt-form"
import { FulfillmentServiceForm } from "./fulfillment-service-form"
import { FulfillmentShipForm } from "./fulfillment-ship-form"

/**
 * 按作业类型分派受控表单。
 * 草稿是可辨识联合，分派后各表单拿到收窄类型，不再各自 narrow。
 * Delivery 为 NO_APPROVAL，仓发/直发只走业务表单，不接入审批区。
 * ElectronicDelivery 为 NO_APPROVAL，电子交付只走业务表单，不接入审批区。
 * ServiceFulfillment 为 NO_APPROVAL，服务履约只走业务表单，不接入审批区。
 * CustomerAcceptance 为 NO_APPROVAL，本页不处理客户验收草稿，也不接入审批区。
 */
export function FulfillmentDraftForm({
    operation,
    draft,
    onChange,
    disabled,
    deliveryRef,
}: {
    operation: FulfillmentOperation
    draft: FulfillmentDraft
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
    deliveryRef?: Ref<FulfillmentDeliveryFormHandle>
}) {
    switch (draft.type) {
        case "RECEIPT":
            return (
                <FulfillmentReceiptForm
                    operation={operation}
                    draft={draft}
                    onChange={onChange}
                    disabled={disabled}
                />
            )
        case "WAREHOUSE_SHIP":
            return (
                <FulfillmentShipForm
                    ref={deliveryRef}
                    operation={operation}
                    draft={draft}
                    onChange={onChange}
                    disabled={disabled}
                />
            )
        case "SUPPLIER_DIRECT":
            return (
                <FulfillmentDirectForm
                    ref={deliveryRef}
                    operation={operation}
                    draft={draft}
                    onChange={onChange}
                    disabled={disabled}
                />
            )
        case "ELECTRONIC":
            return (
                <FulfillmentElectronicForm
                    operation={operation}
                    draft={draft}
                    onChange={onChange}
                    disabled={disabled}
                />
            )
        case "SERVICE":
            return (
                <FulfillmentServiceForm
                    operation={operation}
                    draft={draft}
                    onChange={onChange}
                    disabled={disabled}
                />
            )
    }
}

export function FulfillmentDraftReadOnly({
    operation,
    draft,
}: {
    operation: FulfillmentOperation
    draft: FulfillmentDraft
}) {
    const delivery =
        draft.type === "WAREHOUSE_SHIP" || draft.type === "SUPPLIER_DIRECT"
            ? draft
            : undefined
    const note = "evidenceNote" in draft ? draft.evidenceNote : undefined
    return (
        <section
            aria-label="履约明细"
            className="flex flex-col gap-3 text-body-compact"
        >
            <ul className="divide-y rounded-lg border">
                {operation.lines.map((line) => {
                    const receipt =
                        draft.type === "RECEIPT"
                            ? draft.lines.find(
                                  (item) =>
                                      item.purchaseRevisionLineId ===
                                      line.purchaseRevisionLineId,
                              )
                            : undefined
                    const saved =
                        draft.type !== "RECEIPT"
                            ? draft.lines.find(
                                  (item) =>
                                      item.salesOrderLineId ===
                                      line.salesOrderLineId,
                              )
                            : undefined
                    return (
                        <li
                            key={line.lineId}
                            className="flex flex-col gap-1 px-4 py-3"
                        >
                            <span className="font-medium">{line.itemName}</span>
                            <span className="text-muted-foreground">
                                待处理{" "}
                                <QuantityValue
                                    value={line.remainingQuantity}
                                    unit={line.unitCode}
                                />
                            </span>
                            {receipt && (
                                <span>
                                    实收{" "}
                                    <QuantityValue
                                        value={receipt.receivedQuantity}
                                        unit={line.unitCode}
                                    />{" "}
                                    · 合格{" "}
                                    <QuantityValue
                                        value={receipt.qualifiedQuantity}
                                        unit={line.unitCode}
                                    />{" "}
                                    · 不合格{" "}
                                    <QuantityValue
                                        value={receipt.rejectedQuantity}
                                        unit={line.unitCode}
                                    />
                                </span>
                            )}
                            {saved && (
                                <span>
                                    已填数量{" "}
                                    <QuantityValue
                                        value={saved.quantity}
                                        unit={line.unitCode}
                                    />
                                </span>
                            )}
                            {receipt?.evidenceNote && (
                                <span>{receipt.evidenceNote}</span>
                            )}
                            {saved &&
                                "evidenceNote" in saved &&
                                typeof saved.evidenceNote === "string" &&
                                saved.evidenceNote && (
                                    <span>{saved.evidenceNote}</span>
                                )}
                        </li>
                    )
                })}
            </ul>
            {draft.type === "ELECTRONIC" && (
                <p>
                    接收方：{draft.recipientMasked || "—"} · 交付结果：
                    {RESULT_LABEL[draft.result]}
                </p>
            )}
            {draft.type === "SERVICE" && (
                <p>
                    服务地点：{draft.serviceLocation || "—"} · 服务结果：
                    {draft.result ? SERVICE_RESULT_LABEL[draft.result] : "—"}
                    {draft.completionNote ? ` · ${draft.completionNote}` : ""}
                </p>
            )}
            {note && <p>{note}</p>}
            {delivery?.addressSnapshot?.destinations.map((destination) => (
                <section
                    key={destination.key}
                    className="flex flex-col gap-1 rounded-lg border p-3"
                >
                    <p>
                        {destination.recipient} · {destination.phone}
                    </p>
                    <p>{destination.address}</p>
                    {delivery.packing?.packages
                        .filter(
                            (parcel) =>
                                parcel.destination_key === destination.key,
                        )
                        .map((parcel) => (
                            <p key={parcel.id}>
                                {parcel.carrier} ·{" "}
                                <span className="num">
                                    {parcel.tracking_no}
                                </span>
                            </p>
                        ))}
                </section>
            ))}
            {!delivery?.packing?.packages.length &&
                delivery?.trackingEntries?.map((entry) => (
                    <p
                        key={`${entry.salesOrderLineId}:${entry.carrier}:${entry.trackingNo}`}
                    >
                        {entry.carrier || "物流单号"} ·{" "}
                        <span className="num">{entry.trackingNo}</span>
                    </p>
                ))}
        </section>
    )
}
