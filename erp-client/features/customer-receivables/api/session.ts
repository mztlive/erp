/**
 * Draft allocation session (client-held UI state; pool from HTTP).
 */

import { saveReceiptDraft } from "./receipt-draft"

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
    compareAmounts,
    subtractAmounts,
    sumAmounts,
} from "@/features/customer-receivables/lib/allocation-math"
import {
    businessLabelOrPlaceholder,
    MISSING_COUNTERPARTY_NAME,
    MISSING_CUSTOMER_NAME,
    MISSING_SALES_ORDER_NO,
    receivableEntryLabel,
} from "@/features/customer-receivables/lib/display-labels"

type AllocationPoolScope = {
    salesOrderId?: string
    receivableAccountId?: string
}

export async function buildPool(
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
                            label: `${salesOrderNo} · ${receivableEntryLabel(e.entry_type)}`,
                            salesOrderId: r.sales_order_id,
                            salesOrderNo,
                            salesOrderRevisionId:
                                r.current_sales_order_revision_id,
                            customerName: r.customer_name || undefined,
                            // 缺少服务端额度时不可选，不以原始金额降级。
                            openAmount: e.receipt_available ?? "0",
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

/** 已登记到账时刻按日期控件的上海时区回显，保留秒精度。 */
export function receiptReceivedAtInput(secs: number): string {
    const parts = new Intl.DateTimeFormat("en-CA", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
        second: "2-digit",
        hourCycle: "h23",
    }).formatToParts(new Date(instantToIso(secs)))
    const part = (type: Intl.DateTimeFormatPartTypes) =>
        parts.find((item) => item.type === type)?.value ?? ""
    return `${part("year")}-${part("month")}-${part("day")}T${part("hour")}:${part("minute")}:${part("second")}`
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
    let receiptCustomerId: string | null | undefined
    let serverStatus: string | undefined
    let approval: AllocationSessionView["approval"]
    let fact: AllocationSessionView["fact"] = {}
    let prefillAllocations: AllocationDraftLine[] = []
    let customerId = input.customerId ?? ""
    let customerName = input.customerName ?? ""
    const counterpartyPartyName =
        accounts.find(
            (account) =>
                account.counterparty_party_id === input.counterpartyPartyId &&
                account.counterparty_party_name?.trim(),
        )?.counterparty_party_name ?? input.counterpartyPartyName

    if (input.mode === "receipt" && input.existingFactId) {
        const r = await apiGet<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(input.existingFactId)}/draft`,
        )
        if (
            !r.counterparty_party_id ||
            !r.version ||
            r.unallocated_amount == null
        ) {
            throw {
                kind: "Validation",
                message:
                    "无法读取该回款的完整信息，请刷新或申请完整读取权限后再继续核销。",
            }
        }
        if (r.counterparty_party_id !== input.counterpartyPartyId) {
            throw {
                kind: "Validation",
                message: "该回款不属于本次往来主体，请重新选择回款与核销范围。",
            }
        }
        receiptCustomerId = r.customer_id ?? null
        serverStatus = r.status
        existingFactNo = r.receipt_no
        existingFactVersion = r.version
        approval = mapCustomerReceiptApproval(r.approval)
        customerId = r.customer_id ?? input.customerId ?? ""
        customerName = input.customerName ?? ""
        fact = {
            receivedAt: receiptReceivedAtInput(r.received_at),
            amount: r.amount,
            bankReference: r.bank_reference ?? undefined,
        }
        prefillAllocations = (r.pending_allocations ?? []).map((line) => {
            const target = pool.find(
                (item) => item.targetId === line.receivable_entry_id,
            )
            return {
                lineKey: `line_${line.receivable_entry_id}`,
                targetId: line.receivable_entry_id,
                targetKind: "receivable_entry" as const,
                label: target?.label ?? "原应收项目（当前不可选）",
                salesOrderNo: target?.salesOrderNo ?? "",
                openAmount: target?.openAmount ?? "0",
                amount: line.allocated_amount,
                baselineVersion: target?.baselineVersion ?? 0,
            }
        })
    } else if (input.mode === "invoice" && input.existingFactId) {
        const inv = stripInvoiceApprovalField(
            await apiGet<BackendInvoice>(
                `/admin/invoices/${encodeURIComponent(input.existingFactId)}`,
            ),
        )
        if (
            inv.invoice_no == null ||
            inv.invoice_date == null ||
            inv.net_amount == null ||
            inv.tax_amount == null
        ) {
            throw {
                kind: "Validation",
                message: "附件登记已按批准金额完成，无需继续分配。",
            }
        }
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

    if (
        !prefillAllocations.length &&
        !input.existingFactId &&
        (input.receivableAccountId || input.salesOrderId)
    ) {
        const match = pool.find((target) => {
            if (compareAmounts(target.openAmount, "0") <= 0) return false
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
    const customerAccounts = accounts.filter(
        (account) =>
            account.counterparty_party_id === input.counterpartyPartyId &&
            (!customerId || account.customer_id === customerId),
    )
    const customerAccount = customerAccounts.find((account) =>
        account.customer_name?.trim(),
    )
    if (
        customerAccount &&
        (customerId ||
            customerAccounts.every(
                (account) =>
                    account.customer_id === customerAccount.customer_id,
            ))
    ) {
        customerId ||= customerAccount.customer_id
        customerName = businessLabelOrPlaceholder(
            customerAccount.customer_name,
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
            counterpartyPartyName,
            input.counterpartyPartyId,
            MISSING_COUNTERPARTY_NAME,
        ),
        customerId,
        customerName: businessLabelOrPlaceholder(
            customerName,
            customerId,
            MISSING_CUSTOMER_NAME,
        ),
        status: serverStatus && serverStatus !== "draft" ? "posted" : "draft",
        serverStatus,
        existingFactId: input.existingFactId,
        existingFactNo,
        existingFactVersion,
        receiptCustomerId,
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

    if (session.mode === "receipt") return saveReceiptDraft(session, input)

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
