import {
    deliveryAddressIdentityKey,
    type DeliveryAllocationInputRow,
} from "@/lib/delivery-allocation-summary"

import type { ApprovalPurchaseFulfillment } from "./api/materials"

/** 统计单行明细的独立收货点数量，不暴露具体收货人、电话或街道地址。 */
export function countLineDeliveryPoints(
    allocations?: readonly DeliveryAllocationInputRow[] | null,
): number {
    if (!allocations?.length) return 0
    const keys = new Set<string>()
    for (const row of allocations) {
        keys.add(deliveryAddressIdentityKey(row))
    }
    return keys.size
}

/** 统计整张单据的独立收货点数量，用于审批及抄送视图的轻量数量提示。 */
export function countUniqueDeliveryPoints(
    allocationsList: readonly (
        | readonly DeliveryAllocationInputRow[]
        | null
        | undefined
    )[],
): number {
    const keys = new Set<string>()
    for (const allocations of allocationsList) {
        if (!allocations?.length) continue
        for (const row of allocations) {
            keys.add(deliveryAddressIdentityKey(row))
        }
    }
    return keys.size
}

/** 组装采购履约方式与收货点数量提示（不含详细收货地址）。 */
export function formatPurchaseFulfillmentSummary(
    fulfillment?: ApprovalPurchaseFulfillment | null,
    uniquePointCount = 0,
): string | null {
    const pointHint =
        uniquePointCount > 0 ? `${uniquePointCount} 处收货点` : null

    if (!fulfillment) {
        return pointHint
    }

    switch (fulfillment.responsibility) {
        case "WAREHOUSE": {
            const base = fulfillment.target_warehouse_name
                ? `入仓 · ${fulfillment.target_warehouse_name}`
                : "采购入库"
            return pointHint ? `${base} · ${pointHint}` : base
        }
        case "SUPPLIER_DIRECT":
            return pointHint ? `供应商直发 · ${pointHint}` : "供应商直发"
        case "ELECTRONIC":
            return "电子交付"
        case "SERVICE":
            return pointHint ? `线下服务 · ${pointHint}` : "线下服务"
        case "NONE":
            return "费用单（无需履约）"
        default:
            return pointHint
    }
}
