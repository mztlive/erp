import {
    compareDecimal,
    sumFixed,
    subtractFixed,
    clampZeroFixed,
} from "@/lib/fixed-decimal"
import type { ValidationIssue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"

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
    const add = (message: string, targetId = "fulfillment-packing-form") =>
        issues.push({
            id: `packing-${issues.length}`,
            label: "发货包裹",
            message,
            targetId,
        })
    try {
        if (packing.source_version !== source.source_version)
            add("来源交付安排已变化，请刷新后核对")
        if (
            packing.packages.flatMap((row) => row.items).length > 100 ||
            packing.dispatch.length > 100
        )
            add("一次发货最多100个地址或包裹商品明细行数")
        const seen = new Set<string>()
        for (const [index, parcel] of packing.packages.entries()) {
            const prefix = `fulfillment-packing-package-${toAutomationIdSegment(parcel.id)}`
            const label = `包裹 ${index + 1}`
            if (!parcel.carrier.trim())
                add(`${label}尚未选择物流公司`, `${prefix}-carrier`)
            if (!parcel.tracking_no.trim())
                add(`${label}尚未填写物流单号`, `${prefix}-tracking`)
            if (!parcel.items.length)
                add(
                    `${label}尚未分配商品，请调整商品或移除包裹`,
                    `${prefix}-remove`,
                )
            const key = `${parcel.carrier.trim()}:${parcel.tracking_no.trim()}`
            if (parcel.carrier.trim() && parcel.tracking_no.trim()) {
                if (seen.has(key))
                    add(
                        "同一物流包裹不能重复登记或发往不同地址",
                        `${prefix}-tracking`,
                    )
                seen.add(key)
            }
            try {
                if (
                    parcel.items.some(
                        (item) => compareDecimal(item.quantity, "0", 6) <= 0,
                    )
                )
                    add(`${label}的商品数量必须大于0`, `${prefix}-edit-items`)
            } catch {
                add(
                    `${label}的商品数量无效，请填写最多6位小数的数量`,
                    `${prefix}-edit-items`,
                )
            }
        }
        let positive = false
        for (const row of packing.dispatch) {
            const addressId = `fulfillment-packing-address-${toAutomationIdSegment(row.destination_key)}`
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
                const quantityId = `fulfillment-packing-${toAutomationIdSegment(row.destination_key)}-${toAutomationIdSegment(item.delivery_line_id)}-quantity`
                try {
                    compareDecimal(item.quantity, "0", 6)
                } catch {
                    add("请填写有效的本次发货数量，最多6位小数", quantityId)
                    continue
                }
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
                    add("本次发货数量须在0与该地址应发数量之间", quantityId)
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
                if (compareDecimal(packed, item.quantity, 6) !== 0) {
                    const parcel = packing.packages.find(
                        (parcel) =>
                            parcel.destination_key === row.destination_key,
                    )
                    add(
                        parcel
                            ? "包裹分配数量与本次发货不一致，请调整包裹商品数量"
                            : `${destination?.recipient || "收货地址"}的物流尚未填写，请登记物流公司和单号`,
                        parcel
                            ? `fulfillment-packing-package-${toAutomationIdSegment(parcel.id)}-edit-items`
                            : `${addressId}-logistics`,
                    )
                }
            }
        }
        if (!positive) {
            const first = packing.dispatch.find(
                (row) =>
                    row.items.length &&
                    !source.destinations.find(
                        (destination) =>
                            destination.key === row.destination_key,
                    )?.pending_address,
            )
            add(
                "请至少填写一项大于0的本次发货数量",
                first
                    ? `fulfillment-packing-${toAutomationIdSegment(first.destination_key)}-${toAutomationIdSegment(first.items[0].delivery_line_id)}-quantity`
                    : "fulfillment-packing-form",
            )
        }
    } catch {
        add("请填写有效数量，最多6位小数")
    }
    return issues.filter(
        (issue, i, all) =>
            all.findIndex(
                (value) =>
                    value.message === issue.message &&
                    value.targetId === issue.targetId,
            ) === i,
    )
}
