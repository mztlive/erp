import { apiGet, type Page } from "@/lib/api"
import type { BackendReceivableAccount } from "./dto"
import { salesOrderHref } from "../lib/source-documents"

export type CustomerSourceScope = {
    accountIds?: readonly string[]
    entryIds?: readonly string[]
    counterpartyPartyId?: string
    customerId?: string
}
export type CustomerSourceDocument = { id: string; label: string; href: string }

/** 按真实核销身份解析原销售单；分录只有 ID 时分页读取该往来主体的应收，找到全部目标即停止。 */
export async function fetchCustomerSourceDocuments(
    scope: CustomerSourceScope,
): Promise<{ documents: CustomerSourceDocument[]; unresolved: number }> {
    const accountIds = [...new Set(scope.accountIds?.filter(Boolean) ?? [])]
    const remaining = new Set(scope.entryIds?.filter(Boolean) ?? [])
    const accounts = await Promise.all(
        accountIds.map((id) =>
            apiGet<BackendReceivableAccount>(
                `/admin/receivable-accounts/${encodeURIComponent(id)}`,
            ),
        ),
    )
    if (remaining.size && (scope.counterpartyPartyId || scope.customerId)) {
        let loaded = 0
        for (let page = 1; remaining.size; page += 1) {
            const result = await apiGet<Page<BackendReceivableAccount>>(
                "/admin/receivable-accounts",
                {
                    counterparty_party_id: scope.counterpartyPartyId,
                    customer_id: scope.customerId,
                    page,
                    page_size: 100,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
            const rows = result.items ?? []
            for (const account of rows) {
                const matches = (account.entries ?? []).filter((entry) =>
                    remaining.has(entry.id),
                )
                if (!matches.length) continue
                accounts.push(account)
                for (const entry of matches) remaining.delete(entry.id)
            }
            loaded += rows.length
            if (!rows.length || loaded >= result.total) break
        }
    }
    const documents = new Map<string, CustomerSourceDocument>()
    let unresolved = remaining.size
    for (const account of accounts) {
        if (!account.sales_order_id?.trim()) {
            unresolved += 1
            continue
        }
        documents.set(account.sales_order_id, {
            id: account.sales_order_id,
            label: account.sales_order_no || "原销售单",
            href: salesOrderHref(account.sales_order_id),
        })
    }
    return { documents: [...documents.values()], unresolved }
}
