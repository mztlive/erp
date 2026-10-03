import type {
    SalesChangeDraft,
    SalesChangeDraftLine,
} from "../api/sales-change-draft"
import {
    compareDecimal,
    multiplyFixed,
    normalizeFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"

export type SalesChangeDraftValues = {
    reason: string
    remark: string
    lines: {
        quantity: string
        unitPrice: string
        taxRate: string
        faceValue: string
        cardCount: string
    }[]
}

export function salesChangeDraftValues(
    draft: SalesChangeDraft,
): SalesChangeDraftValues {
    return {
        reason: draft.reason,
        remark: draft.business_remark ?? "",
        lines: draft.lines.map((line) => ({
            quantity: line.goods?.quantity ?? "1",
            unitPrice:
                line.goods?.unit_price_gross ??
                line.voucher?.unit_price_gross ??
                "0",
            taxRate: line.sales_tax_rate,
            faceValue: line.voucher?.face_value ?? "1",
            cardCount: String(line.voucher?.card_count ?? 1),
        })),
    }
}

export function positiveSalesDecimal(value: string, scale: number) {
    try {
        return compareDecimal(value, "0", scale) > 0
    } catch {
        return false
    }
}

/** 实物及服务变更允许数量归零，仍保留原稳定明细身份。 */
export function nonnegativeSalesQuantity(value: string) {
    try {
        return compareDecimal(value, "0", 6) >= 0
    } catch {
        return false
    }
}

export function validSalesTaxRate(value: string) {
    try {
        return (
            compareDecimal(value, "0", 6) >= 0 &&
            compareDecimal(value, "1", 6) <= 0
        )
    } catch {
        return false
    }
}

/** 保留完整冻结字段，只替换允许编辑的数量、价格与税率。 */
export function salesChangeTargetLines(
    draft: SalesChangeDraft,
    values: SalesChangeDraftValues,
): SalesChangeDraftLine[] {
    if (draft.lines.length !== values.lines.length)
        throw new Error("明细已变化，请重新读取原单")
    return draft.lines.map((line, index) => {
        const value = values.lines[index]
        if (line.goods)
            return {
                ...line,
                sales_tax_rate: value.taxRate,
                goods: {
                    ...line.goods,
                    quantity: value.quantity,
                    unit_price_gross: value.unitPrice,
                    pricing_mode:
                        compareDecimal(
                            value.unitPrice,
                            line.goods.unit_price_gross,
                            4,
                        ) === 0
                            ? line.goods.pricing_mode
                            : "MANUAL",
                },
            }
        if (!line.voucher) throw new Error("原明细缺少业务字段")
        const totalsOptions = {
            leftMaxScale: 4,
            rightMaxScale: 0,
            outputScale: 2,
        }
        const faceTotal = multiplyFixed(
            value.faceValue,
            value.cardCount,
            totalsOptions,
        )
        const transaction = multiplyFixed(
            value.unitPrice,
            value.cardCount,
            totalsOptions,
        )
        const gift = subtractFixed(faceTotal, transaction, {
            maxScale: 2,
            outputScale: 2,
        })
        return {
            ...line,
            sales_tax_rate: value.taxRate,
            voucher: {
                ...line.voucher,
                face_value: value.faceValue,
                card_count: value.cardCount,
                unit_price_gross: value.unitPrice,
                face_value_total: faceTotal,
                transaction_amount: transaction,
                gift_amount: gift,
                gift_rate: null,
            },
        }
    })
}

/** 保存结果未知时核对完整规范化目标，包含全部冻结身份与非编辑字段。 */
export function salesChangeSavedMatches(
    current: SalesChangeDraft,
    target: SalesChangeDraftValues,
    original: SalesChangeDraft,
): boolean {
    if (
        current.version <= original.version ||
        current.lines.length !== original.lines.length
    )
        return false
    const amount = (value: string) =>
        normalizeFixed(value, { maxScale: 2, outputScale: 2 })
    const price = (value: string) =>
        normalizeFixed(value, { maxScale: 4, outputScale: 4 })
    const canonicalLine = (line: SalesChangeDraftLine) => ({
        ...line,
        item_name_snapshot: line.item_name_snapshot.trim(),
        spec_snapshot: line.spec_snapshot?.trim() || null,
        unit_snapshot: line.unit_snapshot?.trim() || null,
        sales_tax_rate: normalizeFixed(line.sales_tax_rate, {
            maxScale: 6,
            outputScale: 6,
        }),
        goods: line.goods
            ? {
                  ...line.goods,
                  quantity: normalizeFixed(line.goods.quantity, {
                      maxScale: 6,
                      outputScale: 6,
                  }),
                  unit_price_gross: price(line.goods.unit_price_gross),
                  pricing_mode: line.goods.pricing_mode ?? "MANUAL",
              }
            : null,
        voucher: line.voucher
            ? {
                  ...line.voucher,
                  face_value: amount(line.voucher.face_value),
                  card_count: BigInt(line.voucher.card_count).toString(),
                  unit_price_gross: price(line.voucher.unit_price_gross),
                  face_value_total: amount(line.voucher.face_value_total),
                  transaction_amount: amount(line.voucher.transaction_amount),
                  gift_amount: amount(line.voucher.gift_amount),
                  gift_rate: null,
              }
            : null,
    })
    const canonical = (
        reason: string,
        remark: string | null,
        lines: SalesChangeDraftLine[],
    ) =>
        JSON.stringify(
            {
                reason: reason.trim(),
                remark: remark?.trim() || null,
                lines: lines.map(canonicalLine),
            },
            (_key, value: unknown) =>
                value && typeof value === "object" && !Array.isArray(value)
                    ? Object.fromEntries(
                          Object.entries(value).sort(([left], [right]) =>
                              left.localeCompare(right),
                          ),
                      )
                    : value,
        )
    return (
        canonical(current.reason, current.business_remark, current.lines) ===
        canonical(
            target.reason,
            target.remark,
            salesChangeTargetLines(original, target),
        )
    )
}
