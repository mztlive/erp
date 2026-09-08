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

/** 使用服务端分页总数统计待复核应收，不能用当前页行数代替总数。 */
export async function loadPendingCardReviewCount(
    query: CustomerAccountsQuery,
): Promise<number> {
    const statuses = ["pending_opening", "pending_sync_diff"].filter(
        (status) =>
            query.view !== "receivable" ||
            !query.reviewStatus ||
            query.reviewStatus === "all" ||
            query.reviewStatus === status,
    )
    const pages = await Promise.all(
        statuses.map((reviewStatus) =>
            loadReceivables({
                ...query,
                q: query.view === "receivable" ? query.q : undefined,
                status: query.view === "receivable" ? query.status : undefined,
                reviewStatus,
                page: 1,
                pageSize: 1,
            }),
        ),
    )
    return pages.reduce((total, page) => total + page.total, 0)
}
