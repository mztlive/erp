import type { PurchaseDeliveryLineOption } from "@/features/purchase-orders/api/purchase-delivery-options"
import type { PurchaseDeliveryAllocation } from "@/features/purchase-orders/types"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"

type AddressIdentity = Pick<
    PurchaseDeliveryAllocation,
    "recipient" | "phone" | "address"
>

/** 收件资料身份：收货人、电话与地址。 */
export function deliveryAddressKey(row: AddressIdentity) {
    return `${row.recipient}\u0000${row.phone}\u0000${row.address}`
}

function sameAddress(left: AddressIdentity, right: AddressIdentity) {
    return deliveryAddressKey(left) === deliveryAddressKey(right)
}

/**
 * 按销售当前收件地址列出可分配行，数量取本采购当前分配；未分配的地址为 0。
 */
export function editableDeliveryRows(
    option: PurchaseDeliveryLineOption,
    current: readonly PurchaseDeliveryAllocation[],
): PurchaseDeliveryAllocation[] {
    return option.addresses.map((address) => ({
        recipient: address.recipient,
        phone: address.phone,
        address: address.address,
        quantity:
            current.find((row) => sameAddress(row, address))?.quantity ?? "0",
    }))
}

/** 本采购当前分配中已不在销售当前收件资料里的地址。 */
export function removedDeliveryRows(
    option: PurchaseDeliveryLineOption,
    current: readonly PurchaseDeliveryAllocation[],
) {
    return current.filter(
        (row) => !option.addresses.some((address) => sameAddress(row, address)),
    )
}

function positiveQuantity(value: string) {
    try {
        return compareDecimal(value.trim(), "0", 6) > 0
    } catch {
        return false
    }
}

/** 去掉数量为零或空白的地址，规范化数量文本。 */
export function positiveDeliveryRows(
    rows: readonly PurchaseDeliveryAllocation[],
): PurchaseDeliveryAllocation[] {
    return rows
        .filter((row) => positiveQuantity(row.quantity))
        .map((row) => ({ ...row, quantity: compactFixed(row.quantity.trim()) }))
}

/** 各地址数量合计；存在非法数量时返回 undefined。 */
export function deliveryRowsTotal(rows: readonly PurchaseDeliveryAllocation[]) {
    try {
        return compactFixed(
            sumFixed(
                rows.map((row) => row.quantity.trim() || "0"),
                { maxScale: 6, outputScale: 6 },
            ),
        )
    } catch {
        return undefined
    }
}

/** 各地址数量合计是否等于采购数量。 */
export function deliveryRowsMatchQuantity(
    rows: readonly PurchaseDeliveryAllocation[],
    quantity: string,
) {
    const total = deliveryRowsTotal(rows)
    if (total === undefined) return false
    try {
        return compareDecimal(total, quantity.trim(), 6) === 0
    } catch {
        return false
    }
}

/** 某地址数量是否超过扣除其他采购后的剩余量。 */
export function deliveryRowExceedsAvailable(
    row: PurchaseDeliveryAllocation,
    option: PurchaseDeliveryLineOption,
) {
    const address = option.addresses.find((item) => sameAddress(item, row))
    if (!address) return positiveQuantity(row.quantity)
    try {
        return (
            compareDecimal(
                row.quantity.trim() || "0",
                address.availableQuantity,
                6,
            ) > 0
        )
    } catch {
        return true
    }
}

/** 两组分配的地址与数量是否一致。 */
export function sameDeliveryRows(
    left: readonly PurchaseDeliveryAllocation[],
    right: readonly PurchaseDeliveryAllocation[],
) {
    const a = positiveDeliveryRows(left)
    const b = positiveDeliveryRows(right)
    return (
        a.length === b.length &&
        a.every((row) =>
            b.some(
                (other) =>
                    sameAddress(row, other) &&
                    compareDecimal(row.quantity, other.quantity, 6) === 0,
            ),
        )
    )
}

/** 预计交付日是否晚于销售履约期限；日期均为 YYYY-MM-DD。 */
export function expectedDateAfterDue(expected: string, due: string) {
    return expected.trim() !== "" && expected.trim() > due
}

/** 履约期限之后的日期不可选；期限无法解析时不限制。 */
export function afterDueDates(due: string) {
    const date = new Date(`${due}T00:00:00`)
    return Number.isNaN(date.getTime()) ? undefined : { after: date }
}
