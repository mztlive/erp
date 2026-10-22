import { transferCsv } from "@/lib/data-transfer"
import type { InvoiceRequestScopeQuery } from "./scoped"
import { toQuery } from "./scoped-view"
export function exportInvoiceRequestScopeCsv(query: InvoiceRequestScopeQuery) {
    return transferCsv("invoice_requests", { ...toQuery(query) })
}
