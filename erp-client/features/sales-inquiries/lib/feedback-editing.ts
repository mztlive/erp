import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import type {
    InquiryConclusion,
    InquiryFeedbackLine,
    InquiryFeedbackData,
    InquiryLine,
    InquiryPrice,
} from "../types"

export function newFeedbackPrice(terms?: Partial<FeedbackTerms>): InquiryPrice {
    return {
        price_id: crypto.randomUUID(),
        unit_price_gross: null,
        tax_rate: terms?.tax_rate === undefined ? "0.13" : terms.tax_rate,
    }
}

export function changeFeedbackConclusion(
    value: InquiryFeedbackLine,
    demand: InquiryLine,
    conclusion: InquiryConclusion,
    terms?: Partial<FeedbackTerms>,
): InquiryFeedbackLine {
    if (conclusion === "unavailable")
        return { ...value, conclusion, available_quantity: "0", prices: [] }
    return {
        ...value,
        conclusion,
        available_quantity:
            conclusion === "available"
                ? demand.quantity
                : value.available_quantity,
        prices: value.prices.length ? value.prices : [newFeedbackPrice(terms)],
    }
}

export type FeedbackIssue = { message: string; field: string }

export type FeedbackTerms = Pick<InquiryPrice, "tax_rate">

/** 从已保存的逐商品条件汇总统一项；不同值保持混合状态，不覆盖商品例外。 */
export function feedbackCommonTerms(lines: InquiryFeedbackLine[]) {
    const prices = lines
        .filter((line) => line.conclusion !== "unavailable")
        .flatMap((line) => line.prices)
    const first = prices[0]
    return {
        tax_rate: prices.every((price) => price.tax_rate === first?.tax_rate)
            ? first
                ? (first.tax_rate ?? "")
                : "0.13"
            : null,
    }
}

/** 统一设置只改指定条件，不改变供给结论、单价、数量及其他商品条件。 */
export function applyFeedbackTerms(
    lines: InquiryFeedbackLine[],
    demand: InquiryLine[],
    patch: Partial<FeedbackTerms>,
) {
    const common = feedbackCommonTerms(lines)
    return lines.map((value) => {
        const line = demand.find((item) => item.line_id === value.line_id)
        if (!line || value.conclusion === "unavailable") return value
        const prices = value.prices.length
            ? value.prices
            : [
                  newFeedbackPrice({
                      tax_rate: common.tax_rate ?? undefined,
                  }),
              ]
        return {
            ...value,
            prices: prices.map((price) => ({ ...price, ...patch })),
        }
    })
}

/** 所有报价字段直接定位到商品行。 */
export function feedbackFieldTarget(value: InquiryFeedbackLine, field: string) {
    if (
        field === "available-quantity" &&
        value.conclusion !== "partially_available"
    )
        return "conclusion"
    const price = value.prices.find((item) =>
        field.startsWith(`price-${toAutomationIdSegment(item.price_id)}-`),
    )
    return price
        ? field.replace(`price-${toAutomationIdSegment(price.price_id)}-`, "")
        : field
}
/** 发布前定位常见缺项；最终权限、跨商品及费用校验仍由发布接口执行。 */
export function feedbackIssues(
    value: InquiryFeedbackLine,
    demand: InquiryLine,
): FeedbackIssue[] {
    const issues: FeedbackIssue[] = []
    const add = (message: string, field: string) =>
        issues.push({ message, field })
    if (value.conclusion === "pending") add("请选择最终供给结论", "conclusion")
    if (
        ["unavailable", "partially_available"].includes(value.conclusion) &&
        !value.public_note.trim()
    )
        add("请填写缺货或不可供原因", "public-note")
    try {
        const qty = compareDecimal(
            value.available_quantity,
            "0",
            demand.quantity_scale,
        )
        const wanted = compareDecimal(
            value.available_quantity,
            demand.quantity,
            demand.quantity_scale,
        )
        if (value.conclusion === "available" && wanted !== 0)
            add(
                "可供数量须等于需求；不足时请选择部分可供",
                "available-quantity",
            )
        if (
            value.conclusion === "partially_available" &&
            (qty <= 0 || wanted >= 0)
        )
            add("部分可供数量须大于零且少于需求", "available-quantity")
        if (
            value.conclusion === "unavailable" &&
            (qty !== 0 || value.prices.length > 0)
        )
            add("不可供商品不能保留数量或报价", "conclusion")
        if (
            value.conclusion !== "unavailable" &&
            value.conclusion !== "pending"
        ) {
            if (!value.prices.length) add("请填写供给单价", "gross-price")
            if (value.prices.length > 1)
                add("每件商品仅支持一条报价，请重新填写", "gross-price")
        }
    } catch {
        add("请检查可供数量的格式和精度", "available-quantity")
    }
    if (value.conclusion === "unavailable") return issues
    for (const price of value.prices) {
        const field = `price-${toAutomationIdSegment(price.price_id)}`
        try {
            if (compareDecimal(price.unit_price_gross ?? "", "0", 4) < 0)
                throw new Error()
        } catch {
            add("请填写有效的含税供给单价", `${field}-gross-price`)
        }
        try {
            if (
                compareDecimal(price.tax_rate ?? "", "0", 6) < 0 ||
                compareDecimal(price.tax_rate ?? "", "1", 6) > 0
            )
                add("税率须在 0% 至 100% 之间", `${field}-tax-rate`)
        } catch {
            add("请填写有效税率", `${field}-tax-rate`)
        }
    }
    return issues
}

/** 旧本地草稿的空字符串按未填写提交，非空金额仍由服务端定点数类型校验。 */
export function feedbackRequestData(
    value: InquiryFeedbackData,
): InquiryFeedbackData {
    return {
        ...value,
        fees: value.fees.map((fee) => ({
            fee_id: fee.fee_id,
            line_ids: fee.line_ids,
            public_note: fee.public_note,
            name: fee.name.trim(),
            amount_gross: fee.amount_gross.trim(),
            tax_rate: fee.tax_rate.trim(),
        })),
        lines: value.lines.map((line) => ({
            ...line,
            prices: line.prices.map((price) => ({
                price_id: price.price_id,
                unit_price_gross: price.unit_price_gross?.trim() || null,
                tax_rate: price.tax_rate?.trim() || null,
            })),
        })),
    }
}
