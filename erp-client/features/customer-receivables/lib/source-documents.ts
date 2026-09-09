import type { CustomerAccountPreviewTarget } from "../components/column-types"

export function customerDocumentHref(
    kind: CustomerAccountPreviewTarget["kind"],
    id: string,
): string {
    const view =
        kind === "receivable"
            ? "receivable"
            : kind === "invoice"
              ? "sales_invoice"
              : "receipt"
    return `/finance/customer-accounts?${new URLSearchParams({ view, previewKind: kind, previewId: id })}`
}

export function salesOrderHref(id: string): string {
    return `/sales/orders/${encodeURIComponent(id)}`
}
