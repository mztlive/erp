import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type {
    InquiryConclusion,
    InquiryFeedbackLine,
    InquiryFeedbackData,
    InquiryLine,
    InquiryPrice,
} from "../types"

export function newFeedbackPrice(
    line: InquiryLine,
    quantity = line.quantity,
    terms?: Partial<FeedbackTerms>,
): InquiryPrice {
    return {
        price_id: crypto.randomUUID(),
        quantity,
        unit_price_gross: null,
        tax_rate: terms?.tax_rate === undefined ? "0.13" : terms.tax_rate,
        delivery_due_at: line.delivery_due_at,
        valid_until: terms?.valid_until ?? 0,
        min_quantity: "1",
        max_quantity: null,
        allow_cumulative: false,
        included_cost_note: "",
        allocations: line.allocations.map((item) => ({
            allocation_id: item.allocation_id,
            quantity: line.allocations.length === 1 ? quantity : item.quantity,
        })),
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
    const quantity =
        conclusion === "available" ? demand.quantity : value.available_quantity
    const prices =
        value.prices.length || conclusion === "pending"
            ? value.prices
            : [newFeedbackPrice(demand, quantity, terms)]
    return changeFeedbackQuantity(
        { ...value, conclusion, prices },
        demand,
        quantity,
    )
}

export function changeFeedbackQuantity(
    value: InquiryFeedbackLine,
    demand: InquiryLine,
    quantity: string,
): InquiryFeedbackLine {
    return {
        ...value,
        available_quantity: quantity,
        prices:
            value.prices.length === 1
                ? value.prices.map((price) => ({
                      ...price,
                      quantity,
                      allocations:
                          value.conclusion === "available"
                              ? demand.allocations.map((item) => ({
                                    allocation_id: item.allocation_id,
                                    quantity: item.quantity,
                                }))
                              : demand.allocations.length === 1
                                ? [
                                      {
                                          allocation_id:
                                              demand.allocations[0]
                                                  .allocation_id,
                                          quantity,
                                      },
                                  ]
                                : price.allocations,
                  }))
                : value.prices,
    }
}

export type FeedbackIssue = { message: string; field: string }

export type FeedbackTerms = Pick<InquiryPrice, "tax_rate" | "valid_until">

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
        valid_until: prices.every(
            (price) => price.valid_until === first?.valid_until,
        )
            ? (first?.valid_until ?? 0)
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
                  newFeedbackPrice(
                      line,
                      value.conclusion === "pending"
                          ? line.quantity
                          : value.available_quantity,
                      {
                          tax_rate: common.tax_rate ?? undefined,
                          valid_until: common.valid_until ?? undefined,
                      },
                  ),
              ]
        return {
            ...value,
            prices: prices.map((price) => ({ ...price, ...patch })),
        }
    })
}

/** 单条报价的常用字段在主表编辑，多条报价及特殊条件在展开区编辑。 */
export function feedbackFieldTarget(value: InquiryFeedbackLine, field: string) {
    let target = field
    if (value.prices.length === 1) {
        const prefix = `price-${toAutomationIdSegment(value.prices[0].price_id)}-`
        for (const suffix of ["gross-price"])
            if (field === `${prefix}${suffix}`) target = suffix
    }
    const inline = ["conclusion", "available-quantity", "gross-price"]
    if (
        field === "available-quantity" &&
        value.conclusion !== "partially_available" &&
        value.prices.length <= 1
    )
        target = "conclusion"
    if (["unavailable", "partially_available"].includes(value.conclusion))
        inline.push("public-note")
    return { field: target, expand: !inline.includes(target) }
}
/** 发布前定位常见缺项；最终权限、跨商品及费用校验仍由发布接口执行。 */
export function feedbackIssues(
    value: InquiryFeedbackLine,
    demand: InquiryLine,
): FeedbackIssue[] {
    const issues: FeedbackIssue[] = []
    const sum = (values: string[]) =>
        sumFixed(values, {
            maxScale: demand.quantity_scale,
            outputScale: demand.quantity_scale,
        })
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
            if (
                compareDecimal(
                    sum(value.prices.map((price) => price.quantity)),
                    value.available_quantity,
                    demand.quantity_scale,
                ) !== 0
            )
                add("各条报价的数量合计须等于可供数量", "details")
        }
    } catch {
        add("请检查可供数量及报价数量的格式和精度", "available-quantity")
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
        if (
            !price.delivery_due_at ||
            price.delivery_due_at > demand.delivery_due_at
        )
            add("承诺交付日不能晚于要求交付日", `${field}-due`)
        if (!price.valid_until || price.valid_until <= Date.now() / 1000)
            add("请填写尚未到期的报价有效期", `${field}-valid-until`)
        try {
            if (
                compareDecimal(price.tax_rate ?? "", "0", 6) < 0 ||
                compareDecimal(price.tax_rate ?? "", "1", 6) > 0
            )
                add("税率须在 0 至 1 之间", `${field}-tax-rate`)
            if (compareDecimal(price.quantity, "0", demand.quantity_scale) <= 0)
                add("报价适用数量须大于零", `${field}-quantity`)
            if (
                compareDecimal(price.min_quantity, "0", 6) <= 0 ||
                compareDecimal(price.min_quantity, price.quantity, 6) > 0 ||
                (price.max_quantity &&
                    compareDecimal(price.max_quantity, price.quantity, 6) < 0)
            )
                add("请检查最低、最高适用数量", `${field}-minimum`)
            if (
                (demand.fulfillment_kind === "physical" ||
                    price.allocations.length > 0) &&
                compareDecimal(
                    sum(price.allocations.map((item) => item.quantity)),
                    price.quantity,
                    demand.quantity_scale,
                ) !== 0
            )
                add("请将报价数量分配到配送地址", `${field}-allocations`)
            if (
                price.allocations.some(
                    (item) =>
                        compareDecimal(
                            item.quantity,
                            "0",
                            demand.quantity_scale,
                        ) <= 0,
                )
            )
                add(
                    "配送数量须大于零，不配送的地址请取消勾选",
                    `${field}-allocations`,
                )
        } catch {
            add("请检查报价数量、税率和配送数量", `${field}-quantity`)
        }
    }
    for (const allocation of demand.allocations) {
        try {
            const total = sum(
                value.prices.flatMap((price) =>
                    price.allocations
                        .filter(
                            (item) =>
                                item.allocation_id === allocation.allocation_id,
                        )
                        .map((item) => item.quantity),
                ),
            )
            if (
                compareDecimal(
                    total,
                    allocation.quantity,
                    demand.quantity_scale,
                ) > 0
            )
                add("配送地址的报价数量超过需求，请核对明细", "details")
        } catch {
            /* 上面的价格明细已报告数值错误。 */
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
        lines: value.lines.map((line) => ({
            ...line,
            prices: line.prices.map((price) => ({
                ...price,
                unit_price_gross: price.unit_price_gross?.trim() || null,
                tax_rate: price.tax_rate?.trim() || null,
            })),
        })),
    }
}
