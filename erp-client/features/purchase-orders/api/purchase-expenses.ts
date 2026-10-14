/** 费用采购单：从销售单追加创建，费用行确认发生（凭证随命令 multipart 提交）。 */

import { apiPost, apiPostForm } from "@/lib/api"
import type { PurchaseExpenseCategory } from "@/features/purchase-orders/types"
import type { BackendCreateResult } from "./purchase-order-wire-types"

export type CreateExpensePurchaseLineInput = {
    category: PurchaseExpenseCategory
    description: string
    grossAmount: string
    inputTaxRate: string
    salesOrderLineId?: string
}

export type CreateExpensePurchaseInput = {
    supplierId: string
    paymentTermCode: string
    lines: CreateExpensePurchaseLineInput[]
    idempotencyKey: string
}

export type ConfirmExpenseOccurrenceInput = {
    occurredOn: string
    evidence: File
}

/** 凭证在 multipart 中的临时引用名。 */
const EVIDENCE_REFERENCE = "pending-file:expense-occurrence-evidence"

export function createExpensePurchase(
    salesOrderId: string,
    input: CreateExpensePurchaseInput,
) {
    return apiPost<BackendCreateResult>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/expense-purchases`,
        {
            supplier_id: input.supplierId,
            payment_term_code: input.paymentTermCode,
            lines: input.lines.map((line) => ({
                category: line.category,
                description: line.description,
                gross_amount: line.grossAmount,
                input_tax_rate: line.inputTaxRate,
                sales_order_line_id: line.salesOrderLineId || null,
            })),
            idempotency_key: input.idempotencyKey,
        },
    )
}

export function confirmExpenseOccurrence(
    purchaseOrderId: string,
    expenseLineId: string,
    input: ConfirmExpenseOccurrenceInput,
) {
    const form = new FormData()
    form.append(
        "command",
        JSON.stringify({
            occurred_on: input.occurredOn,
            evidence_attachment_id: EVIDENCE_REFERENCE,
        }),
    )
    form.append(EVIDENCE_REFERENCE, input.evidence, input.evidence.name)
    return apiPostForm<{
        expense_line_id: string
        cost_entry_id: string
        replayed: boolean
    }>(
        `/admin/purchase-orders/${encodeURIComponent(purchaseOrderId)}/expenses/${encodeURIComponent(expenseLineId)}/occurrence`,
        form,
    )
}
