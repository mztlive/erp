import type {
    CustomerAccountsView,
    DueFilter,
} from "@/features/customer-receivables/types"

export function parseView(raw: string | null): CustomerAccountsView {
    if (
        raw === "receipt" ||
        raw === "sales_invoice" ||
        raw === "unallocated" ||
        raw === "receivable"
    ) {
        return raw
    }
    return "receivable"
}

export function parseDue(raw: string | null): DueFilter | undefined {
    if (
        raw === "not_due" ||
        raw === "due_today" ||
        raw === "overdue" ||
        raw === "all"
    ) {
        return raw
    }
    return undefined
}
