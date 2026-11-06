import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { PurchaseDeliveryAllocation } from "../../types"
import {
    deliveryKey,
    editableDeliveryAllocations,
    remainingDeliveryAllocations,
    supplierDeliveryRequired,
} from "./delivery"
import type { SourcingLineInput, SourcingProductLine } from "./types"

/** 有明确交付地址的供应商送达来源，以地址数量作为分配数量的唯一输入。 */
export function usesAddressQuantity(
    product: SourcingProductLine,
    line: SourcingLineInput,
): boolean {
    return (
        product.deliveryAllocations.length > 0 &&
        supplierDeliveryRequired(product, line)
    )
}

/** 旧草稿中的零数量地址不再展示；保留未完成或非法输入，交由校验提示。 */
export function selectedDeliveryRows(
    product: SourcingProductLine,
    line: SourcingLineInput,
): PurchaseDeliveryAllocation[] {
    return editableDeliveryAllocations(product, line).filter((row) => {
        try {
            return (
                compareDecimal(compactFixed(row.quantity.trim()), "0", 6) !== 0
            )
        } catch {
            return true
        }
    })
}

/** 未完成数量不伪装成有效合计；空地址列表的分配数量为零。 */
export function deliveryQuantity(
    rows: readonly PurchaseDeliveryAllocation[],
): string {
    try {
        return compactFixed(
            sumFixed(
                rows.map((row) => compactFixed(row.quantity.trim())),
                { maxScale: 6, outputScale: 6 },
            ),
        )
    } catch {
        return ""
    }
}

/** 可分配上限扣除已生效采购和其他参与来源，不扣除当前来源自己的数量。 */
export function availableDeliveryRows(
    product: SourcingProductLine,
    line: SourcingLineInput,
    lines: readonly SourcingLineInput[],
): PurchaseDeliveryAllocation[] {
    const capacities = new Map(
        remainingDeliveryAllocations(product).map((row) => [
            deliveryKey(row),
            row,
        ]),
    )
    for (const other of lines) {
        if (
            !other.selected ||
            other.rowKey === line.rowKey ||
            other.salesOrderLineId !== line.salesOrderLineId ||
            !supplierDeliveryRequired(product, other)
        )
            continue
        for (const row of editableDeliveryAllocations(product, other)) {
            const key = deliveryKey(row)
            const cap = capacities.get(key)
            if (!cap) continue
            try {
                const quantity = compactFixed(row.quantity.trim())
                if (compareDecimal(quantity, "0", 6) <= 0) continue
                const left = subtractFixed(cap.quantity, quantity, {
                    maxScale: 6,
                    outputScale: 6,
                })
                capacities.set(key, {
                    ...cap,
                    quantity:
                        compareDecimal(left, "0", 6) > 0
                            ? compactFixed(left)
                            : "0",
                })
            } catch {
                // 其他来源尚未填完该地址时，先禁止继续占用，避免把未知数量当作零。
                capacities.set(key, { ...cap, quantity: "0" })
            }
        }
    }
    return [...capacities.values()]
}
