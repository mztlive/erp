import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"

/** 单条收货地址分配输入（兼容销售审批提交与采购审批提交结构）。 */
export type DeliveryAllocationInputRow = Readonly<{
    recipient: string
    phone: string
    region_code: string
    region_name?: string | null
    address: string
    quantity: string
}>

/** 参与整单收货汇总的商品行输入。 */
export type DeliveryAllocationLineInput = Readonly<{
    id: string
    lineNo: number
    itemName: string
    specification?: string | null
    unit?: string | null
    quantity?: string | null
    allocations?: readonly DeliveryAllocationInputRow[] | null
}>

/** 单处收货地址下接收的商品条目（同商品行合并数量）。 */
export type DeliveryAddressItemSummary = Readonly<{
    lineId: string
    lineNo: number
    itemName: string
    specification?: string
    unit: string
    quantity: string
}>

/** 按独立收货点聚合后的地址分组。 */
export type DeliveryAddressGroup = Readonly<{
    key: string
    recipient: string
    phone: string
    regionCode: string
    regionName: string
    address: string
    fullAddress: string
    items: readonly DeliveryAddressItemSummary[]
}>

/** 按商品行聚合后的地址分组。 */
export type DeliveryItemGroup = Readonly<{
    lineId: string
    lineNo: number
    itemName: string
    specification?: string
    unit: string
    quantity: string
    allocations: readonly DeliveryAllocationInputRow[]
    uniqueAddressCount: number
    allocatedTotal: string
    regionSummary: string
}>

/** 地区分布统计，用于多地址顶部概览。 */
export type DeliveryRegionStat = Readonly<{
    label: string
    count: number
}>

/** 整单收货地址汇总模型。 */
export type DeliveryAllocationSummary = Readonly<{
    itemGroups: readonly DeliveryItemGroup[]
    addressGroups: readonly DeliveryAddressGroup[]
    regionDistribution: readonly DeliveryRegionStat[]
    totalRows: number
    uniqueAddressCount: number
    assignedLineCount: number
    unassignedLineCount: number
    /** 整单仅存在 1 处独立收货地址，且所有参与配送的商品行均已分配至该地址。 */
    isUniformSingleAddress: boolean
}>

function normalizedSpecification(value?: string | null): string | undefined {
    const trimmed = value?.trim()
    if (!trimmed || trimmed === "无规格") return undefined
    return trimmed
}

function isActiveDeliveryLine(line: DeliveryAllocationLineInput): boolean {
    if (line.allocations && line.allocations.length > 0) return true
    const quantity = line.quantity?.trim()
    if (!quantity) return false
    try {
        return compareDecimal(quantity, "0", 6) > 0
    } catch {
        return true
    }
}

/** 统一以四元组识别独立收货点（收货人 + 电话 + 地区 + 详细地址）。 */
export function deliveryAddressIdentityKey(
    row: DeliveryAllocationInputRow,
): string {
    return JSON.stringify([
        row.recipient.trim(),
        row.phone.trim(),
        row.region_code.trim() || (row.region_name ?? "").trim(),
        row.address.trim(),
    ])
}

function safeSumFixed(values: readonly string[]): string {
    if (values.length === 0) return "0"
    try {
        return compactFixed(sumFixed(values, { maxScale: 6, outputScale: 6 }))
    } catch {
        return "0"
    }
}

function shortRegionLabel(row: DeliveryAllocationInputRow): string {
    const region = row.region_name?.trim()
    if (region) return region
    const address = row.address.trim()
    if (address) return address.slice(0, 14)
    return "其他地区"
}

function summarizeLineRegions(
    allocations: readonly DeliveryAllocationInputRow[],
): string {
    if (allocations.length === 0) return "未指定收货地址"
    const uniqueAddresses = new Map<string, DeliveryAllocationInputRow>()
    for (const row of allocations) {
        const key = deliveryAddressIdentityKey(row)
        if (!uniqueAddresses.has(key)) uniqueAddresses.set(key, row)
    }
    const rows = [...uniqueAddresses.values()]
    const first = rows[0]!
    if (rows.length === 1) {
        return (
            [first.recipient.trim(), first.region_name?.trim()]
                .filter(Boolean)
                .join(" · ") ||
            first.address.trim() ||
            "1 处地址"
        )
    }
    const regionCounts = new Map<string, number>()
    for (const row of rows) {
        const label = shortRegionLabel(row)
        regionCounts.set(label, (regionCounts.get(label) ?? 0) + 1)
    }
    const sorted = [...regionCounts.entries()].sort((a, b) => b[1] - a[1])
    if (sorted.length === 1) {
        return `${sorted[0]![0]}等 ${rows.length} 处`
    }
    return `${sorted[0]![0]}等 ${rows.length} 处`
}

/** 将整张单据的所有商品行收货分配汇总为按收货地址与按商品双视角。 */
export function buildDeliveryAllocationSummary(
    lines: readonly DeliveryAllocationLineInput[],
): DeliveryAllocationSummary {
    const activeLines = lines.filter(isActiveDeliveryLine)
    const itemGroups: DeliveryItemGroup[] = []
    const addressMap = new Map<
        string,
        {
            key: string
            recipient: string
            phone: string
            regionCode: string
            regionName: string
            address: string
            fullAddress: string
            itemMap: Map<
                string,
                {
                    lineId: string
                    lineNo: number
                    itemName: string
                    specification?: string
                    unit: string
                    quantities: string[]
                }
            >
        }
    >()

    let totalRows = 0
    let assignedLineCount = 0
    let unassignedLineCount = 0

    for (const line of activeLines) {
        const allocations = line.allocations ?? []
        const unit = line.unit?.trim() ?? ""
        const specification = normalizedSpecification(line.specification)
        const lineUniqueKeys = new Set<string>()

        if (allocations.length > 0) {
            assignedLineCount += 1
        } else {
            unassignedLineCount += 1
        }
        totalRows += allocations.length

        for (const row of allocations) {
            const key = deliveryAddressIdentityKey(row)
            lineUniqueKeys.add(key)
            const regionCode = row.region_code.trim()
            const regionName = row.region_name?.trim() ?? ""
            const address = row.address.trim()
            let existing = addressMap.get(key)
            if (!existing) {
                existing = {
                    key,
                    recipient: row.recipient.trim(),
                    phone: row.phone.trim(),
                    regionCode,
                    regionName,
                    address,
                    fullAddress: fullDeliveryAddress({
                        region_name: regionName,
                        address,
                    }),
                    itemMap: new Map(),
                }
                addressMap.set(key, existing)
            } else if (!existing.regionName && regionName) {
                existing.regionName = regionName
                existing.fullAddress = fullDeliveryAddress({
                    region_name: regionName,
                    address: existing.address,
                })
            }

            const existingItem = existing.itemMap.get(line.id)
            if (existingItem) {
                existingItem.quantities.push(row.quantity)
            } else {
                existing.itemMap.set(line.id, {
                    lineId: line.id,
                    lineNo: line.lineNo,
                    itemName: line.itemName,
                    specification,
                    unit,
                    quantities: [row.quantity],
                })
            }
        }

        itemGroups.push({
            lineId: line.id,
            lineNo: line.lineNo,
            itemName: line.itemName,
            specification,
            unit,
            quantity: line.quantity ? compactFixed(line.quantity) : "0",
            allocations,
            uniqueAddressCount: lineUniqueKeys.size,
            allocatedTotal: safeSumFixed(
                allocations.map((row) => row.quantity),
            ),
            regionSummary: summarizeLineRegions(allocations),
        })
    }

    const addressGroups: DeliveryAddressGroup[] = [...addressMap.values()].map(
        (entry) => ({
            key: entry.key,
            recipient: entry.recipient,
            phone: entry.phone,
            regionCode: entry.regionCode,
            regionName: entry.regionName,
            address: entry.address,
            fullAddress: entry.fullAddress,
            items: [...entry.itemMap.values()].map((item) => ({
                lineId: item.lineId,
                lineNo: item.lineNo,
                itemName: item.itemName,
                specification: item.specification,
                unit: item.unit,
                quantity: safeSumFixed(item.quantities),
            })),
        }),
    )

    const regionCounts = new Map<string, number>()
    for (const group of addressGroups) {
        const label = group.regionName || "其他地区"
        regionCounts.set(label, (regionCounts.get(label) ?? 0) + 1)
    }
    const regionDistribution: DeliveryRegionStat[] = [...regionCounts.entries()]
        .sort((a, b) => b[1] - a[1])
        .map(([label, count]) => ({ label, count }))

    const uniqueAddressCount = addressGroups.length
    const isUniformSingleAddress =
        uniqueAddressCount === 1 &&
        assignedLineCount > 0 &&
        unassignedLineCount === 0

    return {
        itemGroups,
        addressGroups,
        regionDistribution,
        totalRows,
        uniqueAddressCount,
        assignedLineCount,
        unassignedLineCount,
        isUniformSingleAddress,
    }
}

/**
 * 为明细表商品行生成极简地址提示：
 * - 整单统一发往同一地址时返回 `null`（由下方「交付与收货地址」统一展示，不在每行重复占位）；
 * - 仅多地址或部分分配时展示一行摘要供快速定位筛选。
 */
export function formatLineDeliveryHint(
    itemGroup: DeliveryItemGroup | undefined,
    summary: DeliveryAllocationSummary,
): string | null {
    if (!itemGroup || itemGroup.allocations.length === 0) return null
    if (summary.isUniformSingleAddress) return null
    return itemGroup.regionSummary
}

export function filterDeliveryAddressGroups(
    groups: readonly DeliveryAddressGroup[],
    query: string,
    focusedLineId?: string | null,
): DeliveryAddressGroup[] {
    const needle = query.trim().toLocaleLowerCase()
    const result: DeliveryAddressGroup[] = []
    for (const group of groups) {
        const scopedItems = focusedLineId
            ? group.items.filter((item) => item.lineId === focusedLineId)
            : group.items
        if (scopedItems.length === 0) continue

        if (!needle) {
            result.push(
                focusedLineId ? { ...group, items: scopedItems } : group,
            )
            continue
        }

        const headerMatched =
            `${group.recipient} ${group.phone} ${group.regionName} ${group.fullAddress}`
                .toLocaleLowerCase()
                .includes(needle)
        if (headerMatched) {
            result.push(
                focusedLineId ? { ...group, items: scopedItems } : group,
            )
            continue
        }

        const matchedItems = scopedItems.filter((item) =>
            `${item.itemName} ${item.specification ?? ""}`
                .toLocaleLowerCase()
                .includes(needle),
        )
        if (matchedItems.length > 0) {
            result.push({ ...group, items: matchedItems })
        }
    }
    return result
}

export function filterDeliveryItemGroups(
    groups: readonly DeliveryItemGroup[],
    query: string,
    focusedLineId?: string | null,
): DeliveryItemGroup[] {
    const needle = query.trim().toLocaleLowerCase()
    const scoped = focusedLineId
        ? groups.filter((group) => group.lineId === focusedLineId)
        : groups
    if (!needle) return [...scoped]

    const result: DeliveryItemGroup[] = []
    for (const group of scoped) {
        const itemMatched = `${group.itemName} ${group.specification ?? ""}`
            .toLocaleLowerCase()
            .includes(needle)
        if (itemMatched) {
            result.push(group)
            continue
        }
        const matchedAllocations = group.allocations.filter((row) =>
            `${row.recipient} ${row.phone} ${row.region_name ?? ""} ${fullDeliveryAddress(row)}`
                .toLocaleLowerCase()
                .includes(needle),
        )
        if (matchedAllocations.length > 0) {
            result.push({ ...group, allocations: matchedAllocations })
        }
    }
    return result
}
