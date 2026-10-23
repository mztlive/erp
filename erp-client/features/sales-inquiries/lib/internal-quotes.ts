import {
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
    compactFixed,
    sumFixed,
    parseDecimal,
} from "@/lib/fixed-decimal"
import type { InquiryLine } from "../types"
import { inquiryDateInput } from "./format"

export type QuoteTerms = {
    unit_cost_gross: string
    input_tax_rate: string
    available_quantity: string
    expected_delivery_date: string
    valid_to: string | null
    freight_gross?: string | null
    unavailable?: boolean
}
export type InternalQuote = {
    id: string
    quote_id: string
    revision_no: number
    requirement_version: number
    feedback_round: number
    created_at: number
    line_id: string
    supplier_id: string
    supplier_offering_id: string
    terms: QuoteTerms
    internal_note: string
}
export type QuoteInput = Omit<
    InternalQuote,
    "id" | "terms" | "requirement_version" | "feedback_round" | "created_at"
> & {
    terms: Omit<QuoteTerms, "valid_to" | "freight_gross"> & {
        valid_to: string
        freight_gross: string
    }
}

/** 每个供应商在本需求行只展示最后一次回复为当前报价；历史版本仍保留。 */
export function latestSupplierQuotes(
    quotes: readonly InternalQuote[],
): InternalQuote[] {
    const current = new Map<string, InternalQuote>()
    for (const quote of quotes) {
        const key = `${quote.line_id}:${quote.supplier_id}`
        const prior = current.get(key)
        if (
            !prior ||
            quote.feedback_round > prior.feedback_round ||
            (quote.feedback_round === prior.feedback_round &&
                (quote.created_at > prior.created_at ||
                    (quote.quote_id === prior.quote_id &&
                        quote.revision_no > prior.revision_no)))
        )
            current.set(key, quote)
    }
    return [...current.values()]
}

/** 按需求数量摊入本行总运费，最终单价统一按 4 位小数进行银行家舍入。 */
export function quoteReferencePrice(
    quote: InternalQuote,
    quantity: string,
): string {
    const { unit_cost_gross, freight_gross } = quote.terms
    if (freight_gross == null) throw new Error("运费尚未确认")
    parseDecimal(freight_gross, { maxScale: 2 })
    if (compareDecimal(quantity, "0", 6) <= 0)
        throw new Error("需求数量须大于零")
    const goodsTotal = multiplyFixed(unit_cost_gross, quantity, {
        leftMaxScale: 4,
        rightMaxScale: 6,
        outputScale: 10,
    })
    const total = sumFixed([goodsTotal, freight_gross], {
        maxScale: 10,
        outputScale: 10,
    })
    return compactFixed(
        divideFixed(total, quantity, {
            numeratorMaxScale: 10,
            denominatorMaxScale: 6,
            outputScale: 4,
        }),
    )
}

export function quoteExclusion(
    quote: InternalQuote,
    line: InquiryLine,
    round: number,
    tax?: string | null,
): string | null {
    const terms = quote.terms
    const today = new Date(Date.now() + 8 * 60 * 60 * 1000)
        .toISOString()
        .slice(0, 10)
    if (quote.feedback_round !== round) return "上一轮报价，需重新确认"
    if (terms.unavailable) return "无法供货"
    if (terms.valid_to && terms.valid_to <= today) return "报价已过期"
    try {
        if (compareDecimal(line.quantity, "0", 6) <= 0)
            return "需求数量须大于零"
        if (compareDecimal(terms.available_quantity, line.quantity, 6) < 0)
            return "可供数量不足"
        if (
            terms.expected_delivery_date >
            inquiryDateInput(line.delivery_due_at)
        )
            return "交期晚于要求"
        if (terms.freight_gross == null) return "运费尚未确认"
        quoteReferencePrice(quote, line.quantity)
        if (tax && compareDecimal(terms.input_tax_rate, tax, 6) !== 0)
            return "税率不同"
    } catch {
        return "报价金额、税率或数量无效，请核对"
    }
    return null
}

export function quotePriceHint(
    quotes: InternalQuote[],
    price: string | null | undefined,
    quantity: string,
) {
    if (!quotes.length) return null
    const sorted = quotes
        .map((quote) => quoteReferencePrice(quote, quantity))
        .sort((a, b) => compareDecimal(a, b, 4))
    const min = sorted[0]
    const max = sorted.at(-1)!
    if (!price)
        return { min, max, message: "", compactMessage: "", below: false }
    try {
        const below = compareDecimal(price, min, 6) < 0
        const above = compareDecimal(price, max, 6) > 0
        const base = above ? max : min
        const delta = compactFixed(
            subtractFixed(below ? base : price, below ? price : base, {
                maxScale: 6,
                outputScale: 6,
            }),
        )
        const percent =
            compareDecimal(base, "0", 6) === 0
                ? ""
                : `（${divideFixed(multiplyFixed(delta, "100", { leftMaxScale: 6, rightMaxScale: 0, outputScale: 6 }), base, { numeratorMaxScale: 6, denominatorMaxScale: 6, outputScale: 2 })}%）`
        return {
            min,
            max,
            below,
            message: `${below ? "低于最低" : above ? "高于最高" : "高于最低"}报价 ¥${delta}${percent}`,
            compactMessage: `${below ? "低" : "高"} ¥${delta}${percent}`,
        }
    } catch {
        return { min, max, below: false, message: "", compactMessage: "" }
    }
}
