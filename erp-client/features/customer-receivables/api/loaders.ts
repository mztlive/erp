/** HTTP list loaders for the customer-accounts feature. */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"
import type { FundsSummaryWire } from "@/lib/funds-scope"

import type { CustomerAccountsQuery } from "@/features/customer-receivables/types"
import { mapBackendStatusFilter } from "./mappers"
import type {
    BackendCustomerReceipt,
    BackendInvoice,
    BackendReceivableAccount,
} from "./dto"

export type CustomerAccountsPage<T> = Page<T> & {
    scope_version?: string
    summary?: FundsSummaryWire
    empty_reason?: string | null
}

export async function loadReceivables(
    query: CustomerAccountsQuery,
    scopeVersion?: string,
): Promise<CustomerAccountsPage<BackendReceivableAccount>> {
    return apiGet<CustomerAccountsPage<BackendReceivableAccount>>(
        "/admin/receivable-accounts",
        {
            scope_version: scopeVersion,
            page: query.page,
            page_size: query.pageSize,
            q: query.q?.trim() || undefined,
            account_id: query.receivableAccountId,
            customer_id: query.customerId,
            counterparty_party_id: query.counterpartyPartyId,
            status: mapBackendStatusFilter(
                query.view === "receivable" ? query.status : undefined,
            ),
            sales_order_id: query.salesOrderId,
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
}

export async function loadReceipts(
    query: CustomerAccountsQuery,
    scopeVersion?: string,
): Promise<CustomerAccountsPage<BackendCustomerReceipt>> {
    return apiGet<CustomerAccountsPage<BackendCustomerReceipt>>(
        "/admin/customer-receipts",
        {
            scope_version: scopeVersion,
            page: query.page,
            page_size: query.pageSize,
            counterparty_party_id: query.counterpartyPartyId,
            q: query.q?.trim() || undefined,
            sales_order_id: query.salesOrderId,
            receivable_account_id: query.receivableAccountId,
            status: mapBackendStatusFilter(
                query.view === "receipt" ? query.status : undefined,
            ),
            sort_by: "received_at",
            sort_dir: "desc",
        },
    )
}

export async function loadSalesInvoices(
    query: CustomerAccountsQuery,
    scopeVersion?: string,
): Promise<CustomerAccountsPage<BackendInvoice>> {
    return apiGet<CustomerAccountsPage<BackendInvoice>>("/admin/invoices", {
        scope_version: scopeVersion,
        page: query.page,
        page_size: query.pageSize,
        invoice_direction: "sales",
        party_id: query.counterpartyPartyId,
        q: query.q?.trim() || undefined,
        sales_order_id: query.salesOrderId,
        receivable_account_id: query.receivableAccountId,
        sort_by: "invoice_date",
        sort_dir: "desc",
    })
}

/** 后续页先取得本次全范围版本，再由服务端校验，避免混合不同范围的金额。 */
export async function loadAccountPage<T>(
    query: CustomerAccountsQuery,
    active: boolean,
    loader: (
        query: CustomerAccountsQuery,
        scopeVersion?: string,
    ) => Promise<CustomerAccountsPage<T>>,
): Promise<CustomerAccountsPage<T>> {
    const first = await loader({
        ...query,
        page: 1,
        pageSize: active ? query.pageSize : 1,
    })
    if (!active || query.page === 1) return first
    if (!first.scope_version)
        throw new Error("查询范围已失效，请回到第一页重新查询")
    return loader(query, first.scope_version)
}
