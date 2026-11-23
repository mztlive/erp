import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
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
import type { AddressAllocation, ArrangementView } from "../api"

const QTY_PRECISION = { maxScale: 6, outputScale: 6 } as const

export type ArrangementLineMeta = Readonly<{
    unit?: string
    sku?: string
    isGift?: boolean
}>

export type DerivedAllocationRow = Readonly<{
    key: string
    idSegment: string
    recipient: string
    phone: string
    regionCode: string
    regionName: string
    address: string
    fullAddress: string
    quantity: string
    shippedQuantity: string
    hasShipped: boolean
}>

export type DerivedUnmatchedShippedRow = Readonly<{
    key: string
    idSegment: string
    recipient: string
    phone: string
    address: string
    quantity: string
}>

export type DerivedArrangementLine = Readonly<{
    lineId: string
    itemName: string
    sku?: string
    isGift: boolean
    unit: string
    quantity: string
    allocations: readonly DerivedAllocationRow[]
    unmatchedShipped: readonly DerivedUnmatchedShippedRow[]
    allocatedQuantity: string
    pendingQuantity: string
    shippedQuantity: string
    hasPending: boolean
    hasShipped: boolean
    isComplete: boolean
    allocatedPercent: string
    uniqueRegions: readonly string[]
    recipientPreview: string
    regionPreview: string
}>

export type DerivedAddressGroupItem = Readonly<{
    lineId: string
    itemName: string
    sku?: string
    isGift: boolean
    unit: string
    quantity: string
    shippedQuantity: string
    hasShipped: boolean
}>

export type DerivedAddressGroup = Readonly<{
    key: string
    idSegment: string
    recipient: string
    phone: string
    regionCode: string
    regionName: string
    address: string
    fullAddress: string
    items: readonly DerivedAddressGroupItem[]
    hasShipped: boolean
    lineCount: number
    singleUnitTotal?: Readonly<{ quantity: string; unit: string }>
}>

export type DerivedArrangementSummary = Readonly<{
    lines: readonly DerivedArrangementLine[]
    addressGroups: readonly DerivedAddressGroup[]
    totalLines: number
    completeLineCount: number
    partialLineCount: number
    unsetLineCount: number
    pendingLineCount: number
    shippedLineCount: number
    uniqueAddressCount: number
    totalAllocationCount: number
    totalShippedCount: number
    unifiedDestination?: DerivedAddressGroup
    singleUnitTotals?: Readonly<{
        total: string
        allocated: string
        pending: string
        shipped: string
        unit: string
    }>
    overallStatus: "complete" | "partial" | "unset"
}>

/** 32-bit FNV-1a 确定性摘要，用于中文地址生成稳定无碰撞的自动化 DOM ID 片段。 */
function fnv1aHex(input: string): string {
    let hash = 0x811c9dc5
    for (let index = 0; index < input.length; index += 1) {
        hash ^= input.charCodeAt(index)
        hash = Math.imul(hash, 0x01000193)
    }
    return (hash >>> 0).toString(16).padStart(8, "0")
}

export function stableAddressIdSegment(key: string, phone: string): string {
    const phonePart = phone.trim() ? toAutomationIdSegment(phone) : "addr"
    return `${phonePart}-${fnv1aHex(key)}`
}

function normalizedAllocationKey(row: {
    recipient: string
    phone: string
    region_code: string
    region_name?: string
    address: string
}): string {
    return [
        row.recipient.trim(),
        row.phone.trim(),
        row.region_code.trim() || (row.region_name ?? "").trim(),
        row.address.trim(),
    ].join("|")
}

function shippedIdentityKey(
    recipient: string,
    phone: string,
    fullAddress: string,
): string {
    return [recipient.trim(), phone.trim(), fullAddress.trim()].join("|")
}

function safeSum(values: readonly string[]): string {
    if (values.length === 0) return "0.000000"
    try {
        return sumFixed(values, QTY_PRECISION)
    } catch {
        return "0.000000"
    }
}

function safePending(total: string, allocated: string): string {
    try {
        return clampZeroFixed(
            subtractFixed(total, allocated, QTY_PRECISION),
            QTY_PRECISION,
        )
    } catch {
        return "0.000000"
    }
}

function isPositiveDecimal(value: string): boolean {
    try {
        return compareDecimal(value, "0", 6) > 0
    } catch {
        return false
    }
}

function computeAllocatedPercent(allocated: string, total: string): string {
    try {
        if (compareDecimal(total, "0", 6) <= 0) return "0"
        const clamped = clampZeroFixed(allocated, QTY_PRECISION)
        const ratio = divideFixed(clamped, total, {
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

function buildRecipientPreview(allocations: readonly DerivedAllocationRow[]) {
    const names: string[] = []
    const seen = new Set<string>()
    for (const row of allocations) {
        const name = row.recipient.trim()
        if (!name || seen.has(name)) continue
        seen.add(name)
        names.push(name)
    }
    if (names.length === 0) return ""
    if (names.length <= 3) return names.join("、")
    return `${names.slice(0, 2).join("、")} 等`
}

function buildRegionPreview(regions: readonly string[]): string {
    if (regions.length === 0) return ""
    if (regions.length === 1) return regions[0]!
    if (regions.length === 2) return `${regions[0]}、${regions[1]}`
    return `${regions[0]}、${regions[1]} 等 ${regions.length} 个地区`
}

/** 构造可供一键复制的标准收件文本。 */
export function formatCopyableAddress(row: {
    recipient: string
    phone: string
    fullAddress: string
}): string {
    return [row.recipient.trim(), row.phone.trim(), row.fullAddress.trim()]
        .filter(Boolean)
        .join(" ")
}

/** 展示紧凑数量 + 单位。 */
export function formatArrangementQuantity(
    quantity: string,
    unit: string,
): string {
    const compact = compactFixed(quantity)
    return unit ? `${compact} ${unit}` : compact
}

/**
 * 将后端收货安排投影为按商品与按收货人双视角摘要，
 * 供概览紧凑汇总区与详情抽屉共同消费。
 */
export function deriveArrangementSummary(
    data: ArrangementView,
    lineMeta?: Readonly<Record<string, ArrangementLineMeta>>,
): DerivedArrangementSummary {
    const groupMap = new Map<
        string,
        {
            key: string
            idSegment: string
            recipient: string
            phone: string
            regionCode: string
            regionName: string
            address: string
            fullAddress: string
            items: DerivedAddressGroupItem[]
            hasShipped: boolean
        }
    >()

    let completeLineCount = 0
    let partialLineCount = 0
    let unsetLineCount = 0
    let shippedLineCount = 0
    let totalAllocationCount = 0
    let totalShippedCount = 0
    let hasUnmatchedShipped = false

    const derivedLines: DerivedArrangementLine[] = data.lines.map((line) => {
        const meta = lineMeta?.[line.line_id]
        const unit = meta?.unit?.trim() || line.unit
        const sku = meta?.sku && meta.sku !== "无规格" ? meta.sku : undefined
        const isGift = Boolean(meta?.isGift)

        // 先按收件身份聚合已发快照（同一收件人可能分多批发货）
        const shippedByIdentity = new Map<
            string,
            {
                key: string
                recipient: string
                phone: string
                address: string
                quantities: string[]
            }
        >()
        for (const shipped of line.shipped_addresses) {
            const identity = shippedIdentityKey(
                shipped.recipient,
                shipped.phone,
                shipped.address,
            )
            const existing = shippedByIdentity.get(identity)
            if (existing) {
                existing.quantities.push(shipped.quantity)
            } else {
                shippedByIdentity.set(identity, {
                    key: shipped.key,
                    recipient: shipped.recipient,
                    phone: shipped.phone,
                    address: shipped.address,
                    quantities: [shipped.quantity],
                })
            }
        }
        totalShippedCount += line.shipped_addresses.length

        // 防御性合并同商品下的重复收件键，避免异常数据造成重复 DOM key
        const mergedAllocations = new Map<
            string,
            { sample: AddressAllocation; quantities: string[] }
        >()
        for (const row of line.allocations) {
            const key = normalizedAllocationKey(row)
            const existing = mergedAllocations.get(key)
            if (existing) {
                existing.quantities.push(row.quantity)
            } else {
                mergedAllocations.set(key, {
                    sample: row,
                    quantities: [row.quantity],
                })
            }
        }

        const matchedShippedIdentities = new Set<string>()
        const regionSet = new Set<string>()
        const allocations: DerivedAllocationRow[] = []

        for (const [key, entry] of mergedAllocations.entries()) {
            const row = entry.sample
            const fullAddress = fullDeliveryAddress(row)
            const quantity = safeSum(entry.quantities)
            const identity = shippedIdentityKey(
                row.recipient,
                row.phone,
                fullAddress,
            )
            const shippedBucket = shippedByIdentity.get(identity)
            const shippedQuantity = shippedBucket
                ? safeSum(shippedBucket.quantities)
                : "0.000000"
            if (shippedBucket) {
                matchedShippedIdentities.add(identity)
            }
            const hasShipped = isPositiveDecimal(shippedQuantity)
            const regionName = row.region_name.trim()
            if (regionName) regionSet.add(regionName)

            const idSegment = stableAddressIdSegment(key, row.phone)
            allocations.push({
                key,
                idSegment,
                recipient: row.recipient.trim(),
                phone: row.phone.trim(),
                regionCode: row.region_code.trim(),
                regionName,
                address: row.address.trim(),
                fullAddress,
                quantity,
                shippedQuantity,
                hasShipped,
            })

            const group = groupMap.get(key)
            const groupItem: DerivedAddressGroupItem = {
                lineId: line.line_id,
                itemName: line.item_name,
                sku,
                isGift,
                unit,
                quantity,
                shippedQuantity,
                hasShipped,
            }
            if (group) {
                group.items.push(groupItem)
                if (hasShipped) group.hasShipped = true
            } else {
                groupMap.set(key, {
                    key,
                    idSegment,
                    recipient: row.recipient.trim(),
                    phone: row.phone.trim(),
                    regionCode: row.region_code.trim(),
                    regionName,
                    address: row.address.trim(),
                    fullAddress,
                    items: [groupItem],
                    hasShipped,
                })
            }
        }

        const unmatchedShipped: DerivedUnmatchedShippedRow[] = []
        for (const [identity, bucket] of shippedByIdentity.entries()) {
            if (matchedShippedIdentities.has(identity)) continue
            unmatchedShipped.push({
                key: bucket.key,
                idSegment: stableAddressIdSegment(identity, bucket.phone),
                recipient: bucket.recipient.trim(),
                phone: bucket.phone.trim(),
                address: bucket.address.trim(),
                quantity: safeSum(bucket.quantities),
            })
        }
        if (unmatchedShipped.length > 0) {
            hasUnmatchedShipped = true
        }

        const allocatedQuantity = safeSum(
            allocations.map((row) => row.quantity),
        )
        const pendingQuantity = safePending(line.quantity, allocatedQuantity)
        const shippedQuantity = safeSum(
            line.shipped_addresses.map((row) => row.quantity),
        )
        const hasPending = isPositiveDecimal(pendingQuantity)
        const hasShipped = isPositiveDecimal(shippedQuantity)
        const isComplete = allocations.length > 0 && !hasPending

        if (allocations.length === 0) {
            unsetLineCount += 1
        } else if (hasPending) {
            partialLineCount += 1
        } else {
            completeLineCount += 1
        }
        if (hasShipped) {
            shippedLineCount += 1
        }
        totalAllocationCount += allocations.length

        const uniqueRegions = [...regionSet]
        return {
            lineId: line.line_id,
            itemName: line.item_name,
            sku,
            isGift,
            unit,
            quantity: line.quantity,
            allocations,
            unmatchedShipped,
            allocatedQuantity,
            pendingQuantity,
            shippedQuantity,
            hasPending,
            hasShipped,
            isComplete,
            allocatedPercent: computeAllocatedPercent(
                allocatedQuantity,
                line.quantity,
            ),
            uniqueRegions,
            recipientPreview: buildRecipientPreview(allocations),
            regionPreview: buildRegionPreview(uniqueRegions),
        }
    })

    const addressGroups: DerivedAddressGroup[] = [...groupMap.values()]
        .map((group) => {
            const units = new Set(group.items.map((item) => item.unit))
            const singleUnit =
                units.size === 1 ? group.items[0]?.unit : undefined
            const singleUnitTotal =
                singleUnit !== undefined
                    ? {
                          quantity: safeSum(
                              group.items.map((item) => item.quantity),
                          ),
                          unit: singleUnit,
                      }
                    : undefined
            return {
                key: group.key,
                idSegment: group.idSegment,
                recipient: group.recipient,
                phone: group.phone,
                regionCode: group.regionCode,
                regionName: group.regionName,
                address: group.address,
                fullAddress: group.fullAddress,
                items: group.items,
                hasShipped: group.hasShipped,
                lineCount: group.items.length,
                singleUnitTotal,
            }
        })
        .sort((left, right) => right.lineCount - left.lineCount)

    const totalLines = derivedLines.length
    const pendingLineCount = partialLineCount + unsetLineCount
    const unifiedDestination =
        totalLines > 0 &&
        pendingLineCount === 0 &&
        addressGroups.length === 1 &&
        !hasUnmatchedShipped
            ? addressGroups[0]
            : undefined

    const allUnits = new Set(derivedLines.map((line) => line.unit))
    const commonUnit = allUnits.size === 1 ? derivedLines[0]?.unit : undefined
    const singleUnitTotals =
        commonUnit !== undefined
            ? {
                  total: safeSum(derivedLines.map((line) => line.quantity)),
                  allocated: safeSum(
                      derivedLines.map((line) => line.allocatedQuantity),
                  ),
                  pending: safeSum(
                      derivedLines.map((line) => line.pendingQuantity),
                  ),
                  shipped: safeSum(
                      derivedLines.map((line) => line.shippedQuantity),
                  ),
                  unit: commonUnit,
              }
            : undefined

    const overallStatus =
        totalLines > 0 && pendingLineCount === 0
            ? "complete"
            : totalAllocationCount === 0
              ? "unset"
              : "partial"

    return {
        lines: derivedLines,
        addressGroups,
        totalLines,
        completeLineCount,
        partialLineCount,
        unsetLineCount,
        pendingLineCount,
        shippedLineCount,
        uniqueAddressCount: addressGroups.length,
        totalAllocationCount,
        totalShippedCount,
        unifiedDestination,
        singleUnitTotals,
        overallStatus,
    }
}
