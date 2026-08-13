import type { CustomerQualityRow } from "../types"

export function buildReturnTo(
    pathname: string,
    params: URLSearchParams,
): string {
    const qs = params.toString()
    return qs ? `${pathname}?${qs}` : pathname
}

export function withReturnFocus(
    returnTo: string,
    customerId: string,
    focusMetric?: string,
) {
    const [path, query = ""] = returnTo.split("?", 2)
    const params = new URLSearchParams(query)
    params.set("focusCustomerId", customerId)
    if (focusMetric) params.set("focusMetric", focusMetric)
    return `${path}?${params.toString()}`
}

export function customerHref(
    customerId: string,
    customerName: string,
    returnTo: string,
) {
    const p = new URLSearchParams()
    p.set("from", "W15")
    p.set("customerName", customerName)
    p.set("returnTo", returnTo)
    return `/sales/customers/${customerId}?${p.toString()}`
}

export function salesOrdersHref(
    row: CustomerQualityRow,
    period: { from: string; to: string },
    returnTo: string,
    businessType?: "VOUCHER" | "GOODS_SERVICE",
) {
    const p = new URLSearchParams()
    p.set("search", row.customerName)
    p.set("customerId", row.customerId)
    p.set("customerName", row.customerName)
    p.set("from", "W15")
    p.set("periodFrom", period.from)
    p.set("periodTo", period.to)
    p.set("returnTo", returnTo)
    if (businessType) {
        p.set(
            "nature",
            businessType === "VOUCHER" ? "card_voucher" : "physical_service",
        )
    }
    return `/sales/orders?${p.toString()}`
}

export function receivablesHref(
    row: CustomerQualityRow,
    period: { from: string; to: string },
    returnTo: string,
) {
    const p = new URLSearchParams()
    p.set("view", "receivable")
    p.set("customerId", row.customerId)
    p.set("customerName", row.customerName)
    p.set("due", "overdue")
    p.set("from", "W15")
    p.set("periodFrom", period.from)
    p.set("periodTo", period.to)
    p.set("returnTo", returnTo)
    return `/finance/customer-accounts?${p.toString()}`
}

export function profitLossHref(
    row: CustomerQualityRow,
    period: { from: string; to: string },
    returnTo: string,
) {
    const p = new URLSearchParams()
    p.set("customerId", row.customerId)
    p.set("customerName", row.customerName)
    p.set("from", period.from)
    p.set("to", period.to)
    p.set("source", "W15")
    p.set("returnTo", returnTo)
    return `/analytics/profit-loss?${p.toString()}`
}
