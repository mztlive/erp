import {
    clampZeroFixed,
    compactFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type {
    SalesDeliveryAllocation,
    SalesOrderDraftLineInput,
} from "../types"
import { allocationSummary } from "./sales-delivery-allocation-summary"
import { addressKey } from "./sales-delivery-import"
import type { UnifiedDeliveryAddress } from "./sales-order-create-model"

export type DeliveryAddressGroup = {
    key: string
    recipient: string
    phone: string
    region_code: string
    region_name?: string
    address: string
    lineKeys: string[]
    lineNames: string[]
    lineCount: number
}

export type OrderDeliveryOverview = {
    unified?: SalesDeliveryAllocation
    groups: DeliveryAddressGroup[]
    uniqueAddressCount: number
    totalLines: number
    allocatedLineCount: number
    completeLineCount: number
    incompleteLineCount: number
    unsetLineCount: number
}

/** 仅当全部商品都完整且唯一地指向同一个收货地址时返回该统一地址。 */
export function detectUnifiedAddress(
    lines: readonly SalesOrderDraftLineInput[],
): SalesDeliveryAllocation | undefined {
    const first = lines[0]?.deliveryAllocations?.[0]
    if (!first || lines.length === 0) return undefined
    const firstKey = addressKey(first)
    return lines.every((line) => {
        const rows = line.deliveryAllocations ?? []
        const row = rows[0]
        return (
            rows.length === 1 &&
            row != null &&
            allocationSummary(line).complete &&
            addressKey(row) === firstKey
        )
    })
        ? first
        : undefined
}

/** 汇总当前销售单中已填写的去重地址池，按覆盖商品数降序排列。 */
export function collectOrderAddressPool(
    lines: readonly SalesOrderDraftLineInput[],
): DeliveryAddressGroup[] {
    const map = new Map<
        string,
        {
            sample: SalesDeliveryAllocation
            lineKeys: Set<string>
            lineNames: string[]
        }
    >()

    for (let index = 0; index < lines.length; index += 1) {
        const line = lines[index]!
        const lineName = line.name.trim() || `商品 ${index + 1}`
        for (const row of line.deliveryAllocations ?? []) {
            if (
                !row.recipient.trim() &&
                !row.phone.trim() &&
                !row.region_code &&
                !row.address.trim()
            ) {
                continue
            }
            const key = addressKey(row)
            const entry = map.get(key)
            if (entry) {
                if (!entry.lineKeys.has(line.rowKey)) {
                    entry.lineKeys.add(line.rowKey)
                    entry.lineNames.push(lineName)
                }
                if (!entry.sample.region_name && row.region_name) {
                    entry.sample = {
                        ...entry.sample,
                        region_name: row.region_name,
                    }
                }
            } else {
                map.set(key, {
                    sample: row,
                    lineKeys: new Set([line.rowKey]),
                    lineNames: [lineName],
                })
            }
        }
    }

    return [...map.entries()]
        .map(([key, entry]) => ({
            key,
            recipient: entry.sample.recipient.trim(),
            phone: entry.sample.phone.trim(),
            region_code: entry.sample.region_code,
            region_name: entry.sample.region_name,
            address: entry.sample.address.trim(),
            lineKeys: [...entry.lineKeys],
            lineNames: entry.lineNames,
            lineCount: entry.lineKeys.size,
        }))
        .sort((left, right) => right.lineCount - left.lineCount)
}

export function summarizeOrderDelivery(
    lines: readonly SalesOrderDraftLineInput[],
): OrderDeliveryOverview {
    const unified = detectUnifiedAddress(lines)
    const groups = collectOrderAddressPool(lines)
    let allocatedLineCount = 0
    let completeLineCount = 0
    let incompleteLineCount = 0

    for (const line of lines) {
        if ((line.deliveryAllocations?.length ?? 0) > 0) {
            allocatedLineCount += 1
            if (allocationSummary(line).complete) {
                completeLineCount += 1
            } else {
                incompleteLineCount += 1
            }
        }
    }

    return {
        unified,
        groups,
        uniqueAddressCount: groups.length,
        totalLines: lines.length,
        allocatedLineCount,
        completeLineCount,
        incompleteLineCount,
        unsetLineCount: lines.length - allocatedLineCount,
    }
}

/** 仅把收货地址应用到已勾选的商品（每项商品按其全部数量分配）。 */
export function applyDeliveryAddressToSelectedLines(
    lines: readonly SalesOrderDraftLineInput[],
    selectedRowKeys: ReadonlySet<string>,
    address: UnifiedDeliveryAddress,
): SalesOrderDraftLineInput[] {
    const nextAddress = {
        recipient: address.recipient.trim(),
        phone: address.phone.trim(),
        region_code: address.region_code,
        region_name: address.region_name,
        address: address.address.trim(),
    }
    return lines.map((line) =>
        selectedRowKeys.has(line.rowKey)
            ? {
                  ...line,
                  deliveryAllocations: [
                      { ...nextAddress, quantity: line.quantity },
                  ],
              }
            : line,
    )
}

/** 将指定地址填补到当前所有尚未设置地址的商品。 */
export function applyDeliveryAddressToUnsetLines(
    lines: readonly SalesOrderDraftLineInput[],
    address: UnifiedDeliveryAddress,
): { nextLines: SalesOrderDraftLineInput[]; updatedCount: number } {
    const nextAddress = {
        recipient: address.recipient.trim(),
        phone: address.phone.trim(),
        region_code: address.region_code,
        region_name: address.region_name,
        address: address.address.trim(),
    }
    let updatedCount = 0
    const nextLines = lines.map((line) => {
        if ((line.deliveryAllocations?.length ?? 0) > 0) return line
        updatedCount += 1
        return {
            ...line,
            deliveryAllocations: [{ ...nextAddress, quantity: line.quantity }],
        }
    })
    return { nextLines, updatedCount }
}

/** 在整张单据中批量修改某一个地址的收件信息，保留每项商品各自的分配数量。 */
export function replaceAddressAcrossOrder(
    lines: readonly SalesOrderDraftLineInput[],
    targetKey: string,
    nextAddress: UnifiedDeliveryAddress,
): { nextLines: SalesOrderDraftLineInput[]; affectedLineCount: number } {
    const normalized = {
        recipient: nextAddress.recipient.trim(),
        phone: nextAddress.phone.trim(),
        region_code: nextAddress.region_code,
        region_name: nextAddress.region_name,
        address: nextAddress.address.trim(),
    }
    const nextKey = addressKey({ ...normalized, quantity: "1" })
    let affectedLineCount = 0

    const nextLines = lines.map((line) => {
        const allocations = line.deliveryAllocations ?? []
        if (!allocations.some((row) => addressKey(row) === targetKey)) {
            return line
        }
        affectedLineCount += 1
        // 若改后的地址与该行已有另一条地址完全重合，自动合并数量避免重复。
        const merged: SalesDeliveryAllocation[] = []
        const byKey = new Map<string, number>()
        for (const row of allocations) {
            const current =
                addressKey(row) === targetKey
                    ? { ...normalized, quantity: row.quantity }
                    : row
            const key = addressKey(current)
            const existingIndex = byKey.get(key)
            if (existingIndex != null && key === nextKey) {
                const existing = merged[existingIndex]!
                try {
                    merged[existingIndex] = {
                        ...existing,
                        quantity: compactFixed(
                            sumFixed([existing.quantity, current.quantity], {
                                maxScale: 6,
                                outputScale: 6,
                            }),
                        ),
                    }
                } catch {
                    merged.push(current)
                }
            } else {
                byKey.set(key, merged.length)
                merged.push(current)
            }
        }
        return { ...line, deliveryAllocations: merged }
    })

    return { nextLines, affectedLineCount }
}

/** 从整张单据的所有商品中移除指定收货地址。 */
export function removeAddressAcrossOrder(
    lines: readonly SalesOrderDraftLineInput[],
    targetKey: string,
): { nextLines: SalesOrderDraftLineInput[]; affectedLineCount: number } {
    let affectedLineCount = 0
    const nextLines = lines.map((line) => {
        const allocations = line.deliveryAllocations ?? []
        const filtered = allocations.filter(
            (row) => addressKey(row) !== targetKey,
        )
        if (filtered.length === allocations.length) return line
        affectedLineCount += 1
        return { ...line, deliveryAllocations: filtered }
    })
    return { nextLines, affectedLineCount }
}

/** 计算当前商品尚未分配的剩余数量；超量或数量无效时返回空字符串。 */
export function remainingQuantityForLine(
    lineQuantity: string,
    rows: readonly { quantity: string }[],
): string {
    try {
        if (compareDecimal(lineQuantity || "0", "0", 6) <= 0) return ""
        const total = sumFixed(
            rows.map((row) => row.quantity || "0"),
            { maxScale: 6, outputScale: 6 },
        )
        if (compareDecimal(total, lineQuantity, 6) >= 0) return ""
        return compactFixed(
            subtractFixed(lineQuantity, total, {
                maxScale: 6,
                outputScale: 6,
            }),
        )
    } catch {
        return ""
    }
}

/** 计算当前商品收货地址已分配比例（0–100 字符串，用于进度条宽度）。 */
export function allocatedPercentForLine(
    lineQuantity: string,
    rows: readonly { quantity: string }[],
): string {
    try {
        if (compareDecimal(lineQuantity || "0", "0", 6) <= 0) return "0"
        const total = sumFixed(
            rows.map((row) => row.quantity || "0"),
            { maxScale: 6, outputScale: 6 },
        )
        const clamped = clampZeroFixed(total, { maxScale: 6, outputScale: 6 })
        const ratio = divideFixed(clamped, lineQuantity, {
            numeratorMaxScale: 6,
            denominatorMaxScale: 6,
            outputScale: 4,
        })
        const pct = multiplyFixed(ratio, "100", {
            leftMaxScale: 4,
            rightMaxScale: 0,
            outputScale: 0,
        })
        return compactFixed(
            minFixed(pct, "100", { maxScale: 0, outputScale: 0 }),
        )
    } catch {
        return "0"
    }
}
