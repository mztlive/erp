import {
    compareDecimal,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type { InvoiceRequestLine } from "./api"

export const INVOICE_REQUEST_LINE_LIMIT = 100
export const INVOICE_QUANTITY_SCALE = 6
export const INVOICE_UNIT_PRICE_SCALE = 4
export const INVOICE_CONTENT_LIMIT = 1000

const AMOUNT_PATTERN = /^-?\d+(?:\.\d{1,2})?$/

export function canonicalInvoiceDecimal(value: string): string {
    return value.trim().replace(/^0+(?=\d)/, "")
}

export function isPositiveInvoiceDecimal(
    value: string,
    scale: number,
): boolean {
    const decimal = canonicalInvoiceDecimal(value)
    return (
        new RegExp(`^\\d+(?:\\.\\d{1,${scale}})?$`).test(decimal) &&
        /[1-9]/.test(decimal)
    )
}

export function isInvoiceAmountText(
    value: string | null | undefined,
): value is string {
    return typeof value === "string" && AMOUNT_PATTERN.test(value.trim())
}

export function invoiceLineAmount({
    quantity,
    unit_price,
}: Pick<InvoiceRequestLine, "quantity" | "unit_price">): string | null {
    if (
        !isPositiveInvoiceDecimal(quantity, INVOICE_QUANTITY_SCALE) ||
        !isPositiveInvoiceDecimal(unit_price, INVOICE_UNIT_PRICE_SCALE)
    ) {
        return null
    }
    return multiplyFixed(
        canonicalInvoiceDecimal(quantity),
        canonicalInvoiceDecimal(unit_price),
        {
            leftMaxScale: INVOICE_QUANTITY_SCALE,
            rightMaxScale: INVOICE_UNIT_PRICE_SCALE,
            outputScale: 2,
        },
    )
}

export function invoiceLinesTotal(
    lines: readonly InvoiceRequestLine[],
): string | null {
    if (!lines.length) return null
    const amounts = lines.map(invoiceLineAmount)
    if (!amounts.every((amount) => amount !== null)) return null
    return sumFixed(amounts, { maxScale: 2, outputScale: 2 })
}

export function invoiceTotalMismatch(
    total: string | null,
    amount: string,
): boolean {
    return (
        total !== null &&
        isPositiveInvoiceDecimal(amount, 2) &&
        compareDecimal(total, canonicalInvoiceDecimal(amount), 2) !== 0
    )
}

/** 判断本次申请金额是否超出当前可申请额度。 */
export function invoiceExceedsAvailable(
    total: string | null,
    availableAmount?: string | null,
): boolean {
    if (!total || !isInvoiceAmountText(availableAmount)) return false
    return (
        compareDecimal(
            canonicalInvoiceDecimal(total),
            canonicalInvoiceDecimal(availableAmount),
            2,
        ) > 0
    )
}

/** 计算扣除本次申请金额后的剩余可申请额度（超额时返回负数）。 */
export function invoiceRemainingAfterRequest(
    total: string | null,
    availableAmount?: string | null,
): string | null {
    if (!isInvoiceAmountText(availableAmount)) return null
    return subtractFixed(
        canonicalInvoiceDecimal(availableAmount),
        total ? canonicalInvoiceDecimal(total) : "0.00",
        { maxScale: 2, outputScale: 2 },
    )
}

/** 超额时返回超出部分的金额，未超额返回 null。 */
export function invoiceOverAmount(
    total: string | null,
    availableAmount?: string | null,
): string | null {
    if (!total || !invoiceExceedsAvailable(total, availableAmount)) return null
    return subtractFixed(
        canonicalInvoiceDecimal(total),
        canonicalInvoiceDecimal(availableAmount!),
        { maxScale: 2, outputScale: 2 },
    )
}
