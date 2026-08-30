/**
 * Draft allocation session (client-held UI state; pool from HTTP).
 */

import { apiGet, type Page } from "@/lib/api"

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
import { stripInvoiceApprovalField } from "@/features/customer-receivables/lib/invoice-no-approval"
import {
    subtractAmounts,
    sumAmounts,
} from "@/features/customer-receivables/lib/allocation-math"
import {
    businessLabelOrPlaceholder,
    MISSING_COUNTERPARTY_NAME,
    MISSING_CUSTOMER_NAME,
    MISSING_SALES_ORDER_NO,
} from "@/features/customer-receivables/lib/display-labels"

type AllocationPoolScope = {
    salesOrderId?: string
    receivableAccountId?: string
}

async function buildPool(
    mode: "receipt" | "invoice",
    counterpartyPartyId: string,
    scope: AllocationPoolScope = {},
): Promise<{
    pool: AllocationSessionView["pool"]
    accounts: readonly BackendReceivableAccount[]
}> {
    const page = await apiGet<Page<BackendReceivableAccount>>(
        "/admin/receivable-accounts",
        {
            page: 1,
            page_size: 100,
            account_id: scope.receivableAccountId,
            counterparty_party_id: counterpartyPartyId,
            sales_order_id: scope.salesOrderId,
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
    const rows = page.items ?? []
    if (page.total > rows.length) {
        throw {
            kind: "Validation",
            message:
                "可核销子账超过 100 条，请先限定销售单或应收子账后再登记。",
        }
    }
    if (mode === "receipt") {
        return {
            accounts: rows,
            pool: rows.flatMap((r) =>
                (r.entries ?? [])
                    .filter((e) => e.direction === "increase")
                    .map((e) => {
                        const salesOrderNo = businessLabelOrPlaceholder(
                            r.sales_order_no,
                            r.sales_order_id,
                            MISSING_SALES_ORDER_NO,
                        )
                        return {
                            targetId: e.id,
                            targetKind: "receivable_entry" as const,
                            label: `${salesOrderNo} · ${e.entry_type}`,
                            salesOrderId: r.sales_order_id,
                            salesOrderNo,
                            // open amount is server field on account; entry-level open is not exposed — use amount as display open
                            openAmount: e.amount,
                            dueDate: e.due_date,
                            counterpartyPartyId: r.counterparty_party_id,
                            baselineVersion: r.version,
                        }
                    }),
            ),
        }
    }
    return {
        accounts: rows,
        pool: rows
            .filter(
                (r) =>
                    r.open_invoiceable_total &&
                    r.open_invoiceable_total !== "0" &&
                    r.open_invoiceable_total !== "0.00",
            )
            .map((r) => {
                const salesOrderNo = businessLabelOrPlaceholder(
                    r.sales_order_no,
                    r.sales_order_id,
                    MISSING_SALES_ORDER_NO,
                )
                return {
                    targetId: r.id,
                    targetKind: "receivable_account" as const,
                    label: `应收子账 #${r.account_seq} · ${salesOrderNo}`,
                    salesOrderId: r.sales_order_id,
                    salesOrderNo,
                    openAmount: r.open_invoiceable_total,
                    dueDate: r.entries?.[0]?.due_date,
                    counterpartyPartyId: r.counterparty_party_id,
                    baselineVersion: r.version,
                }
            }),
    }
}

function recomputeProposed(
    factAmount: string,
    allocations: readonly AllocationDraftLine[],
): { proposedAllocatedTotal: string; proposedUnallocated: string } {
    // Display-only draft hint; formal balances come from server after post.
    const allocated = sumAmounts(allocations.map((item) => item.amount))
    return {
        proposedAllocatedTotal: allocated,
        proposedUnallocated: subtractAmounts(factAmount, allocated, true),
    }
}

export async function createAllocationSession(
    input: CreateSessionInput,
): Promise<AllocationSessionView> {
    const { accounts, pool } = await buildPool(
        input.mode,
        input.counterpartyPartyId,
        {
            salesOrderId: input.salesOrderId,
            receivableAccountId: input.receivableAccountId,
        },
    )
    let existingFactNo: string | undefined
    let existingFactVersion: number | undefined
    let approval: AllocationSessionView["approval"]
    let fact: AllocationSessionView["fact"] = {}
    let prefillAllocations: AllocationDraftLine[] = []
    let customerId = input.customerId ?? ""
    let customerName = input.customerName ?? ""

    if (input.mode === "receipt" && input.existingFactId) {
        const r = await apiGet<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(input.existingFactId)}`,
        )
        existingFactNo = r.receipt_no
        existingFactVersion = r.version
        approval = mapCustomerReceiptApproval(r.approval)
        customerId = r.customer_id ?? ""
        customerName = input.customerName ?? ""
        fact = {
            receivedAt: instantToIso(r.received_at).slice(0, 16),
            amount: r.unallocated_amount,
            bankReference: r.bank_reference ?? undefined,
        }
    } else if (input.mode === "invoice" && input.existingFactId) {
        const inv = stripInvoiceApprovalField(
            await apiGet<BackendInvoice>(
                `/admin/invoices/${encodeURIComponent(input.existingFactId)}`,
            ),
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
        const match = pool.find((target) => {
            if (input.receivableAccountId) {
                return (
                    target.targetKind === "receivable_entry" ||
                    target.targetId === input.receivableAccountId
                )
            }
            return target.salesOrderId === input.salesOrderId
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

    // Resolve the customer from the same immutable account scope.
    if (!customerId && accounts.length > 0) {
        const account = accounts[0]
        customerId = account?.customer_id ?? ""
        customerName = businessLabelOrPlaceholder(
            account?.customer_name,
            customerId,
            MISSING_CUSTOMER_NAME,
        )
    }

    const draftSessionId = `alloc_cust_${crypto.randomUUID()}`
    const factAmount =
        input.mode === "receipt"
            ? (fact.amount ?? "0")
            : (fact.grossAmount ?? "0")
    const proposed = recomputeProposed(factAmount, prefillAllocations)

    const view: AllocationSessionView = {
        draftSessionId,
        mode: input.mode,
        counterpartyPartyId: input.counterpartyPartyId,
        counterpartyPartyName: businessLabelOrPlaceholder(
            input.counterpartyPartyName,
            input.counterpartyPartyId,
            MISSING_COUNTERPARTY_NAME,
        ),
        customerId,
        customerName: businessLabelOrPlaceholder(
            customerName,
            customerId,
            MISSING_CUSTOMER_NAME,
        ),
        status: "draft",
        existingFactId: input.existingFactId,
        existingFactNo,
        existingFactVersion,
        // Invoice 为 NO_APPROVAL，会话不得携带审批绑定。
        approval: input.mode === "receipt" ? approval : undefined,
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
            receivableAccountId: input.receivableAccountId,
        },
        leaseValid: true,
        editVersion: 1,
        note: "本次核销已锁定往来主体；拟分配合计仅作输入提示，以提交后系统结果为准。",
    }
    return view
}

export async function refreshAllocationSession(
    session: AllocationSessionView,
): Promise<AllocationSessionView> {
    const { pool } = await buildPool(
        session.mode,
        session.counterpartyPartyId,
        {
            salesOrderId: session.returnContext?.salesOrderId,
            receivableAccountId: session.returnContext?.receivableAccountId,
        },
    )
    const factAmount =
        session.mode === "receipt"
            ? (session.fact.amount ?? "0")
            : (session.fact.grossAmount ?? "0")
    const proposed = recomputeProposed(factAmount, session.allocations)
    return {
        ...session,
        pool,
        factAmount,
        ...proposed,
    }
}

export async function saveAllocationDraft(
    session: AllocationSessionView | null,
    input: SaveAllocationDraftInput,
): Promise<AllocationSessionView> {
    if (!session || session.status !== "draft") {
        return Promise.reject({
            kind: "Validation",
            message: "草稿已不存在或已确认。",
        })
    }
    if (input.draftSessionId !== session.draftSessionId) {
        return Promise.reject({
            kind: "Validation",
            message: "草稿身份不一致，请重新进入核销页面。",
        })
    }
    if (input.editVersion !== session.editVersion) {
        return Promise.reject({
            kind: "Http",
            message: "草稿数据已更新，请刷新后重试。",
            status: 409,
        })
    }

    const { pool } = await buildPool(
        session.mode,
        session.counterpartyPartyId,
        {
            salesOrderId: session.returnContext?.salesOrderId,
            receivableAccountId: session.returnContext?.receivableAccountId,
        },
    )
    const targets = new Map(
        pool.map((target) => [
            `${target.targetKind}:${target.targetId}`,
            target,
        ]),
    )
    const allocations = input.allocations.map((line) => {
        const target = targets.get(`${line.targetKind}:${line.targetId}`)
        if (!target) {
            throw {
                kind: "Validation",
                message: "分配目标不属于本次核销范围，请刷新后重新选择。",
            }
        }
        return {
            ...line,
            label: target.label,
            salesOrderNo: target.salesOrderNo,
            openAmount: target.openAmount,
            baselineVersion: target.baselineVersion,
        }
    })

    const next: AllocationSessionView = {
        ...session,
        fact: { ...input.fact },
        pool,
        allocations,
        editVersion: session.editVersion + 1,
        savedAt: new Date().toISOString(),
    }
    return next
}
