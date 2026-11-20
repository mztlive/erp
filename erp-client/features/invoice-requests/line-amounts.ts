import { compareDecimal, multiplyFixed, sumFixed } from "@/lib/fixed-decimal"
import type { InvoiceRequestLine } from "./api"

export const INVOICE_REQUEST_LINE_LIMIT = 100
export const INVOICE_QUANTITY_SCALE = 6
export const INVOICE_UNIT_PRICE_SCALE = 4
export const INVOICE_CONTENT_LIMIT = 1000

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
