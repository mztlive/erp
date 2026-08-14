"use client"

import type {
    FulfillmentDraft,
    FulfillmentOperationType,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

/**
 * 切到下一条时把光标放在「第一个真的要动手填的框」上。
 * 入库的数量是带出来的，聚焦后全选即可直接改写；
 * 仓发/直发的物流单号是空的；服务的完成说明是空的且必填。
 */
export const FIRST_INPUT_ID: Record<FulfillmentOperationType, string> = {
    RECEIPT: "receipt-recv-0",
    WAREHOUSE_SHIP: "ship-tracking",
    SUPPLIER_DIRECT: "direct-tracking",
    ELECTRONIC: "el-qty-0",
    SERVICE: "service-note",
}
import { FulfillmentDirectForm } from "./fulfillment-direct-form"
import { FulfillmentElectronicForm } from "./fulfillment-electronic-form"
import { FulfillmentReceiptForm } from "./fulfillment-receipt-form"
import { FulfillmentServiceForm } from "./fulfillment-service-form"
import { FulfillmentShipForm } from "./fulfillment-ship-form"

/**
 * 按作业类型分派受控表单。
 * 草稿是可辨识联合，分派后各表单拿到收窄类型，不再各自 narrow。
 */
export function FulfillmentDraftForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
    draft: FulfillmentDraft
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
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
                    operation={operation}
                    draft={draft}
                    onChange={onChange}
                    disabled={disabled}
                />
            )
        case "SUPPLIER_DIRECT":
            return (
                <FulfillmentDirectForm
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
