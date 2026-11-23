import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { PurchaseDeliveryAllocation } from "../types"
import { purchasePaperFileNamePart } from "./purchase-paper-image"

/** 在线纸质预览默认直接展开的最大地址分配条数；超过后切换为摘要与侧栏检索。 */
export const PREVIEW_INLINE_ADDRESS_LIMIT = 8

/** 单张采购单图片内联展示的最大地址分配条数；超过后主图仅展示摘要并配套导出 Excel 地址表。 */
export const IMAGE_INLINE_ADDRESS_LIMIT = 12

export type PurchaseDeliveryPlanLine = Readonly<{
    lineId?: string
    salesOrderLineId?: string
    itemName: string
    itemSku?: string
    unit?: string
    quantity?: string
    expectedDeliveryDate?: string
    deliveryAllocations?: readonly PurchaseDeliveryAllocation[]
}>

export type PurchaseDeliveryItemGroup = Readonly<{
    key: string
    itemName: string
    itemSku?: string
    unit: string
    quantity: string
    expectedDeliveryDate?: string
    allocations: readonly PurchaseDeliveryAllocation[]
    allocatedTotal: string
    regionSummary: string
}>

export type PurchaseDeliveryAddressItem = Readonly<{
    itemKey: string
    itemName: string
    itemSku?: string
    unit: string
    quantity: string
}>

export type PurchaseDeliveryAddressGroup = Readonly<{
    key: string
    recipient: string
    phone: string
    regionName: string
    address: string
    fullAddress: string
    items: readonly PurchaseDeliveryAddressItem[]
}>

export type PurchaseDeliveryRegionCount = Readonly<{
    name: string
    count: number
}>

export type PurchaseDeliveryPlanSummary = Readonly<{
    itemGroups: readonly PurchaseDeliveryItemGroup[]
    addressGroups: readonly PurchaseDeliveryAddressGroup[]
    totalRows: number
    uniqueAddressCount: number
    unassignedLineCount: number
    topRegions: readonly PurchaseDeliveryRegionCount[]
}>

function isActiveDeliveryLine(line: PurchaseDeliveryPlanLine): boolean {
    const quantity = line.quantity?.trim()
    if (!quantity) return false
    try {
        return compareDecimal(quantity, "0", 6) > 0
    } catch {
        return true
    }
}

function normalizedSku(itemSku?: string): string | undefined {
    const trimmed = itemSku?.trim()
    if (!trimmed || trimmed === "无规格") return undefined
    return trimmed
}

function addressIdentityKey(row: PurchaseDeliveryAllocation): string {
    return JSON.stringify([
        row.recipient.trim(),
        row.phone.trim(),
        row.region_code.trim() || (row.region_name ?? "").trim(),
        row.address.trim(),
    ])
}

function summarizeGroupRegions(
    allocations: readonly PurchaseDeliveryAllocation[],
): string {
    if (allocations.length === 0) return "未指定交付地址，履约前补齐"
    const first = allocations[0]
    const firstRegion = first.region_name?.trim() || first.address.trim()
    if (allocations.length === 1) {
        return [first.recipient.trim(), firstRegion].filter(Boolean).join(" · ")
    }
    const counts = new Map<string, number>()
    for (const row of allocations) {
        const region = row.region_name?.trim() || "其他地区"
        counts.set(region, (counts.get(region) ?? 0) + 1)
    }
    const sorted = [...counts.entries()].sort((a, b) => b[1] - a[1])
    if (sorted.length === 1) {
        return `${sorted[0][0]}（${first.recipient.trim()} 等 ${allocations.length} 处）`
    }
    const lead = sorted
        .slice(0, 2)
        .map(([name, count]) => `${name} ${count} 处`)
        .join("、")
    return sorted.length > 2 ? `${lead} 等 ${sorted.length} 个地区` : lead
}

function safeSumFixed(values: readonly string[]): string {
    if (values.length === 0) return "0"
    try {
        return compactFixed(sumFixed(values, { maxScale: 6, outputScale: 6 }))
    } catch {
        return "0"
    }
}

/** 将采购单各行交付安排汇总为按商品分组、按收货地址分组与地区分布统计。 */
export function buildPurchaseDeliveryPlanSummary(
    lines: readonly PurchaseDeliveryPlanLine[],
): PurchaseDeliveryPlanSummary {
    const activeLines = lines.filter(isActiveDeliveryLine)
    const itemGroups: PurchaseDeliveryItemGroup[] = []
    const addressMap = new Map<
        string,
        {
            key: string
            recipient: string
            phone: string
            regionName: string
            address: string
            fullAddress: string
            items: PurchaseDeliveryAddressItem[]
        }
    >()
    const regionCounts = new Map<string, number>()

    let totalRows = 0
    let unassignedLineCount = 0

    activeLines.forEach((line, index) => {
        const allocations = line.deliveryAllocations ?? []
        const unit = line.unit?.trim() ?? ""
        const sku = normalizedSku(line.itemSku)
        const key =
            line.lineId ||
            line.salesOrderLineId ||
            `${line.itemName}-${sku ?? "none"}-${index}`

        if (allocations.length === 0) {
            unassignedLineCount += 1
        }
        totalRows += allocations.length

        itemGroups.push({
            key,
            itemName: line.itemName,
            itemSku: sku,
            unit,
            quantity: compactFixed(line.quantity ?? "0"),
            expectedDeliveryDate:
                line.expectedDeliveryDate?.trim() || undefined,
            allocations,
            allocatedTotal: safeSumFixed(
                allocations.map((row) => row.quantity),
            ),
            regionSummary: summarizeGroupRegions(allocations),
        })

        for (const row of allocations) {
            const addrKey = addressIdentityKey(row)
            const regionName = row.region_name?.trim() ?? ""
            const existing = addressMap.get(addrKey)
            const itemEntry: PurchaseDeliveryAddressItem = {
                itemKey: key,
                itemName: line.itemName,
                itemSku: sku,
                unit,
                quantity: compactFixed(row.quantity),
            }
            if (existing) {
                existing.items.push(itemEntry)
            } else {
                addressMap.set(addrKey, {
                    key: addrKey,
                    recipient: row.recipient.trim(),
                    phone: row.phone.trim(),
                    regionName,
                    address: row.address.trim(),
                    fullAddress: fullDeliveryAddress(row),
                    items: [itemEntry],
                })
                if (regionName) {
                    regionCounts.set(
                        regionName,
                        (regionCounts.get(regionName) ?? 0) + 1,
                    )
                }
            }
        }
    })

    const topRegions = [...regionCounts.entries()]
        .sort((a, b) => b[1] - a[1])
        .map(([name, count]) => ({ name, count }))

    return {
        itemGroups,
        addressGroups: [...addressMap.values()],
        totalRows,
        uniqueAddressCount: addressMap.size,
        unassignedLineCount,
        topRegions,
    }
}

/** 统计直发/服务采购单中的交付地址分配总条数；入仓履约返回 0。 */
export function countPurchaseDeliveryRows(
    lines: readonly PurchaseDeliveryPlanLine[],
    warehouse: boolean,
): number {
    if (warehouse) return 0
    return lines
        .filter(isActiveDeliveryLine)
        .reduce(
            (total, line) => total + (line.deliveryAllocations?.length ?? 0),
            0,
        )
}

/** 判断采购单图片导出时是否应将收货地址拆分为配套 Excel 明细表。 */
export function shouldBundleDeliveryWorkbook(
    lines: readonly PurchaseDeliveryPlanLine[],
    warehouse: boolean,
): boolean {
    return (
        countPurchaseDeliveryRows(lines, warehouse) > IMAGE_INLINE_ADDRESS_LIMIT
    )
}

/** 生成配套发货地址明细表文件名。 */
export function purchaseDeliveryWorkbookFileName(
    supplierName: string,
    sequencePrefix?: string,
): string {
    const base = purchasePaperFileNamePart(supplierName)
    return sequencePrefix
        ? `${sequencePrefix}-${base}-发货地址明细.xlsx`
        : `${base}-发货地址明细.xlsx`
}
