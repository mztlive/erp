/**
 * W13 卡券票款复核 API · 登记历史回款/发票
 * (POST /admin/customer-receipts、/admin/invoices)。
 */

import { apiPost } from "@/lib/api"
import { listWorkItems } from "@/features/work-items"
import type { RegisterFundsResult } from "@/features/card-funds-review/types"
import { instantToIso } from "./mappers"
import { loadAccount } from "./queue"
import type { BackendCustomerReceipt, BackendInvoice, BackendReceivableAccount } from "./dto"

/** 定位当前责任下的应收子账；不存在时按 404 拒绝。 */
async function requireRegistrationAccount(
    workItemId: string,
): Promise<BackendReceivableAccount> {
    const workItems = await listWorkItems({
        scope: "mine",
        workItemType: "CARD_FUNDS_REVIEW",
        currentWorkItemId: workItemId,
        timezone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        page: 1,
        pageSize: 1,
    })
    const workItem = workItems.items.find((item) => item.id === workItemId)
    const account = workItem ? await loadAccount(workItem.business_object_id) : null
    if (!account) {
        return Promise.reject({
            kind: "Http",
            message: "应收往来子账不存在",
            status: 404,
        })
    }
    return account
}

/**
 * 登记历史回款：create + post customer receipt with entry allocations.
 */
export async function registerHistoricalReceipt(input: {
    workItemId: string
    expectedSubjectVersion: string
    receiptNo: string
    receivedAt: string
    grossAmount: string
    allocations: readonly {
        lineId: string
        targetAccountId: string
        targetLabel: string
        amount: string
    }[]
    evidenceReference: string
}): Promise<RegisterFundsResult> {
    if (!input.grossAmount || Number(input.grossAmount) <= 0) {
        return Promise.reject({
            kind: "Validation",
            message: "禁止创建 0 元或负金额回款；无历史票款请使用「从 0 起」",
        })
    }

    const account = await requireRegistrationAccount(input.workItemId)

    // Prefer increase entries as allocation targets
    const increaseEntry = (account.entries ?? []).find(
        (e) => e.direction === "increase",
    )
    const receivedAtSecs = input.receivedAt
        ? Math.floor(new Date(input.receivedAt).getTime() / 1000)
        : Math.floor(Date.now() / 1000)

    const created = await apiPost<BackendCustomerReceipt>(
        "/admin/customer-receipts",
        {
            receipt_no: input.receiptNo,
            counterparty_party_id: account.counterparty_party_id,
            customer_id: account.customer_id,
            received_at: receivedAtSecs,
            amount: input.grossAmount,
            bank_reference: input.evidenceReference || undefined,
        },
    )

    const entryId = increaseEntry?.id
    let posted = created
    if (entryId) {
        posted = await apiPost<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(created.id)}/post`,
            {
                allocations: [
                    {
                        receivable_entry_id: entryId,
                        allocated_amount: input.grossAmount,
                    },
                ],
            },
        )
    }

    const refreshed = await loadAccount(account.id)
    const subjectHash = `acct:${account.id}:v${refreshed?.version ?? account.version}`
    return {
        fundsFactVersion: `ffv:${account.id}:v${refreshed?.version ?? account.version}`,
        subjectHash,
        settledTotal: refreshed?.settled_total ?? account.settled_total,
        invoicedTotal: refreshed?.invoiced_total ?? account.invoiced_total,
        openTotal: refreshed?.open_total ?? account.open_total,
        openInvoiceableTotal:
            refreshed?.open_invoiceable_total ?? account.open_invoiceable_total,
        receiptFacts: [
            {
                receiptId: posted.id,
                receiptNo: posted.receipt_no,
                receivedAt: instantToIso(posted.received_at),
                grossAmount: posted.amount,
                allocatedToAccount: posted.allocated_total,
                reversed: posted.status === "reversed",
            },
        ],
        invoiceFacts: [],
    }
}

export async function registerHistoricalInvoice(input: {
    workItemId: string
    expectedSubjectVersion: string
    invoiceNo: string
    issuedAt: string
    grossAmount: string
    netAmount: string
    taxAmount: string
    allocations: readonly {
        lineId: string
        targetAccountId: string
        targetLabel: string
        amount: string
    }[]
    evidenceReference: string
}): Promise<RegisterFundsResult> {
    if (!input.grossAmount || Number(input.grossAmount) <= 0) {
        return Promise.reject({
            kind: "Validation",
            message: "禁止创建 0 元或负金额发票；无历史票款请使用「从 0 起」",
        })
    }

    const account = await requireRegistrationAccount(input.workItemId)

    const created = await apiPost<BackendInvoice>("/admin/invoices", {
        invoice_direction: "sales",
        invoice_kind: "blue",
        party_id: account.counterparty_party_id,
        invoice_no: input.invoiceNo,
        invoice_date: input.issuedAt.slice(0, 10),
        gross_amount: input.grossAmount,
        net_amount: input.netAmount,
        tax_amount: input.taxAmount,
    })

    const posted = await apiPost<BackendInvoice>(
        `/admin/invoices/${encodeURIComponent(created.id)}/post`,
        {
            allocations: [
                {
                    receivable_account_id: account.id,
                    allocated_gross_amount: input.grossAmount,
                    allocated_net_amount: input.netAmount,
                    allocated_tax_amount: input.taxAmount,
                },
            ],
        },
    )

    const refreshed = await loadAccount(account.id)
    const subjectHash = `acct:${account.id}:v${refreshed?.version ?? account.version}`
    return {
        fundsFactVersion: `ffv:${account.id}:v${refreshed?.version ?? account.version}`,
        subjectHash,
        settledTotal: refreshed?.settled_total ?? account.settled_total,
        invoicedTotal: refreshed?.invoiced_total ?? account.invoiced_total,
        openTotal: refreshed?.open_total ?? account.open_total,
        openInvoiceableTotal:
            refreshed?.open_invoiceable_total ?? account.open_invoiceable_total,
        receiptFacts: [],
        invoiceFacts: [
            {
                invoiceId: posted.id,
                invoiceNo: posted.invoice_no,
                direction: "BLUE",
                issuedAt: posted.invoice_date,
                grossAmount: posted.gross_amount,
                netAmount: posted.net_amount,
                taxAmount: posted.tax_amount,
                allocatedToAccount: posted.allocated_total,
                reversed: false,
            },
        ],
    }
}
