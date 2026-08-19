/**
 * Draft allocation session (client-held UI state; pool from HTTP).
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api"

import type {
    AllocationDraftLine,
    AllocationSessionView,
    CreateSessionInput,
    SaveAllocationDraftInput,
} from "@/features/customer-receivables/types"
import type {
    BackendCustomerReceipt,
    BackendInvoice,
    BackendReceivableAccount,
} from "./dto"
import { instantToIso } from "./mappers"
import { mapCustomerReceiptApproval } from "@/features/customer-receivables/lib/customer-receipt-approval"

export const sessions = new Map<string, AllocationSessionView>()
let sessionSeq = 100

async function buildPool(
    mode: "receipt" | "invoice",
    counterpartyPartyId: string,
): Promise<AllocationSessionView["pool"]> {
    const page = await apiGet<Page<BackendReceivableAccount>>(
        "/admin/receivable-accounts",
        {
            counterparty_party_id: counterpartyPartyId,
            page: 1,
            page_size: 100,
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
    const rows = page.items ?? []
    if (mode === "receipt") {
        return rows.flatMap((r) =>
            (r.entries ?? [])
                .filter((e) => e.direction === "increase")
                .map((e) => ({
                    targetId: e.id,
                    targetKind: "receivable_entry" as const,
                    label: `${r.sales_order_id} · ${e.entry_type}`,
                    salesOrderNo: r.sales_order_id,
                    // open amount is server field on account; entry-level open is not exposed — use amount as display open
                    openAmount: e.amount,
                    dueDate: e.due_date,
                    counterpartyPartyId: r.counterparty_party_id,
                    baselineVersion: r.version,
                })),
        )
    }
    return rows
        .filter(
            (r) =>
                r.open_invoiceable_total &&
                r.open_invoiceable_total !== "0" &&
                r.open_invoiceable_total !== "0.00",
        )
        .map((r) => ({
            targetId: r.id,
            targetKind: "receivable_account" as const,
            label: `应收子账 #${r.account_seq} · ${r.sales_order_id}`,
            salesOrderNo: r.sales_order_id,
            openAmount: r.open_invoiceable_total,
            dueDate: r.entries?.[0]?.due_date,
            counterpartyPartyId: r.counterparty_party_id,
            baselineVersion: r.version,
        }))
}

function recomputeProposed(
    factAmount: string,
    allocations: readonly AllocationDraftLine[],
): { proposedAllocatedTotal: string; proposedUnallocated: string } {
    // Display-only draft hint; formal balances come from server after post.
    let allocated = 0
    for (const a of allocations) {
        const n = Number(a.amount)
        if (Number.isFinite(n)) allocated += n
    }
    const total = Number(factAmount)
    const t = Number.isFinite(total) ? total : 0
    return {
        proposedAllocatedTotal: allocated.toFixed(2),
        proposedUnallocated: Math.max(0, t - allocated).toFixed(2),
    }
}

export async function createAllocationSession(
    input: CreateSessionInput,
): Promise<AllocationSessionView> {
    const pool = await buildPool(input.mode, input.counterpartyPartyId)
    let existingFactNo: string | undefined
    let existingFactVersion: number | undefined
    let approval: AllocationSessionView["approval"]
    let fact: AllocationSessionView["fact"] = {}
    let prefillAllocations: AllocationDraftLine[] = []
    let customerId = ""
    let customerName = ""

    if (input.mode === "receipt" && input.existingFactId) {
        const r = await apiGet<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(input.existingFactId)}`,
        )
        existingFactNo = r.receipt_no
        existingFactVersion = r.version
        approval = mapCustomerReceiptApproval(r.approval)
        customerId = r.customer_id ?? ""
        customerName = r.customer_id ?? ""
        fact = {
            receivedAt: instantToIso(r.received_at).slice(0, 16),
            amount: r.unallocated_amount,
            bankReference: r.bank_reference ?? undefined,
        }
    } else if (input.mode === "invoice" && input.existingFactId) {
        const inv = await apiGet<BackendInvoice>(
            `/admin/invoices/${encodeURIComponent(input.existingFactId)}`,
        )
        existingFactNo = inv.invoice_no
        fact = {
            invoiceCode: inv.invoice_code ?? undefined,
            invoiceNo: inv.invoice_no,
            invoiceDate: inv.invoice_date,
            grossAmount: inv.unallocated_amount,
            netAmount: inv.net_amount,
            taxAmount: inv.tax_amount,
            invoiceKind: "blue",
        }
    } else {
        const now = new Date()
        const pad = (n: number) => String(n).padStart(2, "0")
        const local = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`
        if (input.mode === "receipt") {
            fact = { receivedAt: local, amount: "", bankReference: "" }
        } else {
            fact = {
                invoiceDate: local.slice(0, 10),
                invoiceNo: "",
                invoiceCode: "",
                grossAmount: "",
                netAmount: "",
                taxAmount: "",
                invoiceKind: "blue",
            }
        }
    }

    if (input.receivableAccountId || input.salesOrderId) {
        const match = pool.find((p) => {
            if (input.receivableAccountId) {
                if (p.targetKind === "receivable_account") {
                    return p.targetId === input.receivableAccountId
                }
                return p.label.includes(input.receivableAccountId)
            }
            if (input.salesOrderId) {
                return p.salesOrderNo === input.salesOrderId
            }
            return false
        })
        if (match) {
            prefillAllocations = [
                {
                    lineKey: `line_${match.targetId}`,
                    targetId: match.targetId,
                    targetKind: match.targetKind,
                    label: match.label,
                    salesOrderNo: match.salesOrderNo,
                    openAmount: match.openAmount,
                    amount: "",
                    baselineVersion: match.baselineVersion,
                },
            ]
        }
    }

    // Resolve customer from first pool account if available
    if (!customerId && pool.length > 0) {
        try {
            const detail = await apiGet<BackendReceivableAccount>(
                `/admin/receivable-accounts/${encodeURIComponent(
                    pool[0]!.targetKind === "receivable_account"
                        ? pool[0]!.targetId
                        : ((
                              await apiGet<Page<BackendReceivableAccount>>(
                                  "/admin/receivable-accounts",
                                  {
                                      counterparty_party_id:
                                          input.counterpartyPartyId,
                                      page: 1,
                                      page_size: 1,
                                  },
                              )
                          ).items[0]?.id ?? ""),
                )}`,
            )
            customerId = detail.customer_id
            customerName = detail.customer_id
        } catch {
            // leave empty — display gap
        }
    }

    const draftSessionId = `alloc_cust_${++sessionSeq}`
    const factAmount =
        input.mode === "receipt"
            ? (fact.amount ?? "0")
            : (fact.grossAmount ?? "0")
    const proposed = recomputeProposed(factAmount, prefillAllocations)

    const view: AllocationSessionView = {
        draftSessionId,
        mode: input.mode,
        counterpartyPartyId: input.counterpartyPartyId,
        counterpartyPartyName: input.counterpartyPartyId,
        customerId,
        customerName,
        status: "draft",
        existingFactId: input.existingFactId,
        existingFactNo,
        existingFactVersion,
        approval,
        fact,
        pool,
        allocations: prefillAllocations,
        factAmount,
        ...proposed,
        submitPolicy: {
            allowUnallocatedRemainder: true,
            label: "允许保留未分配余额（系统统一判定）",
        },
        returnContext: {
            returnTo: input.returnTo,
            from: input.from,
            salesOrderId: input.salesOrderId,
        },
        leaseValid: true,
        editVersion: 1,
        note: "本次核销已锁定往来主体；拟分配合计仅作输入提示，以提交后系统结果为准。",
    }
    sessions.set(draftSessionId, view)
    return view
}

export async function fetchAllocationSession(
    draftSessionId: string,
): Promise<AllocationSessionView | null> {
    const s = sessions.get(draftSessionId)
    if (!s) return null
    const pool = await buildPool(s.mode, s.counterpartyPartyId)
    const factAmount =
        s.mode === "receipt"
            ? (s.fact.amount ?? "0")
            : (s.fact.grossAmount ?? "0")
    const proposed = recomputeProposed(factAmount, s.allocations)
    return {
        ...s,
        pool,
        factAmount,
        ...proposed,
    }
}

export async function saveAllocationDraft(
    input: SaveAllocationDraftInput,
): Promise<AllocationSessionView> {
    const s = sessions.get(input.draftSessionId)
    if (!s || s.status !== "draft") {
        return Promise.reject({
            kind: "Validation",
            message: "草稿已不存在或已确认。",
        })
    }
    if (input.editVersion !== s.editVersion) {
        return Promise.reject({
            kind: "Http",
            message: "草稿数据已更新，请刷新后重试。",
            status: 409,
        })
    }
    const next: AllocationSessionView = {
        ...s,
        fact: { ...input.fact },
        allocations: input.allocations.map((a) => ({ ...a })),
        editVersion: s.editVersion + 1,
        savedAt: new Date().toISOString(),
    }
    sessions.set(input.draftSessionId, next)
    return (await fetchAllocationSession(input.draftSessionId))!
}
