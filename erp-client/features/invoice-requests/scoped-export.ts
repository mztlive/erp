import { runDataTransfer } from "@/lib/data-transfer"
import type { InvoiceRequestScopeQuery } from "./scoped"
import { toQuery } from "./scoped-view"

export function exportInvoiceRequestScopeExcel(
    query: InvoiceRequestScopeQuery,
) {
    return runDataTransfer("invoice_requests_excel", {
        ...toQuery(query),
        page: undefined,
        page_size: undefined,
        scope_version: undefined,
    })
}
