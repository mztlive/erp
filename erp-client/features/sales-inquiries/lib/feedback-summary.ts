import { compareDecimal, multiplyFixed, sumFixed } from "@/lib/fixed-decimal"
import { feedbackIssues } from "./feedback-editing"
import type { InquiryFeedbackLine, InquiryFee, InquiryLine } from "../types"

export type FeedbackSummary = {
    totalCount: number
    completeCount: number
    incompleteCount: number
    availableCount: number
    partialCount: number
    unavailableCount: number
    pendingCount: number
    pricedCount: number
    goodsTotal: string | null
    feeCount: number
    feesTotal: string | null
    grandTotal: string | null
}

/**
 * 计算单行含税供给小计：
 * - 不供应返回 null；
 * - 部分可供应按可供数量计算，其余状态按需求数量预估；
 * - 全程使用定点数，无有效单价或数量时返回 null。
 */
export function feedbackLineSubtotal(
    line: InquiryFeedbackLine,
    demand: InquiryLine,
): string | null {
    if (line.conclusion === "unavailable") return null
    const unitPrice = line.prices[0]?.unit_price_gross?.trim() ?? ""
    if (!unitPrice) return null
    const quantity =
        line.conclusion === "partially_available"
            ? line.available_quantity.trim()
            : demand.quantity.trim()
    if (!quantity) return null
    try {
        if (compareDecimal(unitPrice, "0", 4) < 0) return null
        if (compareDecimal(quantity, "0", demand.quantity_scale) <= 0)
            return null
        return multiplyFixed(quantity, unitPrice, {
            leftMaxScale: demand.quantity_scale,
            rightMaxScale: 4,
            outputScale: 2,
        })
    } catch {
        return null
    }
}

export function sumInquiryFeesGross(
    fees: readonly InquiryFee[],
): string | null {
    const valid: string[] = []
    for (const fee of fees) {
        const value = fee.amount_gross.trim()
        if (!value) continue
        try {
            if (compareDecimal(value, "0", 2) >= 0) valid.push(value)
        } catch {
            // 忽略录入中的未完成费用金额
        }
    }
    if (valid.length === 0) return null
    try {
        return sumFixed(valid, { maxScale: 2, outputScale: 2 })
    } catch {
        return null
    }
}

export function summarizeFeedback(
    lines: readonly InquiryFeedbackLine[],
    demand: readonly InquiryLine[],
    fees: readonly InquiryFee[],
): FeedbackSummary {
    const demandById = new Map(demand.map((item) => [item.line_id, item]))
    let completeCount = 0
    let availableCount = 0
    let partialCount = 0
    let unavailableCount = 0
    let pendingCount = 0
    const subtotals: string[] = []

    for (const line of lines) {
        if (line.conclusion === "available") availableCount++
        else if (line.conclusion === "partially_available") partialCount++
        else if (line.conclusion === "unavailable") unavailableCount++
        else pendingCount++

        const item = demandById.get(line.line_id)
        if (!item) continue
        if (feedbackIssues(line, item).length === 0) completeCount++
        const subtotal = feedbackLineSubtotal(line, item)
        if (subtotal !== null) subtotals.push(subtotal)
    }

    let goodsTotal: string | null = null
    if (subtotals.length > 0) {
        try {
            goodsTotal = sumFixed(subtotals, { maxScale: 2, outputScale: 2 })
        } catch {
            goodsTotal = null
        }
    }

    const feesTotal = sumInquiryFeesGross(fees)
    let grandTotal: string | null = null
    if (goodsTotal !== null || feesTotal !== null) {
        try {
            grandTotal = sumFixed(
                [goodsTotal, feesTotal].filter(
                    (value): value is string => value !== null,
                ),
                { maxScale: 2, outputScale: 2 },
            )
        } catch {
            grandTotal = goodsTotal ?? feesTotal
        }
    }

    return {
        totalCount: lines.length,
        completeCount,
        incompleteCount: lines.length - completeCount,
        availableCount,
        partialCount,
        unavailableCount,
        pendingCount,
        pricedCount: subtotals.length,
        goodsTotal,
        feeCount: fees.length,
        feesTotal,
        grandTotal,
    }
}
