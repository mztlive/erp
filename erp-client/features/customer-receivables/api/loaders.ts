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

export async function loadReceivables(
    query: CustomerAccountsQuery,
): Promise<Page<BackendReceivableAccount>> {
    return apiGet<Page<BackendReceivableAccount>>(
        "/admin/receivable-accounts",
        {
            page: query.page,
            page_size: query.pageSize,
            q: query.q?.trim() || undefined,
            account_id: query.receivableAccountId,
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
    return apiGet<Page<BackendCustomerReceipt>>("/admin/customer-receipts", {
        page: query.page,
        page_size: query.pageSize,
        counterparty_party_id: query.counterpartyPartyId,
        receipt_no: query.q?.trim() || undefined,
        sales_order_id: query.salesOrderId,
        receivable_account_id: query.receivableAccountId,
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
    return apiGet<Page<BackendInvoice>>("/admin/invoices", {
        page: query.page,
        page_size: query.pageSize,
        invoice_direction: "sales",
        party_id: query.counterpartyPartyId,
        invoice_no: query.q?.trim() || undefined,
        sales_order_id: query.salesOrderId,
        receivable_account_id: query.receivableAccountId,
        sort_by: "invoice_date",
        sort_dir: "desc",
    })
}
