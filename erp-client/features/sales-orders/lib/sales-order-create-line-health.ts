import { regionFullName, type RegionIndex } from "@/features/regions"
import {
    compareDecimal,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import type {
    SalesLineProcurementResponsibility,
    SalesOrderDraftLineInput,
} from "../types"
import { allocationSummary } from "./sales-delivery-allocation-summary"

export type SalesLineFilterKey =
    | "all"
    | "incomplete"
    | "no_address"
    | "address_issue"
    | "negative_profit"

export type SalesLineHealth = {
    missingSku: boolean
    missingQuantity: boolean
    missingPrice: boolean
    missingSupplyPrice: boolean
    missingDueDate: boolean
    unresolvedOwner: boolean
    noAddress: boolean
    addressIssue: boolean
    incomplete: boolean
    rowProfit: string | null
    negativeProfit: boolean
}

export type FilteredSalesLineEntry = {
    line: SalesOrderDraftLineInput
    rowIndex: number
    health: SalesLineHealth
}

export type SalesLineHealthCounts = {
    all: number
    incomplete: number
    noAddress: number
    addressIssue: number
    negativeProfit: number
}

function isPositiveDecimal(value: string, maxScale: number): boolean {
    const trimmed = value.trim()
    if (!new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`).test(trimmed)) {
        return false
    }
    return /[1-9]/.test(trimmed)
}

function isNonNegativeDecimal(value: string, maxScale: number): boolean {
    const trimmed = value.trim()
    return new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`).test(trimmed)
}

/** 计算单行含税预估毛利：（成交单价 − 采购供给价）× 数量，舍入到分。 */
export function calculateRowGrossProfit(
    line: SalesOrderDraftLineInput,
): string | null {
    const supplyPrice = line.procurementSupplyPrice?.trim() ?? ""
    const sellingPrice =
        line.pricingMode === "GIFT" ? "0" : line.unitPriceGross.trim()
    if (
        !line.sku.trim() ||
        !supplyPrice ||
        !sellingPrice ||
        !isPositiveDecimal(line.quantity, 6) ||
        !isNonNegativeDecimal(supplyPrice, 4) ||
        (line.pricingMode !== "GIFT" && !isPositiveDecimal(sellingPrice, 4))
    ) {
        return null
    }
    try {
        const options = {
            leftMaxScale: 4,
            rightMaxScale: 6,
            outputScale: 10,
        }
        const diff = subtractFixed(
            multiplyFixed(sellingPrice, line.quantity, options),
            multiplyFixed(supplyPrice, line.quantity, options),
            { maxScale: 10, outputScale: 2 },
        )
        return diff
    } catch {
        return null
    }
}

export function evaluateSalesLineHealth(
    line: SalesOrderDraftLineInput,
    options?: {
        procurementOwners?: ReadonlyMap<
            string,
            SalesLineProcurementResponsibility
        >
        procurementFetching?: boolean
    },
): SalesLineHealth {
    const missingSku = !line.sku.trim()
    const missingQuantity = !isPositiveDecimal(line.quantity, 6)
    const isGift = line.pricingMode === "GIFT"
    const missingPrice = !isGift && !isPositiveDecimal(line.unitPriceGross, 4)
    const missingSupplyPrice =
        !line.procurementSupplyPrice?.trim() ||
        !isNonNegativeDecimal(line.procurementSupplyPrice, 4)
    const missingDueDate = !line.dueDate.trim()
    const unresolvedOwner = Boolean(
        line.sku.trim() &&
        !options?.procurementFetching &&
        options?.procurementOwners &&
        !options.procurementOwners.get(line.rowKey)?.resolved,
    )
    const allocations = line.deliveryAllocations ?? []
    const noAddress = allocations.length === 0
    const addressIssue =
        allocations.length > 0 && !allocationSummary(line).complete
    const rowProfit = calculateRowGrossProfit(line)
    const negativeProfit =
        rowProfit !== null && compareDecimal(rowProfit, "0", 2) < 0

    const incomplete =
        missingSku ||
        missingQuantity ||
        missingPrice ||
        missingSupplyPrice ||
        missingDueDate ||
        addressIssue

    return {
        missingSku,
        missingQuantity,
        missingPrice,
        missingSupplyPrice,
        missingDueDate,
        unresolvedOwner,
        noAddress,
        addressIssue,
        incomplete,
        rowProfit,
        negativeProfit,
    }
}

export function matchesSalesLineFilter(
    health: SalesLineHealth,
    filter: SalesLineFilterKey,
): boolean {
    switch (filter) {
        case "all":
            return true
        case "incomplete":
            return health.incomplete
        case "no_address":
            return health.noAddress
        case "address_issue":
            return health.addressIssue
        case "negative_profit":
            return health.negativeProfit
    }
}

function matchesSearchQuery(
    line: SalesOrderDraftLineInput,
    rowIndex: number,
    query: string,
    ownerName?: string,
    regions?: RegionIndex,
): boolean {
    const needle = query.trim().toLowerCase()
    if (!needle) return true
    if (String(rowIndex + 1) === needle) return true
    const fields = [
        line.name,
        line.specification ?? "",
        line.sku,
        ownerName ?? "",
    ]
    if (fields.some((value) => value.toLowerCase().includes(needle))) {
        return true
    }
    for (const allocation of line.deliveryAllocations ?? []) {
        const regionText =
            allocation.region_name ||
            (regions && allocation.region_code
                ? regionFullName(regions, allocation.region_code)
                : "")
        const addressFields = [
            allocation.recipient,
            allocation.phone,
            regionText,
            allocation.address,
        ]
        if (
            addressFields.some((value) => value.toLowerCase().includes(needle))
        ) {
            return true
        }
    }
    return false
}

export function buildFilteredSalesLines(
    lines: readonly SalesOrderDraftLineInput[],
    options: {
        query: string
        filter: SalesLineFilterKey
        procurementOwners?: ReadonlyMap<
            string,
            SalesLineProcurementResponsibility
        >
        procurementFetching?: boolean
        regions?: RegionIndex
    },
): {
    entries: FilteredSalesLineEntry[]
    counts: SalesLineHealthCounts
} {
    const counts: SalesLineHealthCounts = {
        all: lines.length,
        incomplete: 0,
        noAddress: 0,
        addressIssue: 0,
        negativeProfit: 0,
    }
    const entries: FilteredSalesLineEntry[] = []

    for (let rowIndex = 0; rowIndex < lines.length; rowIndex += 1) {
        const line = lines[rowIndex]!
        const health = evaluateSalesLineHealth(line, options)
        if (health.incomplete) counts.incomplete += 1
        if (health.noAddress) counts.noAddress += 1
        if (health.addressIssue) counts.addressIssue += 1
        if (health.negativeProfit) counts.negativeProfit += 1

        if (!matchesSalesLineFilter(health, options.filter)) continue
        const ownerName = options.procurementOwners?.get(line.rowKey)?.ownerName
        if (
            !matchesSearchQuery(
                line,
                rowIndex,
                options.query,
                ownerName,
                options.regions,
            )
        ) {
            continue
        }
        entries.push({ line, rowIndex, health })
    }

    return { entries, counts }
}
