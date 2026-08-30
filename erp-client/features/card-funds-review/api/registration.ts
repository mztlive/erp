/**
 * W13 卡券票款复核 API · 登记历史回款/发票
 * 每次提交只调用一个 W13 原子登记命令。
 */

import { apiPost } from "@/lib/api"
import { compareDecimal } from "@/lib/fixed-decimal"
import type { RegisterFundsResult } from "@/features/card-funds-review/types"
import type { BackendCardFundsRegistrationResult } from "./dto"

function projectRegistrationResult(
    result: BackendCardFundsRegistrationResult,
): RegisterFundsResult {
    return {
        fundsFactVersion: result.funds_fact_version,
        subjectHash: result.subject_hash,
        settledTotal: result.settled_total,
        invoicedTotal: result.invoiced_total,
        openTotal: result.open_total,
        openInvoiceableTotal: result.open_invoiceable_total,
        receiptFacts: result.receipt_facts.map((fact) => ({
            receiptId: fact.receipt_id,
            receiptNo: fact.receipt_no,
            receivedAt: fact.received_at,
            grossAmount: fact.gross_amount,
            allocatedToAccount: fact.allocated_to_account,
            otherAllocationSummary: fact.other_allocation_summary ?? undefined,
            reversed: fact.reversed,
        })),
        invoiceFacts: result.invoice_facts.map((fact) => ({
            invoiceId: fact.invoice_id,
            invoiceNo: fact.invoice_no,
            direction: fact.direction,
            issuedAt: fact.issued_at,
            grossAmount: fact.gross_amount,
            netAmount: fact.net_amount,
            taxAmount: fact.tax_amount,
            allocatedToAccount: fact.allocated_to_account,
            reversed: fact.reversed,
        })),
    }
}

/**
 * 一次登记历史回款、核销分配、子账进度和审计。
 */
export async function registerHistoricalReceipt(input: {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    expectedFundsFactVersion: string
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
    idempotencyKey: string
}): Promise<RegisterFundsResult> {
    if (!input.grossAmount || compareDecimal(input.grossAmount, "0", 2) <= 0) {
        return Promise.reject({
            kind: "Validation",
            message: "禁止创建 0 元或负金额回款；无历史票款请使用「从 0 起」",
        })
    }

    const receivedAtSecs = input.receivedAt
        ? Math.floor(new Date(input.receivedAt).getTime() / 1000)
        : Math.floor(Date.now() / 1000)
    const result = await apiPost<BackendCardFundsRegistrationResult>(
        "/admin/card-funds-review/receipts",
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            expected_funds_fact_version: input.expectedFundsFactVersion,
            receipt_no: input.receiptNo.trim() || undefined,
            received_at: receivedAtSecs,
            gross_amount: input.grossAmount,
            allocations: input.allocations.map((line) => ({
                target_account_id: line.targetAccountId,
                amount: line.amount,
            })),
            evidence_reference: input.evidenceReference,
            idempotency_key: input.idempotencyKey,
        },
    )
    return projectRegistrationResult(result)
}

export async function registerHistoricalInvoice(input: {
    workItemId: string
    expectedTaskVersion: string
    expectedSubjectVersion: string
    expectedFundsFactVersion: string
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
    idempotencyKey: string
}): Promise<RegisterFundsResult> {
    if (!input.grossAmount || compareDecimal(input.grossAmount, "0", 2) <= 0) {
        return Promise.reject({
            kind: "Validation",
            message: "禁止创建 0 元或负金额发票；无历史票款请使用「从 0 起」",
        })
    }

    const result = await apiPost<BackendCardFundsRegistrationResult>(
        "/admin/card-funds-review/invoices",
        {
            work_item_id: input.workItemId,
            expected_task_version: input.expectedTaskVersion,
            expected_subject_version: input.expectedSubjectVersion,
            expected_funds_fact_version: input.expectedFundsFactVersion,
            invoice_no: input.invoiceNo.trim() || undefined,
            invoice_date: input.issuedAt.slice(0, 10),
            gross_amount: input.grossAmount,
            net_amount: input.netAmount,
            tax_amount: input.taxAmount,
            allocations: input.allocations.map((line) => ({
                target_account_id: line.targetAccountId,
                amount: line.amount,
            })),
            evidence_reference: input.evidenceReference,
            idempotency_key: input.idempotencyKey,
        },
    )
    return projectRegistrationResult(result)
}
