import { compactFixed } from "@/lib/fixed-decimal"
import { directDeliveryCapacity } from "./delivery"
import { minimumUnits, quantityUnits, formatQuantityUnits } from "./quantity"
import type { SourcingSalesOrder, SourcingSupplierOption } from "./types"

/**
 * 集采直发选项的起订说明与可选状态。
 * 按当前销售单同一供给的剩余可配送量判断，不按正在编辑的单行数量禁用，
 * 避免阻止多行合并达到起订量；最终实际分配量仍由表单和服务端校验。
 */
export function bulkMinimumOptionState(
    order: SourcingSalesOrder,
    option: SourcingSupplierOption,
): { label: string; disabled: boolean } {
    const minimum = option.delivery?.bulkMinimumOrderQuantity
    const offeringId = option.delivery?.supplierOfferingId
    if (
        option.sourceType !== "PURCHASE" ||
        option.fulfillmentResponsibility !== "SUPPLIER_DIRECT" ||
        option.priceBasis !== "BULK" ||
        !minimum ||
        !offeringId
    )
        return { label: "", disabled: false }

    let capacity = BigInt(0)
    let unit = ""
    for (const product of order.lines) {
        const candidate = product.options.find(
            (entry) =>
                entry.basisId === option.basisId &&
                entry.delivery?.supplierOfferingId === offeringId,
        )
        if (!candidate) continue
        unit = product.unit
        const deliveryCapacity = directDeliveryCapacity(product, candidate)
        capacity += minimumUnits(
            quantityUnits(product.remainingQuantity),
            quantityUnits(candidate.maxCreateQuantity),
            quantityUnits(deliveryCapacity ?? product.remainingQuantity),
        )
    }
    const disabled = capacity < quantityUnits(minimum)
    const label = `${compactFixed(minimum)} ${unit}起订`
    return {
        label: disabled
            ? `${label}（本单最多可分配 ${formatQuantityUnits(capacity)} ${unit}，不足起订量）`
            : label,
        disabled,
    }
}
