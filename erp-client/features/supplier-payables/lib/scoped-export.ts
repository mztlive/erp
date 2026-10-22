import { transferCsv } from "@/lib/data-transfer"
import type { SupplierScopeQuery } from "../api/scoped"
import { toQuery } from "../api/scoped-view"
export function exportSupplierScopeCsv(query: SupplierScopeQuery) {
    return transferCsv("supplier_payables", {
        ...toQuery(query),
        view: query.view,
    })
}
