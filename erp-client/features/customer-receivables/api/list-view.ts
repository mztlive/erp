/** List view + detail view composition (client projection of server data). */

import { apiGet } from "@/lib/api"
import { nullIfNotFound } from "@/lib/api/errors"

import type {
    CustomerAccountsDetailView,
    CustomerAccountsListView,
    CustomerAccountsQuery,
} from "@/features/customer-receivables/types"
import {
    filterSummary,
    projectCustomerRefund,
    projectInvoice,
    projectReceipt,
    projectReceiptReversal,
    projectReceivable,
} from "./mappers"
import {
    loadAccountPage,
    loadReceipts,
    loadReceivables,
    loadSalesInvoices,
    type CustomerAccountsPage,
} from "./loaders"
import type {
    BackendCustomerReceipt,
    BackendCustomerRefund,
    BackendInvoice,
    BackendReceiptReversal,
    BackendReceivableAccount,
} from "./dto"
import type { CustomerAccountsDetailKind } from "@/features/customer-receivables/types"

export type CustomerAccountsListAccess = Readonly<{
    receivable: boolean
    receipt: boolean
    sales_invoice: boolean
}>

export type CustomerAccountsLedgerListView = CustomerAccountsListView & {
    unallocated: CustomerAccountsListView["unallocated"] & {
        receiptsNoScope: boolean
        invoicesNoScope: boolean
    }
}

export async function fetchCustomerAccountsList(
    query: CustomerAccountsQuery,
    access?: CustomerAccountsListAccess,
): Promise<CustomerAccountsLedgerListView> {
    const emptyPage = {
        items: [],
        total: 0,
        page: query.page,
        page_size: query.pageSize,
    }
    const receivableActive = query.view === "receivable"
    const receiptActive =
        query.view === "receipt" || query.view === "unallocated"
    const invoiceActive =
        query.view === "sales_invoice" || query.view === "unallocated"
    const [recvResult, rcptResult, invResult] = await Promise.allSettled([
        receivableActive || access?.receivable !== false
            ? loadAccountPage(query, receivableActive, loadReceivables)
            : undefined,
        receiptActive || access?.receipt !== false
            ? loadAccountPage(query, receiptActive, loadReceipts)
            : undefined,
        invoiceActive || access?.sales_invoice !== false
            ? loadAccountPage(query, invoiceActive, loadSalesInvoices)
            : undefined,
    ])
    // 当前列表失败正常报错；其他卡片无法读取时显示未知，不阻断已有列表。
    for (const [active, result] of [
        [receivableActive, recvResult],
        [receiptActive, rcptResult],
        [invoiceActive, invResult],
    ] as const) {
        if (active && result.status === "rejected") throw result.reason
    }
    const recvResultPage =
        recvResult.status === "fulfilled" ? recvResult.value : undefined
    const rcptResultPage =
        rcptResult.status === "fulfilled" ? rcptResult.value : undefined
    const invResultPage =
        invResult.status === "fulfilled" ? invResult.value : undefined
    const recvPage: CustomerAccountsPage<BackendReceivableAccount> =
        (receivableActive ? recvResultPage : undefined) ?? emptyPage
    const rcptPage: CustomerAccountsPage<BackendCustomerReceipt> =
        (receiptActive ? rcptResultPage : undefined) ?? emptyPage
    const invPage: CustomerAccountsPage<BackendInvoice> =
        (invoiceActive ? invResultPage : undefined) ?? emptyPage
    const receivableSummary = recvResultPage?.summary
    const receiptSummary = rcptResultPage?.summary
    const invoiceSummary = invResultPage?.summary

    const hasDataScope = receivableActive
        ? recvPage.empty_reason !== "no_scope"
        : query.view === "receipt"
          ? rcptPage.empty_reason !== "no_scope"
          : query.view === "sales_invoice"
            ? invPage.empty_reason !== "no_scope"
            : rcptPage.empty_reason !== "no_scope" ||
              invPage.empty_reason !== "no_scope"

    const receivables = (recvPage.items ?? []).map(projectReceivable)
    const partyDisplay = new Map(
        receivables.map((row) => [
            row.counterpartyPartyId,
            {
                counterpartyPartyName: row.counterpartyPartyName,
                customerName: row.customerName,
            },
        ]),
    )
    const receipts = (rcptPage.items ?? []).map((receipt) =>
        projectReceipt(receipt),
    )
    const invoices = (invPage.items ?? [])
        .filter((i) => i.invoice_direction === "sales" || !i.invoice_direction)
        .map((invoice) =>
            projectInvoice(invoice, partyDisplay.get(invoice.party_id)),
        )

    // due filter: backend gap — cannot filter overdue without due_state
    const unallocatedReceipts = receipts.filter(
        (r) =>
            r.status === "posted" &&
            r.unallocatedAmount &&
            r.unallocatedAmount !== "0" &&
            r.unallocatedAmount !== "0.00",
    )
    const unallocatedInvoices = invoices.filter(
        (i) =>
            i.invoiceKind === "blue" &&
            i.status === "registered" &&
            i.unallocatedAmount &&
            i.unallocatedAmount !== "0" &&
            i.unallocatedAmount !== "0.00",
    )

    let total = 0
    if (query.view === "receivable") {
        total = recvPage.total ?? receivables.length
    } else if (query.view === "receipt") {
        total = rcptPage.total ?? receipts.length
    } else if (query.view === "sales_invoice") {
        total = invPage.total ?? invoices.length
    } else {
        total = unallocatedReceipts.length + unallocatedInvoices.length
    }

    const hasFilters = Boolean(
        query.q?.trim() ||
        query.counterpartyPartyId ||
        query.customerId ||
        query.due ||
        query.status ||
        query.salesOrderId ||
        query.receivableAccountId,
    )

    const cpMap = new Map<
        string,
        {
            counterpartyPartyId: string
            counterpartyPartyName: string
            customerId: string
            customerName: string
        }
    >()
    for (const r of receivables) {
        if (!cpMap.has(r.counterpartyPartyId)) {
            cpMap.set(r.counterpartyPartyId, {
                counterpartyPartyId: r.counterpartyPartyId,
                counterpartyPartyName: r.counterpartyPartyName,
                customerId: r.customerId,
                customerName: r.customerName,
            })
        }
    }

    // 汇总来自服务端同一授权条件下的全部匹配记录，不合计当前页。
    return {
        view: query.view,
        metrics: {
            openReceivableTotal: receivableSummary?.balance_total ?? null,
            overdueReceivableTotal: receivableSummary?.overdue_total ?? null,
            unallocatedReceiptTotal: receiptSummary?.balance_total ?? null,
            unallocatedInvoiceTotal: invoiceSummary?.balance_total ?? null,
        },
        receivables,
        receipts,
        invoices,
        unallocated: {
            receipts: unallocatedReceipts,
            receiptsNoScope: rcptPage.empty_reason === "no_scope",
            invoices: unallocatedInvoices,
            invoicesNoScope: invPage.empty_reason === "no_scope",
            note: "下方分别列出本页待核销的回款与待关联销售单的销项发票。",
        },
        counterparties: [...cpMap.values()],
        total,
        filterSummary: filterSummary(query),
        permissionVersion: "pv-w11-http-1",
        dataWatermark: `wm-w11-${recvPage.total ?? 0}-${rcptPage.total ?? 0}-${invPage.total ?? 0}`,
        queriedAt: new Date().toISOString(),
        hasDataScope,
        moduleAllowed: true,
        canRegister: true,
        canExport: true,
        emptyReason: !hasDataScope
            ? "NO_DATA_SCOPE"
            : total === 0
              ? hasFilters
                  ? "FILTER_NO_RESULT"
                  : "NO_DATA"
              : undefined,
        submitPolicy: {
            allowUnallocatedRemainder: true,
            label: "允许保留未分配余额",
        },
    }
}

function loadDetailOrNull<T>(path: string): Promise<T | null> {
    return nullIfNotFound(() => apiGet<T>(path))
}

export async function fetchCustomerAccountsDetail(
    kind: CustomerAccountsDetailKind,
    id: string,
): Promise<CustomerAccountsDetailView | null> {
    const queriedAt = new Date().toISOString()
    if (kind === "receivable") {
        const seed = await loadDetailOrNull<BackendReceivableAccount>(
            `/admin/receivable-accounts/${encodeURIComponent(id)}`,
        )
        return seed
            ? { kind, receivable: projectReceivable(seed), queriedAt }
            : null
    }
    if (kind === "receipt") {
        const seed = await loadDetailOrNull<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(id)}`,
        )
        return seed ? { kind, receipt: projectReceipt(seed), queriedAt } : null
    }
    if (kind === "refund") {
        const seed = await loadDetailOrNull<BackendCustomerRefund>(
            `/admin/customer-refunds/${encodeURIComponent(id)}`,
        )
        return seed
            ? { kind, refund: projectCustomerRefund(seed), queriedAt }
            : null
    }
    if (kind === "reversal") {
        const seed = await loadDetailOrNull<BackendReceiptReversal>(
            `/admin/receipt-reversals/${encodeURIComponent(id)}`,
        )
        return seed
            ? { kind, reversal: projectReceiptReversal(seed), queriedAt }
            : null
    }
    const seed = await loadDetailOrNull<BackendInvoice>(
        `/admin/invoices/${encodeURIComponent(id)}`,
    )
    return seed ? { kind, invoice: projectInvoice(seed), queriedAt } : null
}
