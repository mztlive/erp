import { describe, expect, test } from "vitest"

import type {
    ScopedPayableAccountWire,
    ScopedPurchaseInvoiceAllocationWire,
    ScopedSupplierPaymentWire,
} from "@/features/supplier-payables/api/scoped"

function payable(overrides: Partial<ScopedPayableAccountWire> = {}) {
    return {
        id: "pa-1",
        source_document_id: "po-1",
        source_type: "purchase_order",
        supplier_id: "sup-1",
        status: "open",
        created_at: 1,
        visible_settled_share: "40.00",
        gross_total: "100.00",
        settled_total: "40.00",
        open_total: "60.00",
        permission_limited: false,
        procurement_owner_user_id: "buyer-1",
        business_org_unit_id: "org-1",
        ...overrides,
    } satisfies ScopedPayableAccountWire
}

function payment(overrides: Partial<ScopedSupplierPaymentWire> = {}) {
    return {
        id: "pay-1",
        payment_no: "FK-1",
        status: "posted",
        supplier_id: "sup-1",
        paid_at: 1,
        created_at: 1,
        visible_allocated_share: "40.00",
        amount: "100.00",
        allocated_total: "40.00",
        unallocated_amount: "60.00",
        allocations: null,
        permission_limited: false,
        ...overrides,
    } satisfies ScopedSupplierPaymentWire
}

function allocation(
    overrides: Partial<ScopedPurchaseInvoiceAllocationWire> = {},
) {
    return {
        id: "al-1",
        invoice_id: "inv-1",
        invoice_no: "PINV-1",
        payable_account_id: "pa-1",
        created_at: 1,
        visible_allocated_amount: "40.00",
        allocated_gross_amount: "40.00",
        permission_limited: false,
        ...overrides,
    } satisfies ScopedPurchaseInvoiceAllocationWire
}

describe("M09 金额口径：获授权份额与整单分离", () => {
    test("部分受限时整单与未分配为 null，获授权份额始终有值", () => {
        const limited = payment({
            permission_limited: true,
            amount: null,
            allocated_total: null,
            unallocated_amount: null,
        })
        expect(limited.visible_allocated_share).toBe("40.00")
        expect(limited.amount).toBeNull()
        expect(limited.unallocated_amount).toBeNull()
    })

    test("应付子账整单受限时为空", () => {
        const limited = payable({
            permission_limited: true,
            gross_total: null,
            open_total: null,
        })
        expect(limited.visible_settled_share).toBe("40.00")
        expect(limited.gross_total).toBeNull()
    })

    test("进项发票分配整单受限时为空", () => {
        const limited = allocation({
            permission_limited: true,
            allocated_gross_amount: null,
        })
        expect(limited.visible_allocated_amount).toBe("40.00")
        expect(limited.allocated_gross_amount).toBeNull()
    })
})
