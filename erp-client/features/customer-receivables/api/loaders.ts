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

export async function loadReceivables(
    query: CustomerAccountsQuery,
): Promise<Page<BackendReceivableAccount>> {
    return apiGet<Page<BackendReceivableAccount>>(
        "/admin/receivable-accounts",
        {
            customer_id: query.customerId,
            counterparty_party_id: query.counterpartyPartyId,
            status: mapBackendStatusFilter(query.status),
            sales_order_id: query.salesOrderId,
            review_status: mapBackendStatusFilter(query.reviewStatus),
            page: 1,
            page_size: LIST_PAGE_SIZE,
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
}

export async function loadReceipts(
    query: CustomerAccountsQuery,
): Promise<Page<BackendCustomerReceipt>> {
    return apiGet<Page<BackendCustomerReceipt>>("/admin/customer-receipts", {
        counterparty_party_id: query.counterpartyPartyId,
        receipt_no: query.q?.trim() || undefined,
        status: mapBackendStatusFilter(
            query.view === "receipt" ? query.status : undefined,
        ),
        page: 1,
        page_size: LIST_PAGE_SIZE,
        sort_by: "received_at",
        sort_dir: "desc",
    })
}

export async function loadSalesInvoices(
    query: CustomerAccountsQuery,
): Promise<Page<BackendInvoice>> {
    return apiGet<Page<BackendInvoice>>("/admin/invoices", {
        invoice_direction: "sales",
        party_id: query.counterpartyPartyId,
        invoice_no: query.q?.trim() || undefined,
        page: 1,
        page_size: LIST_PAGE_SIZE,
        sort_by: "invoice_date",
        sort_dir: "desc",
    })
}
