import { transferCsv } from "@/lib/data-transfer"
import type { ReceivableScopeQuery } from "../api/scoped"
import { toQuery } from "../api/scoped-view"
export function exportReceivableScopeCsv(query: ReceivableScopeQuery) {
    return transferCsv("receivables", { ...toQuery(query), view: query.view })
}
