import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { DeliveryTerm, PurchaseDeliveryAllocation } from "../../types"
import {
    sourcingQuantityStep,
    sourcingUnitQuantityError,
} from "../sourcing-quantity"
import {
    deliveryKey,
    remainingDeliveryAllocations,
    supplierDeliveryRequired,
} from "./delivery"
import {
    deliveryQuantity,
    selectedDeliveryRows,
    usesAddressQuantity,
} from "./delivery-editor"
import {
    addressDeliverable,
    addressDeliveryTerm,
    sourcingRouteLabel,
} from "./delivery-terms"
import { findSourcingOption } from "./options"
import { formatQuantityUnits, minimumUnits, quantityUnits } from "./quantity"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSupplierOption,
} from "./types"

export type RegionParts = Readonly<{
    province: string
    cityOrDistrict: string
    full: string
}>

const PROVINCE_PATTERN =
    /^(北京市|天津市|上海市|重庆市|[^省]+省|[^区]+自治区|[^区]+特别行政区)/

/** 从地区全称解析省/直辖市与市/区片段，便于 100+ 地址按地区快速分组。 */
export function parseRegionParts(
    regionName?: string | null,
    regionCode?: string,
): RegionParts {
    const full = regionName?.trim() ?? ""
    if (!full) {
        const fallback = regionCode?.trim()
            ? `地区 ${regionCode.trim()}`
            : "未标明地区"
        return { province: fallback, cityOrDistrict: fallback, full: fallback }
    }
    const matched = full.match(PROVINCE_PATTERN)
    if (!matched) {
        return { province: full, cityOrDistrict: full, full }
    }
    const province = matched[1]!
    const rest = full.slice(province.length).trim()
    return {
        province,
        cityOrDistrict: rest || province,
        full,
    }
}

export function isPositiveQuantity(raw: string): boolean {
    try {
        return compareDecimal(compactFixed(raw.trim() || "0"), "0", 6) > 0
    } catch {
        return false
    }
}

export type DirectSourceInfo = Readonly<{
    index: number
    sourceNumber: number
    rowKey: string
    line: SourcingLineInput
    option: SourcingSupplierOption
    supplierName: string
    routeLabel: string
    shortLabel: string
    selected: boolean
    assignedAddressCount: number
    assignedTotalQty: string
    notDeliverableCount: number
    freightExtraCount: number
}>

export type NonDirectSourceInfo = Readonly<{
    index: number
    sourceNumber: number
    rowKey: string
    line: SourcingLineInput
    option: SourcingSupplierOption
    name: string
    routeLabel: string
    shortLabel: string
    selected: boolean
    isStock: boolean
    quantity: string
    maxCreateQuantity: string
}>

export type AddressAllocationSlice = Readonly<{
    lineIndex: number
    sourceNumber: number
    rowKey: string
    supplierName: string
    shortLabel: string
    selected: boolean
    quantity: string
    positive: boolean
    option: SourcingSupplierOption
    term: DeliveryTerm | undefined
    deliverable: boolean
    error?: string
}>

export type GlobalAddressEntry = Readonly<{
    key: string
    baseRow: PurchaseDeliveryAllocation
    demandQty: string
    fullAddress: string
    region: RegionParts
    slices: readonly AddressAllocationSlice[]
    activeSlices: readonly AddressAllocationSlice[]
    assignedTotal: string
    remainingUnassigned: string
    overAllocated: boolean
    hasError: boolean
    effectiveTerm: DeliveryTerm | undefined
    hasNotDeliverable: boolean
    hasFreightExtra: boolean
    anySourceDeliverable: boolean
    deliverableByPrimary: boolean
}>

export type RegionFacetItem = Readonly<{
    key: string
    label: string
    totalCount: number
    fullyAssignedCount: number
    unassignedCount: number
    notDeliverableCount: number
    freightExtraCount: number
    totalDemandQty: string
}>

export type RegionFacetGroup = Readonly<{
    level: "province" | "region" | "none"
    items: readonly RegionFacetItem[]
}>

export type GlobalAddressState = Readonly<{
    directSources: readonly DirectSourceInfo[]
    nonDirectSources: readonly NonDirectSourceInfo[]
    primaryDirectSource?: DirectSourceInfo
    nonDirectTotalQty: string
    hasNonDirectSources: boolean
    directTargetQty: string
    directAssignedTotalQty: string
    /** 供应商直发已分总量超出直发额度（总需求 - 仓库/库存承担量）的部分；未超出时为 "0"。 */
    directOverTargetQty: string
    /** 供应商直发尚可分配的缺口额度；已满或超出时为 "0"。 */
    directShortfallQty: string
    /** 直发已分总量是否恰好等于直发目标额度（含混合仓发场景）。 */
    directBalanced: boolean
    entries: readonly GlobalAddressEntry[]
    regionFacets: RegionFacetGroup
    counts: Readonly<{
        total: number
        assigned: number
        unassigned: number
        issues: number
        notDeliverable: number
        freightExtra: number
    }>
}>

export type LineUpdatePatch = Readonly<{
    index: number
    nextLine: SourcingLineInput
}>

function sourceAssignedMap(
    product: SourcingProductLine,
    line: SourcingLineInput,
): Map<string, PurchaseDeliveryAllocation> {
    const rows = line.deliveryAllocations ?? selectedDeliveryRows(product, line)
    return new Map(rows.map((row) => [deliveryKey(row), row]))
}

function safeAdd(values: readonly string[]): string {
    if (values.length === 0) return "0"
    try {
        return compactFixed(sumFixed(values, { maxScale: 6, outputScale: 6 }))
    } catch {
        return "0"
    }
}

function safePositiveDiff(left: string, right: string): string {
    try {
        const diff = subtractFixed(left, right, {
            maxScale: 6,
            outputScale: 6,
        })
        return compareDecimal(diff, "0", 6) > 0 ? compactFixed(diff) : "0"
    } catch {
        return "0"
    }
}

/**
 * 构建当前商品的「以地址为中心」全局分配视图模型：
 * 汇总该商品全部直发来源、非直发（库存/入仓）承担量、各地址归属切片与地区分组统计。
 */
export function computeGlobalAddressState(
    product: SourcingProductLine,
    productAllocations: readonly { line: SourcingLineInput; index: number }[],
): GlobalAddressState {
    const directCandidates = productAllocations.flatMap(
        ({ line, index }, pos) => {
            if (!usesAddressQuantity(product, line)) return []
            const option = findSourcingOption(product, line.basisId)
            if (!option) return []
            return [{ line, index, option, sourceNumber: pos + 1 }]
        },
    )

    const multiSource = productAllocations.length > 1

    const nonDirectSources: NonDirectSourceInfo[] = productAllocations.flatMap(
        ({ line, index }, pos) => {
            if (usesAddressQuantity(product, line)) return []
            const option = findSourcingOption(product, line.basisId)
            if (!option) return []
            const isStock = option.sourceType === "EXISTING_STOCK"
            const name = isStock
                ? option.warehouseName || option.supplierName
                : option.supplierName
            const routeLabel = sourcingRouteLabel(option)
            const shortLabel = multiSource
                ? `源${pos + 1} · ${name}（${routeLabel}）`
                : `${name}（${routeLabel}）`
            return [
                {
                    index,
                    sourceNumber: pos + 1,
                    rowKey: line.rowKey,
                    line,
                    option,
                    name,
                    routeLabel,
                    shortLabel,
                    selected: line.selected,
                    isStock,
                    quantity: line.quantity?.trim() || "0",
                    maxCreateQuantity: isStock
                        ? (option.sourceAvailableQuantity ??
                          option.maxCreateQuantity)
                        : option.maxCreateQuantity,
                },
            ]
        },
    )

    const nonDirectSelected = productAllocations.filter(
        ({ line }) => line.selected && !usesAddressQuantity(product, line),
    )
    const nonDirectTotalQty = safeAdd(
        nonDirectSelected.map(({ line }) => line.quantity || "0"),
    )
    const hasNonDirectSources = nonDirectSelected.length > 0
    const directTargetQty = safePositiveDiff(
        product.remainingQuantity,
        nonDirectTotalQty,
    )

    const baseRemaining = remainingDeliveryAllocations(product).filter((row) =>
        isPositiveQuantity(row.quantity),
    )
    const baseKeys = new Set(baseRemaining.map(deliveryKey))
    const assignedMaps = new Map<
        string,
        Map<string, PurchaseDeliveryAllocation>
    >()
    const extraDraftAddresses: PurchaseDeliveryAllocation[] = []
    const extraSeen = new Set<string>()

    for (const candidate of directCandidates) {
        const map = sourceAssignedMap(product, candidate.line)
        assignedMaps.set(candidate.line.rowKey, map)
        for (const [key, row] of map.entries()) {
            if (!baseKeys.has(key) && !extraSeen.has(key)) {
                extraSeen.add(key)
                extraDraftAddresses.push(row)
            }
        }
    }

    const allAddresses = [...baseRemaining, ...extraDraftAddresses]

    const directSources: DirectSourceInfo[] = directCandidates.map((item) => {
        const map = assignedMaps.get(item.line.rowKey)!
        let assignedAddressCount = 0
        const qtyValues: string[] = []
        let notDeliverableCount = 0
        let freightExtraCount = 0

        for (const addr of allAddresses) {
            const key = deliveryKey(addr)
            const assigned = map.get(key)
            if (assigned && isPositiveQuantity(assigned.quantity)) {
                assignedAddressCount += 1
                qtyValues.push(compactFixed(assigned.quantity.trim()))
            }
            const term = addressDeliveryTerm(item.option, addr.region_code)
            if (term === "NOT_DELIVERABLE") notDeliverableCount += 1
            else if (term === "FREIGHT_EXTRA") freightExtraCount += 1
        }

        const routeLabel = sourcingRouteLabel(item.option)
        const shortLabel = multiSource
            ? `源${item.sourceNumber} · ${item.option.supplierName}`
            : item.option.supplierName

        return {
            index: item.index,
            sourceNumber: item.sourceNumber,
            rowKey: item.line.rowKey,
            line: item.line,
            option: item.option,
            supplierName: item.option.supplierName,
            routeLabel,
            shortLabel,
            selected: item.line.selected,
            assignedAddressCount,
            assignedTotalQty: safeAdd(qtyValues),
            notDeliverableCount,
            freightExtraCount,
        }
    })

    const primaryDirectSource =
        directSources.find((src) => src.selected) ?? directSources[0]

    const entries: GlobalAddressEntry[] = allAddresses.map((baseRow) => {
        const key = deliveryKey(baseRow)
        const demandQty = baseKeys.has(key)
            ? compactFixed(baseRow.quantity)
            : "0"
        const region = parseRegionParts(
            baseRow.region_name,
            baseRow.region_code,
        )

        const rawSlices: AddressAllocationSlice[] = []
        for (const src of directSources) {
            const map = assignedMaps.get(src.rowKey)!
            const assigned = map.get(key)
            if (!assigned) continue
            const rawQty = assigned.quantity
            const trimmed = rawQty.trim()
            const positive = isPositiveQuantity(rawQty)
            const term = addressDeliveryTerm(src.option, baseRow.region_code)
            const deliverable = addressDeliverable(
                src.option,
                baseRow.region_code,
            )

            let error: string | undefined
            if (trimmed !== "") {
                let normalized = trimmed
                let isZero = false
                try {
                    normalized = compactFixed(trimmed)
                    isZero = compareDecimal(normalized, "0", 6) === 0
                } catch {
                    // 非法输入交给单位精度检查给出中文提示
                }
                if (!isZero) {
                    error = sourcingUnitQuantityError(
                        normalized,
                        product.quantityScale,
                    )
                    if (!error && !deliverable) {
                        error = `${src.supplierName} 不配送此地区，请取消或改派其他来源`
                    }
                }
            }

            rawSlices.push({
                lineIndex: src.index,
                sourceNumber: src.sourceNumber,
                rowKey: src.rowKey,
                supplierName: src.supplierName,
                shortLabel: src.shortLabel,
                selected: src.selected,
                quantity: rawQty,
                positive,
                option: src.option,
                term,
                deliverable,
                error,
            })
        }

        const activeSlices = rawSlices.filter(
            (slice) => slice.selected && slice.positive,
        )
        const assignedTotal = safeAdd(
            activeSlices.map((slice) => compactFixed(slice.quantity.trim())),
        )
        const overAllocated = (() => {
            try {
                return compareDecimal(assignedTotal, demandQty, 6) > 0
            } catch {
                return false
            }
        })()
        const remainingUnassigned = overAllocated
            ? "0"
            : safePositiveDiff(demandQty, assignedTotal)

        const slices = overAllocated
            ? rawSlices.map((slice) =>
                  slice.selected && slice.positive && !slice.error
                      ? {
                            ...slice,
                            error: `该地址各来源合计 ${assignedTotal} ${product.unitName ?? ""}，超过需求 ${demandQty} ${product.unitName ?? ""}`,
                        }
                      : slice,
              )
            : rawSlices

        // 计算该地址当前有效的配送判定：已分配时看实际承接来源，未分配时看主来源
        const termsToCheck: (DeliveryTerm | undefined)[] =
            activeSlices.length > 0
                ? activeSlices.map((slice) => slice.term)
                : primaryDirectSource
                  ? [
                        addressDeliveryTerm(
                            primaryDirectSource.option,
                            baseRow.region_code,
                        ),
                    ]
                  : []

        const effectiveTerm: DeliveryTerm | undefined = termsToCheck.includes(
            "NOT_DELIVERABLE",
        )
            ? "NOT_DELIVERABLE"
            : termsToCheck.includes("FREIGHT_EXTRA")
              ? "FREIGHT_EXTRA"
              : termsToCheck.includes("FREIGHT_INCLUDED")
                ? "FREIGHT_INCLUDED"
                : undefined

        const hasNotDeliverable = effectiveTerm === "NOT_DELIVERABLE"
        const hasFreightExtra = effectiveTerm === "FREIGHT_EXTRA"
        const hasError = overAllocated || slices.some((s) => Boolean(s.error))
        const anySourceDeliverable = directSources.some((src) =>
            addressDeliverable(src.option, baseRow.region_code),
        )
        const deliverableByPrimary = primaryDirectSource
            ? addressDeliverable(
                  primaryDirectSource.option,
                  baseRow.region_code,
              )
            : false

        return {
            key,
            baseRow,
            demandQty,
            fullAddress: fullDeliveryAddress(baseRow),
            region,
            slices,
            activeSlices,
            assignedTotal,
            remainingUnassigned,
            overAllocated,
            hasError,
            effectiveTerm,
            hasNotDeliverable,
            hasFreightExtra,
            anySourceDeliverable,
            deliverableByPrimary,
        }
    })

    const regionFacets = buildRegionFacets(entries)

    const assignedCount = entries.filter((e) =>
        isPositiveQuantity(e.assignedTotal),
    ).length
    const unassignedCount = entries.filter((e) =>
        isPositiveQuantity(e.remainingUnassigned),
    ).length
    const notDeliverableCount = entries.filter(
        (e) => e.hasNotDeliverable,
    ).length
    const freightExtraCount = entries.filter((e) => e.hasFreightExtra).length
    const issuesCount = entries.filter(
        (e) => e.hasError || e.hasNotDeliverable || e.hasFreightExtra,
    ).length

    const directAssignedTotalQty = safeAdd(
        directSources
            .filter((src) => src.selected)
            .map((src) => src.assignedTotalQty),
    )
    const directOverTargetQty = safePositiveDiff(
        directAssignedTotalQty,
        directTargetQty,
    )
    const directShortfallQty = safePositiveDiff(
        directTargetQty,
        directAssignedTotalQty,
    )
    const directBalanced =
        !isPositiveQuantity(directOverTargetQty) &&
        !isPositiveQuantity(directShortfallQty)

    return {
        directSources,
        nonDirectSources,
        primaryDirectSource,
        nonDirectTotalQty,
        hasNonDirectSources,
        directTargetQty,
        directAssignedTotalQty,
        directOverTargetQty,
        directShortfallQty,
        directBalanced,
        entries,
        regionFacets,
        counts: {
            total: entries.length,
            assigned: assignedCount,
            unassigned: unassignedCount,
            issues: issuesCount,
            notDeliverable: notDeliverableCount,
            freightExtra: freightExtraCount,
        },
    }
}

/** 自动按省聚合；全在同省时下钻到市/区；全在同一区时返回 none 避免无意义分组。 */
export function buildRegionFacets(
    entries: readonly GlobalAddressEntry[],
): RegionFacetGroup {
    if (entries.length === 0) return { level: "none", items: [] }
    const provinces = new Set(entries.map((e) => e.region.province))
    let level: "province" | "region" | "none" = "none"
    if (provinces.size >= 2) {
        level = "province"
    } else {
        const regions = new Set(entries.map((e) => e.region.full))
        if (regions.size >= 2) level = "region"
    }
    if (level === "none") return { level: "none", items: [] }

    const buckets = new Map<
        string,
        {
            key: string
            label: string
            totalCount: number
            fullyAssignedCount: number
            unassignedCount: number
            notDeliverableCount: number
            freightExtraCount: number
            demands: string[]
        }
    >()

    for (const entry of entries) {
        const key =
            level === "province" ? entry.region.province : entry.region.full
        const label =
            level === "province"
                ? entry.region.province
                : entry.region.cityOrDistrict
        const bucket = buckets.get(key) ?? {
            key,
            label,
            totalCount: 0,
            fullyAssignedCount: 0,
            unassignedCount: 0,
            notDeliverableCount: 0,
            freightExtraCount: 0,
            demands: [],
        }
        bucket.totalCount += 1
        if (isPositiveQuantity(entry.remainingUnassigned)) {
            bucket.unassignedCount += 1
        } else if (isPositiveQuantity(entry.assignedTotal)) {
            bucket.fullyAssignedCount += 1
        }
        if (entry.hasNotDeliverable) bucket.notDeliverableCount += 1
        if (entry.hasFreightExtra) bucket.freightExtraCount += 1
        bucket.demands.push(entry.demandQty)
        buckets.set(key, bucket)
    }

    const items: RegionFacetItem[] = [...buckets.values()]
        .map((bucket) => ({
            key: bucket.key,
            label: bucket.label,
            totalCount: bucket.totalCount,
            fullyAssignedCount: bucket.fullyAssignedCount,
            unassignedCount: bucket.unassignedCount,
            notDeliverableCount: bucket.notDeliverableCount,
            freightExtraCount: bucket.freightExtraCount,
            totalDemandQty: safeAdd(bucket.demands),
        }))
        .sort((a, b) => {
            // 优先把有不配送/运费异常的地区排在前面，其次按地址数降序
            const aAlert = a.notDeliverableCount * 10 + a.freightExtraCount
            const bAlert = b.notDeliverableCount * 10 + b.freightExtraCount
            if (aAlert > 0 !== bAlert > 0) return aAlert > 0 ? -1 : 1
            if (b.totalCount !== a.totalCount)
                return b.totalCount - a.totalCount
            return a.label.localeCompare(b.label, "zh-CN")
        })

    return { level, items }
}

/** 获取指定地址的地区筛选键。 */
export function entryRegionFacetKey(
    entry: GlobalAddressEntry,
    level: RegionFacetGroup["level"],
): string {
    return level === "province" ? entry.region.province : entry.region.full
}

function normalizeRowsForLine(
    product: SourcingProductLine,
    line: SourcingLineInput,
): PurchaseDeliveryAllocation[] {
    return (
        line.deliveryAllocations ?? selectedDeliveryRows(product, line)
    ).map((row) => ({ ...row }))
}

function buildLinePatch(
    index: number,
    line: SourcingLineInput,
    rows: PurchaseDeliveryAllocation[],
): LineUpdatePatch {
    return {
        index,
        nextLine: {
            ...line,
            deliveryAllocations: rows,
            quantity: deliveryQuantity(rows),
        },
    }
}

/**
 * 批量把指定地址分配/改派给某个直发来源。
 * - `move_full`：跨源一键改派（自动从同商品其他直发来源扣减，并按完整需求分给目标源）。
 * - `fill_remaining`：仅将尚未分配的余量填入目标源，不改动其他来源已有分配。
 */
export function buildBatchAssignPatches(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    targetRowKey: string,
    addressKeys: ReadonlySet<string>,
    mode: "move_full" | "fill_remaining",
): {
    patches: LineUpdatePatch[]
    assignedCount: number
    skippedUndeliverableCount: number
} {
    const productLines = allLines
        .map((line, index) => ({ line, index }))
        .filter(
            ({ line }) =>
                line.salesOrderLineId === product.salesOrderLineId &&
                supplierDeliveryRequired(product, line),
        )
    const targetEntry = productLines.find(
        ({ line }) => line.rowKey === targetRowKey,
    )
    if (!targetEntry) {
        return { patches: [], assignedCount: 0, skippedUndeliverableCount: 0 }
    }
    const targetOption = findSourcingOption(product, targetEntry.line.basisId)
    if (!targetOption) {
        return { patches: [], assignedCount: 0, skippedUndeliverableCount: 0 }
    }

    const remainingMap = new Map(
        remainingDeliveryAllocations(product).map((row) => [
            deliveryKey(row),
            row,
        ]),
    )
    const lineRowsMap = new Map<
        string,
        {
            index: number
            line: SourcingLineInput
            rows: Map<string, PurchaseDeliveryAllocation>
        }
    >()
    for (const item of productLines) {
        const rows = normalizeRowsForLine(product, item.line)
        lineRowsMap.set(item.line.rowKey, {
            index: item.index,
            line: item.line,
            rows: new Map(rows.map((r) => [deliveryKey(r), r])),
        })
    }

    const targetState = lineRowsMap.get(targetRowKey)!
    let assignedCount = 0
    let skippedUndeliverableCount = 0

    for (const key of addressKeys) {
        const base = remainingMap.get(key)
        if (!base || !isPositiveQuantity(base.quantity)) continue
        if (!addressDeliverable(targetOption, base.region_code)) {
            skippedUndeliverableCount += 1
            continue
        }

        if (mode === "move_full") {
            // 从同商品其他直发来源中移除此地址，实现一步改派
            for (const [rk, state] of lineRowsMap.entries()) {
                if (rk === targetRowKey) continue
                state.rows.delete(key)
            }
            targetState.rows.set(key, {
                ...base,
                quantity: compactFixed(base.quantity),
            })
            assignedCount += 1
        } else {
            // 仅填充当前尚未分配的余量
            const othersQty: string[] = []
            for (const [rk, state] of lineRowsMap.entries()) {
                if (rk === targetRowKey || !state.line.selected) continue
                const row = state.rows.get(key)
                if (row && isPositiveQuantity(row.quantity)) {
                    othersQty.push(compactFixed(row.quantity.trim()))
                }
            }
            const usedByOthers = safeAdd(othersQty)
            const cap = safePositiveDiff(base.quantity, usedByOthers)
            if (!isPositiveQuantity(cap)) continue
            targetState.rows.set(key, { ...base, quantity: cap })
            assignedCount += 1
        }
    }

    const patches: LineUpdatePatch[] = []
    for (const state of lineRowsMap.values()) {
        patches.push(
            buildLinePatch(state.index, state.line, [...state.rows.values()]),
        )
    }
    return { patches, assignedCount, skippedUndeliverableCount }
}

/** 批量清空指定地址在所有直发来源（或指定来源）下的分配。 */
export function buildClearAddressesPatches(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    addressKeys: ReadonlySet<string>,
    onlyRowKey?: string,
): LineUpdatePatch[] {
    const patches: LineUpdatePatch[] = []
    allLines.forEach((line, index) => {
        if (
            line.salesOrderLineId !== product.salesOrderLineId ||
            !supplierDeliveryRequired(product, line)
        ) {
            return
        }
        if (onlyRowKey && line.rowKey !== onlyRowKey) return
        const rows = normalizeRowsForLine(product, line).filter(
            (row) => !addressKeys.has(deliveryKey(row)),
        )
        patches.push(buildLinePatch(index, line, rows))
    })
    return patches
}

/** 更新某个地址在指定来源上的分配数量。 */
export function buildSetSliceQuantityPatch(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    targetRowKey: string,
    baseRow: PurchaseDeliveryAllocation,
    nextQty: string,
    removeIfZero = false,
): LineUpdatePatch | null {
    const index = allLines.findIndex((line) => line.rowKey === targetRowKey)
    if (index < 0) return null
    const line = allLines[index]!
    const rows = normalizeRowsForLine(product, line)
    const key = deliveryKey(baseRow)
    const trimmed = nextQty.trim()

    const isZeroOrEmpty =
        trimmed === "" ||
        (() => {
            try {
                return compareDecimal(compactFixed(trimmed), "0", 6) === 0
            } catch {
                return false
            }
        })()

    if (removeIfZero && isZeroOrEmpty) {
        const nextRows = rows.filter((item) => deliveryKey(item) !== key)
        return buildLinePatch(index, line, nextRows)
    }

    const exists = rows.some((item) => deliveryKey(item) === key)
    const nextRows = exists
        ? rows.map((item) =>
              deliveryKey(item) === key ? { ...item, quantity: nextQty } : item,
          )
        : [...rows, { ...baseRow, quantity: nextQty }]

    return buildLinePatch(index, line, nextRows)
}

/** 为指定直发来源在某地址上增量填充未覆盖数量（若已有分配则在原量上叠加，不覆盖原值）。 */
export function buildAddSliceQuantityPatch(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    targetRowKey: string,
    baseRow: PurchaseDeliveryAllocation,
    addQty: string,
): LineUpdatePatch | null {
    if (!isPositiveQuantity(addQty)) return null
    const index = allLines.findIndex((line) => line.rowKey === targetRowKey)
    if (index < 0) return null
    const line = allLines[index]!
    const rows = normalizeRowsForLine(product, line)
    const key = deliveryKey(baseRow)
    const existing = rows.find((item) => deliveryKey(item) === key)
    const existingQty =
        existing && isPositiveQuantity(existing.quantity)
            ? compactFixed(existing.quantity.trim())
            : "0"
    const nextQty = safeAdd([existingQty, compactFixed(addQty.trim())])
    return buildSetSliceQuantityPatch(
        product,
        allLines,
        targetRowKey,
        baseRow,
        nextQty,
    )
}

export type QuotaAllocationPreview = Readonly<{
    patches: readonly LineUpdatePatch[]
    allocatedQty: string
    allocatedAddressCount: number
    shortfallQty: string
    valid: boolean
    error?: string
}>

/**
 * 按目标总量自动为指定来源凑单分配（面向 100+ 同城/大批量地址快速按量拆分）：
 * - 作用域内先释放该来源原有分配，优先从尚未分配的地址中按目标总量贪心凑单；
 * - 当开启 `allowReallocateFromOtherDirect` 且未分配余量不足时，按序从同商品其他直发来源调拨可配地址，
 *   并同步更新被调拨来源的明细（确保多直发来源之间总量守恒、零超分）；
 * - 优先选择包邮/普通配送地址，其次选择运费另计地址，自动跳过不配送地址；
 * - 严格使用六位定点单位与最小计量步长，整件商品绝不拆出小数。
 */
export function computeQuotaAutoAllocation(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    targetRowKey: string,
    rawTargetQty: string,
    options: {
        scopeAddressKeys?: ReadonlySet<string>
        preferFreightIncluded: boolean
        allowReallocateFromOtherDirect?: boolean
    },
): QuotaAllocationPreview {
    const trimmed = rawTargetQty.trim()
    if (!trimmed) {
        return {
            patches: [],
            allocatedQty: "0",
            allocatedAddressCount: 0,
            shortfallQty: "0",
            valid: false,
            error: "请输入目标数量",
        }
    }

    const qtyError = sourcingUnitQuantityError(trimmed, product.quantityScale)
    if (qtyError) {
        return {
            patches: [],
            allocatedQty: "0",
            allocatedAddressCount: 0,
            shortfallQty: "0",
            valid: false,
            error: qtyError,
        }
    }

    const stepStr = sourcingQuantityStep(product.quantityScale) ?? "1"
    const stepUnits = quantityUnits(stepStr)
    const targetUnits = quantityUnits(compactFixed(trimmed))

    const targetIndex = allLines.findIndex(
        (line) => line.rowKey === targetRowKey,
    )
    if (targetIndex < 0) {
        return {
            patches: [],
            allocatedQty: "0",
            allocatedAddressCount: 0,
            shortfallQty: compactFixed(trimmed),
            valid: false,
            error: "未找到目标供货来源",
        }
    }

    const targetLine = allLines[targetIndex]!
    const targetOption = findSourcingOption(product, targetLine.basisId)
    if (!targetOption) {
        return {
            patches: [],
            allocatedQty: "0",
            allocatedAddressCount: 0,
            shortfallQty: compactFixed(trimmed),
            valid: false,
            error: "请先为目标来源选择供应商",
        }
    }

    // 统计同商品其他已启用直发来源对每个地址的占用，并保留可变快照用于按需调拨
    const otherDirectStates: Array<{
        index: number
        line: SourcingLineInput
        rows: Map<string, PurchaseDeliveryAllocation>
        dirty: boolean
    }> = []
    const usedByOthers = new Map<string, bigint>()
    allLines.forEach((line, index) => {
        if (
            !line.selected ||
            line.rowKey === targetRowKey ||
            line.salesOrderLineId !== product.salesOrderLineId ||
            !supplierDeliveryRequired(product, line)
        ) {
            return
        }
        const rows = normalizeRowsForLine(product, line)
        const map = new Map<string, PurchaseDeliveryAllocation>()
        for (const row of rows) {
            const key = deliveryKey(row)
            map.set(key, { ...row })
            if (!isPositiveQuantity(row.quantity)) continue
            try {
                const units = quantityUnits(compactFixed(row.quantity.trim()))
                usedByOthers.set(
                    key,
                    (usedByOthers.get(key) ?? BigInt(0)) + units,
                )
            } catch {
                // 跳过非法值
            }
        }
        otherDirectStates.push({ index, line, rows: map, dirty: false })
    })

    const baseRemaining = remainingDeliveryAllocations(product).filter((row) =>
        isPositiveQuantity(row.quantity),
    )

    // 保留作用域之外的现有分配（若用户仅针对某个省份/筛选集按量分配）
    const existingRows = normalizeRowsForLine(product, targetLine)
    const preservedOutsideScope: PurchaseDeliveryAllocation[] = []
    if (options.scopeAddressKeys) {
        for (const row of existingRows) {
            if (!options.scopeAddressKeys.has(deliveryKey(row))) {
                preservedOutsideScope.push(row)
            }
        }
    }

    type Candidate = {
        row: PurchaseDeliveryAllocation
        key: string
        freeUnits: bigint
        stealableUnits: bigint
        isFreightExtra: boolean
    }

    const candidates: Candidate[] = baseRemaining
        .map((row) => {
            const key = deliveryKey(row)
            if (
                options.scopeAddressKeys &&
                !options.scopeAddressKeys.has(key)
            ) {
                return null
            }
            if (!addressDeliverable(targetOption, row.region_code)) {
                return null
            }
            const demandUnits = quantityUnits(compactFixed(row.quantity))
            const others = usedByOthers.get(key) ?? BigInt(0)
            const freeUnits =
                demandUnits > others ? demandUnits - others : BigInt(0)
            const stealableUnits = options.allowReallocateFromOtherDirect
                ? minimumUnits(demandUnits, others)
                : BigInt(0)
            if (freeUnits + stealableUnits < stepUnits) return null
            const term = addressDeliveryTerm(targetOption, row.region_code)
            return {
                row,
                key,
                freeUnits,
                stealableUnits,
                isFreightExtra: term === "FREIGHT_EXTRA",
            }
        })
        .filter((item): item is Candidate => item !== null)

    if (options.preferFreightIncluded) {
        candidates.sort((a, b) => {
            if (a.isFreightExtra !== b.isFreightExtra) {
                return a.isFreightExtra ? 1 : -1
            }
            return 0
        })
    }

    let remainingQuota = targetUnits
    let allocatedUnits = BigInt(0)
    const allocatedByKey = new Map<
        string,
        { row: PurchaseDeliveryAllocation; units: bigint }
    >()

    const recordAllocation = (
        row: PurchaseDeliveryAllocation,
        key: string,
        units: bigint,
    ) => {
        if (units <= BigInt(0)) return
        const current = allocatedByKey.get(key)
        allocatedByKey.set(key, {
            row,
            units: (current?.units ?? BigInt(0)) + units,
        })
        allocatedUnits += units
        remainingQuota -= units
    }

    // 第一轮：优先消化未分配余量，不改动其他来源
    for (const candidate of candidates) {
        if (remainingQuota < stepUnits) break
        if (candidate.freeUnits < stepUnits) continue
        const rawTake = minimumUnits(candidate.freeUnits, remainingQuota)
        const take = (rawTake / stepUnits) * stepUnits
        recordAllocation(candidate.row, candidate.key, take)
    }

    // 第二轮：若允许跨直发源调拨且仍有目标余量，则从其他直发源扣减可配地址
    if (options.allowReallocateFromOtherDirect && remainingQuota >= stepUnits) {
        for (const candidate of candidates) {
            if (remainingQuota < stepUnits) break
            if (candidate.stealableUnits < stepUnits) continue
            let need =
                (minimumUnits(candidate.stealableUnits, remainingQuota) /
                    stepUnits) *
                stepUnits
            if (need <= BigInt(0)) continue

            let stolen = BigInt(0)
            for (const other of otherDirectStates) {
                if (need < stepUnits) break
                const otherRow = other.rows.get(candidate.key)
                if (!otherRow || !isPositiveQuantity(otherRow.quantity))
                    continue
                let otherUnits = BigInt(0)
                try {
                    otherUnits = quantityUnits(
                        compactFixed(otherRow.quantity.trim()),
                    )
                } catch {
                    continue
                }
                const takeFromOther =
                    (minimumUnits(otherUnits, need) / stepUnits) * stepUnits
                if (takeFromOther <= BigInt(0)) continue
                const leftUnits = otherUnits - takeFromOther
                if (leftUnits <= BigInt(0)) {
                    other.rows.delete(candidate.key)
                } else {
                    other.rows.set(candidate.key, {
                        ...otherRow,
                        quantity: formatQuantityUnits(leftUnits),
                    })
                }
                other.dirty = true
                stolen += takeFromOther
                need -= takeFromOther
            }
            recordAllocation(candidate.row, candidate.key, stolen)
        }
    }

    const allocatedInScope: PurchaseDeliveryAllocation[] = [
        ...allocatedByKey.values(),
    ].map(({ row, units }) => ({
        ...row,
        quantity: formatQuantityUnits(units),
    }))

    const nextRows = [...preservedOutsideScope, ...allocatedInScope]
    const patches: LineUpdatePatch[] = [
        buildLinePatch(targetIndex, targetLine, nextRows),
    ]
    for (const other of otherDirectStates) {
        if (!other.dirty) continue
        patches.push(
            buildLinePatch(other.index, other.line, [...other.rows.values()]),
        )
    }

    return {
        patches,
        allocatedQty: formatQuantityUnits(allocatedUnits),
        allocatedAddressCount: allocatedInScope.length,
        shortfallQty: formatQuantityUnits(remainingQuota),
        valid: true,
    }
}

/**
 * 一键缩减直发超出额度的地址分配（典型场景：仓库/集采入仓承担了部分量，需从直发地址释放多余数量）：
 * - 优先释放「不配送」与「运费另计」的地址，再按倒序释放普通地址，保留前序已配地址的稳定性；
 * - 严格遵守商品最小计量步长。
 */
export function buildTrimDirectExcessPatches(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    rawExcessQty: string,
    onlyRowKey?: string,
): {
    patches: LineUpdatePatch[]
    trimmedQty: string
    releasedAddressCount: number
} {
    if (!isPositiveQuantity(rawExcessQty)) {
        return { patches: [], trimmedQty: "0", releasedAddressCount: 0 }
    }
    const stepStr = sourcingQuantityStep(product.quantityScale) ?? "1"
    const stepUnits = quantityUnits(stepStr)
    let remainingToTrim = quantityUnits(compactFixed(rawExcessQty.trim()))

    const states = allLines.flatMap((line, index) => {
        if (
            !line.selected ||
            line.salesOrderLineId !== product.salesOrderLineId ||
            !supplierDeliveryRequired(product, line)
        ) {
            return []
        }
        if (onlyRowKey && line.rowKey !== onlyRowKey) return []
        const option = findSourcingOption(product, line.basisId)
        if (!option) return []
        const rows = normalizeRowsForLine(product, line)
        return [
            {
                index,
                line,
                option,
                rows: new Map(rows.map((r) => [deliveryKey(r), { ...r }])),
                dirty: false,
            },
        ]
    })

    type TrimCandidate = {
        stateIndex: number
        key: string
        priority: number
        orderIndex: number
        units: bigint
    }

    const candidates: TrimCandidate[] = []
    states.forEach((state, stateIndex) => {
        let orderIndex = 0
        for (const [key, row] of state.rows.entries()) {
            orderIndex += 1
            if (!isPositiveQuantity(row.quantity)) continue
            let units = BigInt(0)
            try {
                units = quantityUnits(compactFixed(row.quantity.trim()))
            } catch {
                continue
            }
            const term = addressDeliveryTerm(state.option, row.region_code)
            // 优先裁掉不配送(0) -> 运费另计(1) -> 普通配送(2)，同类倒序裁切保留前序稳定
            const priority =
                term === "NOT_DELIVERABLE"
                    ? 0
                    : term === "FREIGHT_EXTRA"
                      ? 1
                      : 2
            candidates.push({
                stateIndex,
                key,
                priority,
                orderIndex,
                units,
            })
        }
    })

    candidates.sort((a, b) => {
        if (a.priority !== b.priority) return a.priority - b.priority
        if (b.stateIndex !== a.stateIndex) return b.stateIndex - a.stateIndex
        return b.orderIndex - a.orderIndex
    })

    let trimmedUnits = BigInt(0)
    let releasedAddressCount = 0

    for (const candidate of candidates) {
        if (remainingToTrim < stepUnits) break
        const state = states[candidate.stateIndex]!
        const row = state.rows.get(candidate.key)
        if (!row) continue
        const rawTake = minimumUnits(candidate.units, remainingToTrim)
        const take = (rawTake / stepUnits) * stepUnits
        if (take <= BigInt(0)) continue
        const left = candidate.units - take
        if (left <= BigInt(0)) {
            state.rows.delete(candidate.key)
            releasedAddressCount += 1
        } else {
            state.rows.set(candidate.key, {
                ...row,
                quantity: formatQuantityUnits(left),
            })
        }
        state.dirty = true
        trimmedUnits += take
        remainingToTrim -= take
    }

    const patches = states
        .filter((state) => state.dirty)
        .map((state) =>
            buildLinePatch(state.index, state.line, [...state.rows.values()]),
        )

    return {
        patches,
        trimmedQty: formatQuantityUnits(trimmedUnits),
        releasedAddressCount,
    }
}

/**
 * 保留指定直发来源已有地址分配，仅将尚未分配的可配送地址按缺口额度增量补齐。
 */
export function buildFillDirectShortfallPatches(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    targetRowKey: string,
    rawShortfallQty: string,
): {
    patches: LineUpdatePatch[]
    addedQty: string
    addedAddressCount: number
} {
    if (!isPositiveQuantity(rawShortfallQty)) {
        return { patches: [], addedQty: "0", addedAddressCount: 0 }
    }
    const targetIndex = allLines.findIndex(
        (line) => line.rowKey === targetRowKey,
    )
    if (targetIndex < 0) {
        return { patches: [], addedQty: "0", addedAddressCount: 0 }
    }
    const targetLine = allLines[targetIndex]!
    const targetOption = findSourcingOption(product, targetLine.basisId)
    if (!targetOption) {
        return { patches: [], addedQty: "0", addedAddressCount: 0 }
    }

    const stepStr = sourcingQuantityStep(product.quantityScale) ?? "1"
    const stepUnits = quantityUnits(stepStr)
    let remainingNeed = quantityUnits(compactFixed(rawShortfallQty.trim()))

    const usedAllDirect = new Map<string, bigint>()
    for (const line of allLines) {
        if (
            !line.selected ||
            line.salesOrderLineId !== product.salesOrderLineId ||
            !supplierDeliveryRequired(product, line)
        ) {
            continue
        }
        for (const row of normalizeRowsForLine(product, line)) {
            if (!isPositiveQuantity(row.quantity)) continue
            const key = deliveryKey(row)
            try {
                const units = quantityUnits(compactFixed(row.quantity.trim()))
                usedAllDirect.set(
                    key,
                    (usedAllDirect.get(key) ?? BigInt(0)) + units,
                )
            } catch {
                // 忽略非法值
            }
        }
    }

    const targetRows = new Map(
        normalizeRowsForLine(product, targetLine).map((r) => [
            deliveryKey(r),
            { ...r },
        ]),
    )

    const baseRemaining = remainingDeliveryAllocations(product).filter((row) =>
        isPositiveQuantity(row.quantity),
    )

    const candidates = baseRemaining
        .map((row) => {
            const key = deliveryKey(row)
            if (!addressDeliverable(targetOption, row.region_code)) return null
            const demandUnits = quantityUnits(compactFixed(row.quantity))
            const used = usedAllDirect.get(key) ?? BigInt(0)
            const freeUnits =
                demandUnits > used ? demandUnits - used : BigInt(0)
            if (freeUnits < stepUnits) return null
            const term = addressDeliveryTerm(targetOption, row.region_code)
            return {
                row,
                key,
                freeUnits,
                isFreightExtra: term === "FREIGHT_EXTRA",
            }
        })
        .filter((item): item is NonNullable<typeof item> => item !== null)
        .sort((a, b) =>
            a.isFreightExtra === b.isFreightExtra
                ? 0
                : a.isFreightExtra
                  ? 1
                  : -1,
        )

    let addedUnits = BigInt(0)
    let addedAddressCount = 0

    for (const candidate of candidates) {
        if (remainingNeed < stepUnits) break
        const rawTake = minimumUnits(candidate.freeUnits, remainingNeed)
        const take = (rawTake / stepUnits) * stepUnits
        if (take <= BigInt(0)) continue
        const existing = targetRows.get(candidate.key)
        let prevUnits = BigInt(0)
        if (existing && isPositiveQuantity(existing.quantity)) {
            try {
                prevUnits = quantityUnits(
                    compactFixed(existing.quantity.trim()),
                )
            } catch {
                prevUnits = BigInt(0)
            }
        }
        targetRows.set(candidate.key, {
            ...candidate.row,
            quantity: formatQuantityUnits(prevUnits + take),
        })
        if (prevUnits === BigInt(0)) addedAddressCount += 1
        addedUnits += take
        remainingNeed -= take
    }

    if (addedUnits <= BigInt(0)) {
        return { patches: [], addedQty: "0", addedAddressCount: 0 }
    }

    return {
        patches: [
            buildLinePatch(targetIndex, targetLine, [...targetRows.values()]),
        ],
        addedQty: formatQuantityUnits(addedUnits),
        addedAddressCount,
    }
}

/**
 * 混合履约跨模式转交：将选中地址从供应商直发来源释放，并把对应数量原子划入指定仓库/库存来源，
 * 同时受该来源可用上限与商品剩余总需求约束，避免超配。
 */
export function buildTransferAddressesToNonDirectPatches(
    product: SourcingProductLine,
    allLines: readonly SourcingLineInput[],
    targetNonDirectRowKey: string,
    addressKeys: ReadonlySet<string>,
): {
    patches: LineUpdatePatch[]
    releasedCount: number
    transferredQty: string
    cappedByMax: boolean
} {
    const targetIndex = allLines.findIndex(
        (line) => line.rowKey === targetNonDirectRowKey,
    )
    if (targetIndex < 0 || addressKeys.size === 0) {
        return {
            patches: [],
            releasedCount: 0,
            transferredQty: "0",
            cappedByMax: false,
        }
    }
    const targetLine = allLines[targetIndex]!
    const targetOption = findSourcingOption(product, targetLine.basisId)
    if (!targetOption || usesAddressQuantity(product, targetLine)) {
        return {
            patches: [],
            releasedCount: 0,
            transferredQty: "0",
            cappedByMax: false,
        }
    }

    const remainingMap = new Map(
        remainingDeliveryAllocations(product).map((row) => [
            deliveryKey(row),
            row,
        ]),
    )

    const directStates = allLines.flatMap((line, index) => {
        if (
            line.salesOrderLineId !== product.salesOrderLineId ||
            !supplierDeliveryRequired(product, line)
        ) {
            return []
        }
        const rows = normalizeRowsForLine(product, line)
        return [
            {
                index,
                line,
                rows: new Map(rows.map((r) => [deliveryKey(r), { ...r }])),
                dirty: false,
            },
        ]
    })

    let releasedDirectUnits = BigInt(0)
    let unassignedDemandUnits = BigInt(0)
    let releasedCount = 0

    for (const key of addressKeys) {
        const base = remainingMap.get(key)
        let releasedForAddr = BigInt(0)
        for (const state of directStates) {
            const row = state.rows.get(key)
            if (!row) continue
            if (state.line.selected && isPositiveQuantity(row.quantity)) {
                try {
                    releasedForAddr += quantityUnits(
                        compactFixed(row.quantity.trim()),
                    )
                } catch {
                    // 忽略非法值
                }
            }
            state.rows.delete(key)
            state.dirty = true
        }
        if (releasedForAddr > BigInt(0)) {
            releasedDirectUnits += releasedForAddr
            releasedCount += 1
        } else if (base && isPositiveQuantity(base.quantity)) {
            unassignedDemandUnits += quantityUnits(compactFixed(base.quantity))
            releasedCount += 1
        }
    }

    const desiredAddUnits = releasedDirectUnits + unassignedDemandUnits

    // 计算释放直发后，该商品还剩多少总需求空间可供目标仓库/库存承接
    const productDemandUnits = quantityUnits(product.remainingQuantity)
    let otherAllocatedUnits = BigInt(0)
    allLines.forEach((line, index) => {
        if (
            !line.selected ||
            index === targetIndex ||
            line.salesOrderLineId !== product.salesOrderLineId
        ) {
            return
        }
        const directMatch = directStates.find((s) => s.index === index)
        if (directMatch) {
            for (const row of directMatch.rows.values()) {
                if (!isPositiveQuantity(row.quantity)) continue
                try {
                    otherAllocatedUnits += quantityUnits(
                        compactFixed(row.quantity.trim()),
                    )
                } catch {
                    // 忽略非法值
                }
            }
            return
        }
        if (isPositiveQuantity(line.quantity || "0")) {
            try {
                otherAllocatedUnits += quantityUnits(
                    compactFixed(line.quantity.trim()),
                )
            } catch {
                // 忽略非法值
            }
        }
    })

    let currentTargetUnits = BigInt(0)
    if (isPositiveQuantity(targetLine.quantity || "0")) {
        try {
            currentTargetUnits = quantityUnits(
                compactFixed(targetLine.quantity.trim()),
            )
        } catch {
            currentTargetUnits = BigInt(0)
        }
    }

    const maxQtyStr =
        targetOption.sourceType === "EXISTING_STOCK"
            ? (targetOption.sourceAvailableQuantity ??
              targetOption.maxCreateQuantity)
            : targetOption.maxCreateQuantity
    const optionCapUnits = quantityUnits(maxQtyStr)
    const demandCapUnits =
        productDemandUnits > otherAllocatedUnits
            ? productDemandUnits - otherAllocatedUnits
            : BigInt(0)
    const effectiveMaxTargetUnits = minimumUnits(optionCapUnits, demandCapUnits)
    const headroom =
        effectiveMaxTargetUnits > currentTargetUnits
            ? effectiveMaxTargetUnits - currentTargetUnits
            : BigInt(0)

    const actualAddUnits = minimumUnits(desiredAddUnits, headroom)
    const nextTargetUnits = currentTargetUnits + actualAddUnits
    const cappedByMax = actualAddUnits < desiredAddUnits

    const patches: LineUpdatePatch[] = directStates
        .filter((state) => state.dirty)
        .map((state) =>
            buildLinePatch(state.index, state.line, [...state.rows.values()]),
        )

    if (actualAddUnits > BigInt(0) || !targetLine.selected) {
        patches.push({
            index: targetIndex,
            nextLine: {
                ...targetLine,
                selected: true,
                quantity:
                    nextTargetUnits > BigInt(0)
                        ? formatQuantityUnits(nextTargetUnits)
                        : targetLine.quantity,
            },
        })
    }

    return {
        patches,
        releasedCount,
        transferredQty: formatQuantityUnits(actualAddUnits),
        cappedByMax,
    }
}
