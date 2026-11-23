import {
    compareDecimal,
    multiplyFixed,
    parseDecimal,
    sumFixed,
} from "@/lib/fixed-decimal"
import { deliveryProgress } from "./delivery-progress"
import type { DemandFormValues } from "./demand-form"
import type { InquiryLine } from "../types"

export type DemandSummary = Readonly<{
    lineCount: number
    physicalCount: number
    readyDeliveryCount: number
    pendingDeliveryCount: number
    overAllocatedCount: number
    missingDueDateCount: number
    invalidQuantityCount: number
    pricedLineCount: number
    referenceTotal: string | null
}>

export function isLineQuantityValid(line: InquiryLine): boolean {
    try {
        return compareDecimal(line.quantity, "0", line.quantity_scale) > 0
    } catch {
        return false
    }
}

export function isLineReferencePriceValid(line: InquiryLine): boolean {
    const raw = line.sales_reference_price_gross?.trim()
    if (!raw) return true
    try {
        parseDecimal(raw, { maxScale: 4 })
        return true
    } catch {
        return false
    }
}

export function lineReferenceSubtotal(line: InquiryLine): string | null {
    const rawPrice = line.sales_reference_price_gross?.trim()
    if (!rawPrice) return null
    try {
        if (compareDecimal(line.quantity, "0", line.quantity_scale) <= 0) {
            return null
        }
        return multiplyFixed(line.quantity, rawPrice, {
            leftMaxScale: line.quantity_scale,
            rightMaxScale: 4,
            outputScale: 2,
        })
    } catch {
        return null
    }
}

export function summarizeDemand(values: DemandFormValues): DemandSummary {
    const lines = values.draft.lines
    let physicalCount = 0
    let readyDeliveryCount = 0
    let pendingDeliveryCount = 0
    let overAllocatedCount = 0
    let missingDueDateCount = 0
    let invalidQuantityCount = 0
    const subtotals: string[] = []

    for (const line of lines) {
        if (line.fulfillment_kind === "physical") {
            physicalCount += 1
        }
        if (!line.delivery_due_at) {
            missingDueDateCount += 1
        }
        if (!isLineQuantityValid(line)) {
            invalidQuantityCount += 1
        }
        const progress = deliveryProgress(line)
        if (!progress.complete) {
            pendingDeliveryCount += 1
        } else if (progress.overAllocated) {
            overAllocatedCount += 1
        } else {
            readyDeliveryCount += 1
        }
        const subtotal = lineReferenceSubtotal(line)
        if (subtotal !== null) {
            subtotals.push(subtotal)
        }
    }

    let referenceTotal: string | null = null
    if (subtotals.length > 0) {
        try {
            referenceTotal = sumFixed(subtotals, {
                maxScale: 2,
                outputScale: 2,
            })
        } catch {
            referenceTotal = null
        }
    }

    return {
        lineCount: lines.length,
        physicalCount,
        readyDeliveryCount,
        pendingDeliveryCount,
        overAllocatedCount,
        missingDueDateCount,
        invalidQuantityCount,
        pricedLineCount: subtotals.length,
        referenceTotal,
    }
}
