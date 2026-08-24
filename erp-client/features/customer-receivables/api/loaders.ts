/** HTTP list loaders for the customer-accounts feature. */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"

import type { CustomerAccountsQuery } from "@/features/customer-receivables/types"
import { mapBackendStatusFilter } from "./mappers"
import type {
    BackendCustomerReceipt,
    BackendInvoice,
    BackendReceivableAccount,
} from "./dto"

const LIST_PAGE_SIZE = 100
const MAX_LIST_PAGES = 50

/** 拉取完整分页，避免先截取前 100 条再按销售单做客户端过滤。 */
export async function loadAllPages<T>(
    path: string,
    query: Record<string, unknown> = {},
): Promise<Page<T>> {
    const items: T[] = []
    let page = 1
    let total = Number.POSITIVE_INFINITY

    while (items.length < total && page <= MAX_LIST_PAGES) {
        const result = await apiGet<Page<T>>(path, {
            ...query,
            page,
            page_size: LIST_PAGE_SIZE,
        })
        items.push(...(result.items ?? []))
        total = result.total ?? items.length
        if (!result.items?.length) break
        page += 1
    }

    return {
        items,
        total: Number.isFinite(total) ? total : items.length,
        page: 1,
        page_size: LIST_PAGE_SIZE,
    }
}

export async function loadReceivables(
    query: CustomerAccountsQuery,
): Promise<Page<BackendReceivableAccount>> {
    return loadAllPages<BackendReceivableAccount>(
        "/admin/receivable-accounts",
        {
            customer_id: query.customerId,
            counterparty_party_id: query.counterpartyPartyId,
            status: mapBackendStatusFilter(query.status),
            sales_order_id: query.salesOrderId,
            review_status: mapBackendStatusFilter(query.reviewStatus),
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
}

export async function loadReceipts(
    query: CustomerAccountsQuery,
): Promise<Page<BackendCustomerReceipt>> {
    return loadAllPages<BackendCustomerReceipt>("/admin/customer-receipts", {
        counterparty_party_id: query.counterpartyPartyId,
        receipt_no: query.q?.trim() || undefined,
        status: mapBackendStatusFilter(
            query.view === "receipt" ? query.status : undefined,
        ),
        sort_by: "received_at",
        sort_dir: "desc",
    })
}

export async function loadSalesInvoices(
    query: CustomerAccountsQuery,
): Promise<Page<BackendInvoice>> {
    return loadAllPages<BackendInvoice>("/admin/invoices", {
        invoice_direction: "sales",
        party_id: query.counterpartyPartyId,
        invoice_no: query.q?.trim() || undefined,
        sort_by: "invoice_date",
        sort_dir: "desc",
    })
}
