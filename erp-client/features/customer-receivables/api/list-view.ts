/** List view + detail view composition (client projection of server data). */

import { apiGet } from "@/lib/api"

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
import { loadReceipts, loadReceivables, loadSalesInvoices } from "./loaders"
import type {
    BackendCustomerReceipt,
    BackendCustomerRefund,
    BackendInvoice,
    BackendReceiptReversal,
    BackendReceivableAccount,
} from "./dto"
import type { CustomerAccountsDetailKind } from "@/features/customer-receivables/types"

function emptyView(
    query: CustomerAccountsQuery,
    reason: CustomerAccountsListView["emptyReason"],
    moduleAllowed: boolean,
    hasDataScope: boolean,
): CustomerAccountsListView {
    return {
        view: query.view,
        metrics: {
            openReceivableTotal: "0.00",
            overdueReceivableTotal: "0.00",
            unallocatedReceiptTotal: "0.00",
            unallocatedInvoiceTotal: "0.00",
            cardPendingReviewCount: 0,
        },
        receivables: [],
        receipts: [],
        invoices: [],
        unallocated: {
            receipts: [],
            invoices: [],
            note: "待核销视图中回款与销项发票分区展示；两类未分配余额不得相加。",
        },
        counterparties: [],
        total: 0,
        filterSummary:
            reason === "NO_DATA_SCOPE"
                ? "当前角色未配置客户往来范围"
                : reason === "PERMISSION_REVOKED"
                  ? "权限已收回"
                  : "空",
        permissionVersion: "pv-w11-http-1",
        dataWatermark: "",
        queriedAt: new Date().toISOString(),
        hasDataScope,
        moduleAllowed,
        canRegister: moduleAllowed,
        canExport: moduleAllowed,
        emptyReason: reason,
        submitPolicy: {
            allowUnallocatedRemainder: true,
            label: "允许保留未分配余额（系统统一判定）",
        },
    }
}

// silence unused emptyView for future permission paths
void emptyView

export async function fetchCustomerAccountsList(
    query: CustomerAccountsQuery,
): Promise<CustomerAccountsListView> {
    const [recvPage, rcptPage, invPage] = await Promise.all([
        loadReceivables(query),
        loadReceipts(query),
        loadSalesInvoices(query),
    ])

    let receivables = (recvPage.items ?? []).map(projectReceivable)
    const partyDisplay = new Map(
        receivables.map((row) => [
            row.counterpartyPartyId,
            {
                counterpartyPartyName: row.counterpartyPartyName,
                customerName: row.customerName,
            },
        ]),
    )
    let receipts = (rcptPage.items ?? []).map((receipt) =>
        projectReceipt(
            receipt,
            partyDisplay.get(receipt.counterparty_party_id),
        ),
    )
    let invoices = (invPage.items ?? [])
        .filter((i) => i.invoice_direction === "sales" || !i.invoice_direction)
        .map((invoice) =>
            projectInvoice(invoice, partyDisplay.get(invoice.party_id)),
        )

    if (query.receivableAccountId) {
        receivables = receivables.filter(
            (r) => r.accountId === query.receivableAccountId,
        )
    }
    if (query.q?.trim()) {
        const q = query.q.trim().toLowerCase()
        if (query.view === "receivable" || query.view === "unallocated") {
            receivables = receivables.filter(
                (r) =>
                    r.accountId.toLowerCase().includes(q) ||
                    r.salesOrderId.toLowerCase().includes(q) ||
                    r.customerId.toLowerCase().includes(q) ||
                    r.counterpartyPartyId.toLowerCase().includes(q),
            )
        }
    }

    if (query.salesOrderId || query.receivableAccountId) {
        const targetIds = new Set<string>()
        for (const account of receivables) {
            targetIds.add(account.accountId)
            for (const entry of account.entries) targetIds.add(entry.entryId)
        }
        const belongsToCurrentOrder = (
            allocations: readonly { targetId: string }[],
        ) =>
            allocations.some((allocation) => targetIds.has(allocation.targetId))
        receipts = receipts.filter((receipt) =>
            belongsToCurrentOrder(receipt.allocations),
        )
        invoices = invoices.filter((invoice) =>
            belongsToCurrentOrder(invoice.allocations),
        )
    }

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

    const orderScoped = Boolean(query.salesOrderId || query.receivableAccountId)
    let total = 0
    if (query.view === "receivable") {
        total = orderScoped
            ? receivables.length
            : (recvPage.total ?? receivables.length)
    } else if (query.view === "receipt") {
        total = orderScoped
            ? receipts.length
            : (rcptPage.total ?? receipts.length)
    } else if (query.view === "sales_invoice") {
        total = orderScoped
            ? invoices.length
            : (invPage.total ?? invoices.length)
    } else {
        total = unallocatedReceipts.length + unallocatedInvoices.length
    }

    const hasFilters = Boolean(
        query.q?.trim() ||
        query.counterpartyPartyId ||
        query.customerId ||
        query.due ||
        query.status ||
        query.reviewStatus ||
        query.salesOrderId,
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

    // metrics：后端无汇总端点 — 缺口登记，返回占位 0（禁止前端求和冒充）
    return {
        view: query.view,
        metrics: {
            openReceivableTotal: "0.00",
            overdueReceivableTotal: "0.00",
            unallocatedReceiptTotal: "0.00",
            unallocatedInvoiceTotal: "0.00",
            cardPendingReviewCount: 0,
        },
        receivables,
        receipts,
        invoices,
        unallocated: {
            receipts: unallocatedReceipts,
            invoices: unallocatedInvoices,
            note: "待核销视图中回款与销项发票分区展示；两类未分配余额不得相加为单一指标。",
        },
        counterparties: [...cpMap.values()],
        total,
        filterSummary: filterSummary(query),
        permissionVersion: "pv-w11-http-1",
        dataWatermark: `wm-w11-${recvPage.total ?? 0}-${rcptPage.total ?? 0}-${invPage.total ?? 0}`,
        queriedAt: new Date().toISOString(),
        hasDataScope: true,
        moduleAllowed: true,
        canRegister: true,
        canExport: true,
        emptyReason:
            total === 0
                ? hasFilters
                    ? "FILTER_NO_RESULT"
                    : "NO_DATA"
                : undefined,
        submitPolicy: {
            allowUnallocatedRemainder: true,
            label: "允许保留未分配余额（系统统一判定）",
        },
    }
}

export async function fetchCustomerAccountsDetail(
    kind: CustomerAccountsDetailKind,
    id: string,
): Promise<CustomerAccountsDetailView | null> {
    if (kind === "receivable") {
        try {
            const seed = await apiGet<BackendReceivableAccount>(
                `/admin/receivable-accounts/${encodeURIComponent(id)}`,
            )
            return {
                kind,
                receivable: projectReceivable(seed),
                queriedAt: new Date().toISOString(),
            }
        } catch {
            return null
        }
    }
    if (kind === "receipt") {
        try {
            const seed = await apiGet<BackendCustomerReceipt>(
                `/admin/customer-receipts/${encodeURIComponent(id)}`,
            )
            return {
                kind,
                receipt: projectReceipt(seed),
                queriedAt: new Date().toISOString(),
            }
        } catch {
            return null
        }
    }
    if (kind === "refund") {
        try {
            const seed = await apiGet<BackendCustomerRefund>(
                `/admin/customer-refunds/${encodeURIComponent(id)}`,
            )
            return {
                kind,
                refund: projectCustomerRefund(seed),
                queriedAt: new Date().toISOString(),
            }
        } catch {
            return null
        }
    }
    if (kind === "reversal") {
        try {
            const seed = await apiGet<BackendReceiptReversal>(
                `/admin/receipt-reversals/${encodeURIComponent(id)}`,
            )
            return {
                kind,
                reversal: projectReceiptReversal(seed),
                queriedAt: new Date().toISOString(),
            }
        } catch {
            return null
        }
    }
    try {
        const seed = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(id)}`,
        )
        return {
            kind,
            invoice: projectInvoice(seed),
            queriedAt: new Date().toISOString(),
        }
    } catch {
        return null
    }
}
