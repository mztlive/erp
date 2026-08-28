import { describe, expect, test } from "vitest"

import type {
    BackendPaymentReversal,
    BackendSupplierPayment,
} from "@/features/supplier-payables/api/mappers"

import {
    paymentReversalDocumentFacts,
    shouldLoadDocumentFacts,
} from "./document-facts"

const reversal: BackendPaymentReversal = {
    id: "reversal-1",
    reversal_no: "PCZ-1",
    status: "IN_APPROVAL",
    original_supplier_payment_id: "payment-1",
    reason_text: "收款信息有误",
    amount: "880.00",
    handled_by: "付款",
    reviewed_by: "采购1",
    occurred_at: 1_788_000_000,
    version: 1,
    created_at: 1_788_000_000,
}

const payment: BackendSupplierPayment = {
    id: "payment-1",
    payment_no: "FK-1",
    status: "POSTED",
    supplier_id: "supplier-1",
    supplier_name: "华东供应商",
    paid_at: 1_787_000_000,
    amount: "1000.00",
    bank_reference: "****8899",
    version: 1,
    created_at: 1_787_000_000,
    allocated_total: "1000.00",
    unallocated_amount: "0.00",
    allocations: [
        {
            id: "allocation-1",
            allocation_seq: 1,
            allocation_action: "apply",
            payable_entry_id: "entry-1",
            payable_account_id: "account-1",
            source_type: "purchase_order",
            source_document_id: "purchase-1",
            source_document_no: "PO-1",
            allocated_amount: "1000.00",
            allocated_at: 1_787_000_000,
        },
    ],
}

describe("payment reversal workspace facts", () => {
    test("loads facts for both payment reversal object spellings", () => {
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "payment_reversal",
                hasSummary: false,
            }),
        ).toBe(true)
        expect(
            shouldLoadDocumentFacts({
                businessObjectType: "PaymentReversal",
                hasSummary: false,
            }),
        ).toBe(true)
    })

    test("projects the original payment, supplier, reason, amount and allocation", () => {
        const facts = paymentReversalDocumentFacts(reversal, payment)

        expect(facts.counterparty).toBe("华东供应商")
        expect(facts.impact).toContain("原付款保持不变")
        expect(facts.sections).toEqual(
            expect.arrayContaining([
                { label: "冲正金额", value: "¥880", numeric: true },
                {
                    label: "原付款单",
                    value: "FK-1",
                    objectId: "payment-1",
                },
                { label: "冲正原因", value: "收款信息有误" },
                { label: "供应商", value: "华东供应商" },
            ]),
        )
        expect(facts.lines).toEqual([{ title: "PO-1", quantity: "¥1,000" }])
    })
})
