import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"
import { hasPermission } from "@/lib/permissions"

const TYPE_PERMISSIONS: Record<
    FulfillmentOperationType,
    { list: string; execute: readonly string[] }
> = {
    RECEIPT: {
        list: "purchase_receipt:list",
        execute: [
            "purchase_receipt:detail",
            "purchase_receipt:update",
            "purchase_receipt:post",
        ],
    },
    WAREHOUSE_SHIP: {
        list: "delivery:list",
        execute: ["delivery:detail", "delivery:update", "delivery:post"],
    },
    SUPPLIER_DIRECT: {
        list: "delivery:list",
        execute: ["delivery:detail", "delivery:update", "delivery:post"],
    },
    ELECTRONIC: {
        list: "electronic_delivery:list",
        execute: ["electronic_delivery:confirm"],
    },
    SERVICE: {
        list: "service_fulfillment:list",
        execute: ["service_fulfillment:confirm"],
    },
}

export function canListFulfillmentOperation(
    granted: readonly string[] | undefined | null,
    operationType: FulfillmentOperationType,
): boolean {
    return hasPermission(granted, TYPE_PERMISSIONS[operationType].list)
}

export function canExecuteFulfillmentOperation(
    granted: readonly string[] | undefined | null,
    operationType: FulfillmentOperationType,
): boolean {
    const required = TYPE_PERMISSIONS[operationType]
    return (
        hasPermission(granted, required.list) &&
        required.execute.every((permission) =>
            hasPermission(granted, permission),
        )
    )
}
