import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { PurchaseDeliveryAllocation } from "../../types"
import { sourcingUnitQuantityError } from "../sourcing-quantity"
import { addressDeliverable } from "./delivery-terms"
import { findSourcingOption } from "./options"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "./types"

export function deliveryKey(row: PurchaseDeliveryAllocation): string {
    return JSON.stringify([
        row.recipient,
        row.phone,
        row.region_code,
        row.address,
    ])
}

function groupDeliveryAllocations(
    rows: readonly PurchaseDeliveryAllocation[],
): Map<string, PurchaseDeliveryAllocation> {
    const groups = new Map<string, PurchaseDeliveryAllocation>()
    for (const row of rows) {
        const key = deliveryKey(row)
        const existing = groups.get(key)
        groups.set(key, {
            ...row,
            quantity: sumFixed([existing?.quantity ?? "0", row.quantity], {
                maxScale: 6,
                outputScale: 6,
            }),
        })
    }
    return groups
}

/** 同一收件资料合并展示；当前采购已承担数量从销售需求中扣减。 */
export function remainingDeliveryAllocations(
    product: SourcingProductLine,
): PurchaseDeliveryAllocation[] {
    const groups = groupDeliveryAllocations(product.deliveryAllocations)
    for (const row of product.assignedDeliveryAllocations) {
        const existing = groups.get(deliveryKey(row))
        if (existing)
            existing.quantity = subtractFixed(existing.quantity, row.quantity, {
                maxScale: 6,
                outputScale: 6,
            })
    }
    return [...groups.values()].map((row) => ({
        ...row,
        quantity:
            compareDecimal(row.quantity, "0", 6) < 0
                ? "0"
                : compactFixed(row.quantity),
    }))
}

/** 销售要求按收件资料合并，同时给出扣除已有采购承担后的剩余数量。 */
export function deliveryRequirementRows(
    product: SourcingProductLine,
): (PurchaseDeliveryAllocation & { remaining: string })[] {
    const remaining = remainingDeliveryAllocations(product)
    return [
        ...groupDeliveryAllocations(product.deliveryAllocations).values(),
    ].map((row, index) => ({
        ...row,
        quantity: compactFixed(row.quantity),
        remaining: remaining[index]?.quantity ?? "0",
    }))
}

/** 本方案可配送的剩余地址恰好承担本次数量时，地址数量按销售要求默认带入。 */
export function defaultDeliveryCoversQuantity(
    product: SourcingProductLine,
    input: SourcingLineInput,
): boolean {
    const rows = editableDeliveryAllocations(product, {
        ...input,
        deliveryAllocations: undefined,
    })
    try {
        return (
            compareDecimal(
                sumFixed(
                    rows.map((row) => row.quantity),
                    { maxScale: 6, outputScale: 6 },
                ),
                input.quantity,
                6,
            ) === 0 && compareDecimal(input.quantity, "0", 6) > 0
        )
    } catch {
        return false
    }
}

/** 本次数量恰好承担全部剩余地址需求时，地址数量可按销售要求默认带入。 */
export function deliveryDefaultsCoverQuantity(
    product: SourcingProductLine,
    quantity: string,
): boolean {
    const total = sumFixed(
        remainingDeliveryAllocations(product).map((row) => row.quantity),
        { maxScale: 6, outputScale: 6 },
    )
    try {
        return compareDecimal(total, quantity, 6) === 0
    } catch {
        return false
    }
}

export function supplierDeliveryRequired(
    product: SourcingProductLine,
    input: SourcingLineInput,
): boolean {
    const option = findSourcingOption(product, input.basisId)
    return (
        option?.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility !== "WAREHOUSE"
    )
}

/**
 * 仅全量承担本方案可配送的剩余地址时默认带入；部分承担必须逐地址确认。
 * 不在本方案配送范围内的地址始终为 0，不得分配。
 */
export function editableDeliveryAllocations(
    product: SourcingProductLine,
    input: SourcingLineInput,
): PurchaseDeliveryAllocation[] {
    if (input.deliveryAllocations) return input.deliveryAllocations
    const option = findSourcingOption(product, input.basisId)
    const rows = remainingDeliveryAllocations(product)
    const deliverable = rows.filter((row) =>
        addressDeliverable(option, row.region_code),
    )
    // 数量格式错误时按部分承担处理，数量校验单独显示。
    let all = false
    try {
        all =
            compareDecimal(
                sumFixed(
                    deliverable.map((row) => row.quantity),
                    { maxScale: 6, outputScale: 6 },
                ),
                input.quantity,
                6,
            ) === 0
    } catch {
        all = false
    }
    return rows.map((row) => ({
        ...row,
        quantity:
            all && addressDeliverable(option, row.region_code)
                ? row.quantity
                : "0",
    }))
}

export function sourcingDeliveryAllocations(
    product: SourcingProductLine | undefined,
    input: SourcingLineInput,
): PurchaseDeliveryAllocation[] {
    if (!product || !supplierDeliveryRequired(product, input)) return []
    return editableDeliveryAllocations(product, input).filter((row) => {
        try {
            return compareDecimal(row.quantity, "0", 6) > 0
        } catch {
            return false
        }
    })
}

export function deliveryValidationErrors(
    order: SourcingSalesOrder,
    lines: readonly SourcingLineInput[],
): Array<[number, string]> {
    const errors: Array<[number, string]> = []
    for (const product of order.lines) {
        const caps = new Map(
            remainingDeliveryAllocations(product).map((row) => [
                deliveryKey(row),
                row.quantity,
            ]),
        )
        lines.forEach((line, index) => {
            if (
                !line.selected ||
                line.salesOrderLineId !== product.salesOrderLineId ||
                !supplierDeliveryRequired(product, line)
            )
                return
            const rows = editableDeliveryAllocations(product, line)
            if (product.deliveryAllocations.length === 0 && rows.length === 0)
                return
            try {
                for (const row of rows) {
                    if (compareDecimal(row.quantity || "0", "0", 6) === 0)
                        continue
                    const quantityError = sourcingUnitQuantityError(
                        row.quantity,
                        product.quantityScale,
                    )
                    if (
                        quantityError ||
                        compareDecimal(row.quantity, "0", 6) < 0
                    )
                        throw new Error(quantityError ?? "地址数量不能为负数")
                    const cap = caps.get(deliveryKey(row))
                    if (cap === undefined)
                        throw new Error(
                            "交付地址与销售要求不一致，请刷新后重新分配",
                        )
                    const left = subtractFixed(cap, row.quantity, {
                        maxScale: 6,
                        outputScale: 6,
                    })
                    if (compareDecimal(left, "0", 6) < 0)
                        throw new Error(
                            "各来源对该地址的交付数量合计超过剩余需求",
                        )
                    caps.set(deliveryKey(row), left)
                }
                const total = sumFixed(
                    rows.map((row) => row.quantity || "0"),
                    { maxScale: 6, outputScale: 6 },
                )
                if (compareDecimal(total, line.quantity, 6) !== 0)
                    throw new Error("各地址交付数量合计必须等于本次分配数量")
            } catch (error) {
                errors.push([
                    index,
                    `${product.itemName}：${error instanceof Error ? error.message : "请检查地址交付数量"}`,
                ])
            }
        })
    }
    return errors
}

/**
 * 直发方案按本方案可配送地址的剩余需求合计，作为推荐数量上限。
 * 非直发方案或销售未指定地址时为空，不限制。
 */
export function directDeliveryCapacity(
    product: SourcingProductLine,
    option: SourcingSupplierOption,
): string | undefined {
    if (
        option.sourceType !== "PURCHASE" ||
        option.fulfillmentResponsibility !== "SUPPLIER_DIRECT" ||
        product.deliveryAllocations.length === 0
    )
        return undefined
    return sumFixed(
        remainingDeliveryAllocations(product)
            .filter((row) => addressDeliverable(option, row.region_code))
            .map((row) => row.quantity),
        { maxScale: 6, outputScale: 6 },
    )
}
