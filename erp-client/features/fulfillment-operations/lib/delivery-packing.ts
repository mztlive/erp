import {
    compareDecimal,
    sumFixed,
    subtractFixed,
    clampZeroFixed,
} from "@/lib/fixed-decimal"
import type { ValidationIssue } from "@/components/business"

export type PackingItem = { delivery_line_id: string; quantity: string }
export type DeliveryDestination = {
    key: string
    recipient: string
    phone: string
    address: string
    items: PackingItem[]
    pending_address?: boolean
}
export type DeliveryAddressSnapshot = {
    source_version: string
    destinations: DeliveryDestination[]
}
export type DeliveryPackage = {
    id: string
    destination_key: string
    carrier: string
    tracking_no: string
    items: PackingItem[]
}
export type DeliveryPacking = {
    source_version: string
    dispatch: Array<{ destination_key: string; items: PackingItem[] }>
    packages: DeliveryPackage[]
}
export const packingSum = (values: string[]) =>
    sumFixed(values, { maxScale: 6, outputScale: 6 })
export function initialPacking(
    source: DeliveryAddressSnapshot,
): DeliveryPacking {
    return {
        source_version: source.source_version,
        dispatch: source.destinations.map((row) => ({
            destination_key: row.key,
            items: row.items.map((item) => ({
                ...item,
                quantity: row.pending_address ? "0" : item.quantity,
            })),
        })),
        packages: [],
    }
}
export function unpackedItems(
    packing: DeliveryPacking,
    key: string,
): PackingItem[] {
    return (
        packing.dispatch.find((row) => row.destination_key === key)?.items ?? []
    )
        .map((item) => ({
            ...item,
            quantity: clampZeroFixed(
                subtractFixed(
                    item.quantity || "0",
                    packingSum(
                        packing.packages
                            .filter((row) => row.destination_key === key)
                            .flatMap((row) =>
                                row.items
                                    .filter(
                                        (value) =>
                                            value.delivery_line_id ===
                                            item.delivery_line_id,
                                    )
                                    .map((value) => value.quantity || "0"),
                            ),
                    ),
                    { maxScale: 6, outputScale: 6 },
                ),
                { maxScale: 6, outputScale: 6 },
            ),
        }))
        .filter((item) => compareDecimal(item.quantity, "0", 6) > 0)
}
export function packingIssues(
    packing: DeliveryPacking,
    source: DeliveryAddressSnapshot,
): ValidationIssue[] {
    const issues: ValidationIssue[] = []
    const add = (message: string) =>
        issues.push({
            id: `packing-${issues.length}`,
            label: "发货包裹",
            message,
            targetId: "fulfillment-packing-form",
        })
    try {
        if (packing.source_version !== source.source_version)
            add("来源交付安排已变化，请刷新后核对")
        if (
            packing.packages.flatMap((row) => row.items).length > 100 ||
            packing.dispatch.length > 100
        )
            add("一次发货最多100个地址或包裹商品关联")
        const seen = new Set<string>()
        for (const parcel of packing.packages) {
            if (
                !parcel.carrier.trim() ||
                !parcel.tracking_no.trim() ||
                !parcel.items.length
            )
                add("请填写包裹物流公司、单号及商品数量")
            const key = `${parcel.carrier.trim()}:${parcel.tracking_no.trim()}`
            if (seen.has(key)) add("同一物流包裹不能重复登记或发往不同地址")
            seen.add(key)
            if (
                parcel.items.some(
                    (item) => compareDecimal(item.quantity, "0", 6) <= 0,
                )
            )
                add("包裹商品数量必须大于0")
        }
        let positive = false
        for (const row of packing.dispatch) {
            const destination = source.destinations.find(
                (value) => value.key === row.destination_key,
            )
            if (
                destination?.pending_address &&
                (row.items.some(
                    (item) => compareDecimal(item.quantity, "0", 6) > 0,
                ) ||
                    packing.packages.some(
                        (parcel) =>
                            parcel.destination_key === row.destination_key,
                    ))
            )
                add("待补地址的商品本次不能发货，请补齐地址或将本次数量设为0")
            for (const item of row.items) {
                const limit =
                    source.destinations
                        .find((value) => value.key === row.destination_key)
                        ?.items.find(
                            (value) =>
                                value.delivery_line_id ===
                                item.delivery_line_id,
                        )?.quantity ?? "0"
                if (
                    compareDecimal(item.quantity, "0", 6) < 0 ||
                    compareDecimal(item.quantity, limit, 6) > 0
                )
                    add("本次发货数量须在0与该地址应发数量之间")
                if (compareDecimal(item.quantity, "0", 6) > 0) positive = true
                const packed = packingSum(
                    packing.packages
                        .filter(
                            (parcel) =>
                                parcel.destination_key === row.destination_key,
                        )
                        .flatMap((parcel) =>
                            parcel.items
                                .filter(
                                    (value) =>
                                        value.delivery_line_id ===
                                        item.delivery_line_id,
                                )
                                .map((value) => value.quantity),
                        ),
                )
                if (compareDecimal(packed, item.quantity, 6) !== 0)
                    add("每项商品的包裹数量合计须等于该地址本次发货数量")
            }
        }
        if (!positive) add("本次发货数量必须大于0")
    } catch {
        add("请填写有效数量，最多6位小数")
    }
    return issues.filter(
        (issue, i, all) =>
            all.findIndex((value) => value.message === issue.message) === i,
    )
}
