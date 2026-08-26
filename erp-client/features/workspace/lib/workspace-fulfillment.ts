import type { FulfillmentQueueFilters } from "@/features/fulfillment-operations/api"
import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"

import type { WorkspaceWorkItem } from "../types"

export type WorkspaceFulfillmentDescriptor = Readonly<{
    role: FulfillmentQueueFilters["role"]
    operationTypes: readonly [FulfillmentOperationType]
}>

/**
 * 把 W01 履约任务的服务端身份解析为唯一操作类型。
 * 任务类型、对象类型、责任角色与原因码任一不一致时失败关闭。
 */
export function workspaceFulfillmentDescriptor(
    item: Pick<
        WorkspaceWorkItem,
        | "workItemType"
        | "handlerKey"
        | "businessObjectType"
        | "ownerRole"
        | "reasonCode"
    >,
): WorkspaceFulfillmentDescriptor | null {
    if (
        item.workItemType !== "FULFILLMENT_OPERATION" ||
        item.handlerKey !== "fulfillment_operation"
    ) {
        return null
    }

    const objectType = item.businessObjectType.trim().toLowerCase()
    const identity = `${objectType}:${item.ownerRole}:${item.reasonCode ?? ""}`
    switch (identity) {
        case "purchase_receipt:warehouse_inbound_handler:PURCHASE_RECEIPT_READY":
            return { role: "warehouse", operationTypes: ["RECEIPT"] }
        case "delivery:warehouse_outbound_handler:WAREHOUSE_DELIVERY_READY":
            return { role: "warehouse", operationTypes: ["WAREHOUSE_SHIP"] }
        case "delivery:purchase_order_owner:SUPPLIER_DIRECT_DELIVERY_READY":
            return {
                role: "procurement",
                operationTypes: ["SUPPLIER_DIRECT"],
            }
        case "electronic_delivery:purchase_order_owner:ELECTRONIC_DELIVERY_READY":
            return { role: "procurement", operationTypes: ["ELECTRONIC"] }
        case "service_fulfillment:purchase_order_owner:SERVICE_FULFILLMENT_READY":
            return { role: "procurement", operationTypes: ["SERVICE"] }
        default:
            return null
    }
}
