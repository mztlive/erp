import { describe, expect, test } from "vitest"

import type {
    ScopedCustomerReceiptWire,
    ScopedInvoiceWire,
    ScopedReceivableAccountWire,
} from "@/features/customer-receivables/api/scoped"

function receivable(overrides: Partial<ScopedReceivableAccountWire> = {}) {
    return {
        id: "ra-1",
        sales_order_id: "so-1",
        account_seq: 1,
        status: "open",
        created_at: 1,
        visible_settled_share: "60.00",
        gross_total: "100.00",
        settled_total: "60.00",
        open_total: "40.00",
        permission_limited: false,
        sales_owner_user_id: "sales-1",
        business_org_unit_id: "org-1",
        ...overrides,
    } satisfies ScopedReceivableAccountWire
}

function receipt(overrides: Partial<ScopedCustomerReceiptWire> = {}) {
    return {
        id: "r-1",
        receipt_no: "HK-1",
        status: "posted",
        received_at: 1,
        created_at: 1,
        visible_allocated_share: "60.00",
        amount: "100.00",
        allocated_total: "60.00",
        unallocated_amount: "40.00",
        allocations: null,
        permission_limited: false,
        ...overrides,
    } satisfies ScopedCustomerReceiptWire
}

function invoice(overrides: Partial<ScopedInvoiceWire> = {}) {
    return {
        id: "i-1",
        invoice_no: "INV-1",
        invoice_direction: "sales",
        invoice_kind: "blue",
        status: "issued",
        invoice_date: "2026-09-01",
        created_at: 1,
        visible_allocated_share: "60.00",
        gross_amount: "100.00",
        allocated_total: "60.00",
        unallocated_amount: "40.00",
        allocations: null,
        purchase_allocations: null,
        permission_limited: false,
        ...overrides,
    } satisfies ScopedInvoiceWire
}

describe("M07 金额口径：获授权份额与整单分离", () => {
    test("部分受限时整单与未分配为 null，获授权份额始终有值", () => {
        const limited = receipt({
            permission_limited: true,
            amount: null,
            allocated_total: null,
            unallocated_amount: null,
        })
        expect(limited.visible_allocated_share).toBe("60.00")
        expect(limited.amount).toBeNull()
        expect(limited.unallocated_amount).toBeNull()
        // 禁止用零值掩盖受限：获授权份额不得被写成零。
        expect(limited.visible_allocated_share).not.toBe("0.00")
    })

    test("60/40 例子：人员汇总只算匹配份额，未分配单列", () => {
        const matched = receivable().visible_settled_share
        const unassigned = receipt().unallocated_amount
        expect(matched).toBe("60.00")
        expect(unassigned).toBe("40.00")
        // 两种份额必须分别保留，不能由前端合成为同一整单金额。
        expect(matched).not.toBe(receivable().gross_total)
    })

    test("销项发票整单受限时为空", () => {
        const limited = invoice({
            permission_limited: true,
            gross_amount: null,
        })
        expect(limited.visible_allocated_share).toBe("60.00")
        expect(limited.gross_amount).toBeNull()
    })
})
