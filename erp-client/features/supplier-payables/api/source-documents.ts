import { apiGet, type Page } from "@/lib/api"
import { projectPayable, type BackendPayableAccount } from "./mappers"
import type { PayableSourceType } from "../types"
import {
    sourceDocumentHref,
    sourceDocumentOpenLabel,
} from "../lib/related-documents"

export type SupplierSourceScope = {
    allocations?: readonly {
        payableAccountId: string
        sourceHref?: string
        sourceDocumentNo: string
        sourceType: PayableSourceType
    }[]
    entryId?: string
    supplierId?: string
}
export type SupplierSourceDocument = {
    href: string
    label: string
    action: string
}

/** 通过应付主键或原分录查原采购/结算单；按来源地址去重，不将分录 ID 伪装成单据地址。 */
export async function fetchSupplierSourceDocuments(
    scope: SupplierSourceScope,
): Promise<{ documents: SupplierSourceDocument[]; unresolved: number }> {
    const documents = new Map<string, SupplierSourceDocument>()
    let unresolved = 0
    const add = (
        href: string | undefined,
        label: string,
        type: PayableSourceType,
    ) => {
        if (href)
            documents.set(href, {
                href,
                label,
                action: sourceDocumentOpenLabel(type),
            })
        else unresolved += 1
    }
    const missing = new Set<string>()
    for (const allocation of scope.allocations ?? []) {
        if (allocation.sourceHref)
            add(
                allocation.sourceHref,
                allocation.sourceDocumentNo,
                allocation.sourceType,
            )
        else if (allocation.payableAccountId)
            missing.add(allocation.payableAccountId)
        else unresolved += 1
    }
    const results = await Promise.allSettled(
        [...missing].map((id) =>
            apiGet<BackendPayableAccount>(
                `/admin/payable-accounts/${encodeURIComponent(id)}`,
            ),
        ),
    )
    for (const result of results) {
        if (result.status === "rejected") {
            unresolved += 1
            continue
        }
        const payable = projectPayable(result.value)
        add(
            payable.sourceHref ??
                sourceDocumentHref(
                    payable.sourceType,
                    payable.sourceDocumentId,
                ),
            payable.sourceDocumentNo,
            payable.sourceType,
        )
    }
    if (scope.entryId && scope.supplierId) {
        let loaded = 0
        let found = false
        for (let page = 1; !found; page += 1) {
            const result = await apiGet<Page<BackendPayableAccount>>(
                "/admin/payable-accounts",
                {
                    supplier_id: scope.supplierId,
                    page,
                    page_size: 100,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
            const rows = result.items ?? []
            const account = rows.find((row) =>
                row.entries?.some((entry) => entry.id === scope.entryId),
            )
            if (account) {
                const payable = projectPayable(account)
                add(
                    payable.sourceHref,
                    payable.sourceDocumentNo,
                    payable.sourceType,
                )
                found = true
            }
            loaded += rows.length
            if (!rows.length || loaded >= result.total) break
        }
        if (!found) unresolved += 1
    } else if (scope.entryId) unresolved += 1
    return { documents: [...documents.values()], unresolved }
}
