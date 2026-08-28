import { describe, expect, test } from "vitest"

import { projectPayable, projectPayment } from "./mappers"

const paymentBase = {
    id: "pay-1",
    payment_no: "FK-1",
    status: "posted",
    supplier_id: "sup-1",
    paid_at: 1,
    amount: "10.00",
    version: 1,
    created_at: 1,
    allocated_total: "10.00",
    unallocated_amount: "0.00",
}

describe("projectPayment", () => {
    test("供应商名称缺失时不上屏供应商 ID", () => {
        const row = projectPayment({
            ...paymentBase,
            allocations: [],
        })
        expect(row.supplierName).toBe("供应商名称待补全")
        expect(row.supplierName).not.toBe("sup-1")
    })

    test("核销来源展示业务单号而不是分录 ID", () => {
        const row = projectPayment({
            ...paymentBase,
            supplier_name: "华东供应商",
            allocations: [
                {
                    id: "alloc-1",
                    allocation_seq: 1,
                    allocation_action: "apply",
                    payable_entry_id: "pe-1",
                    payable_account_id: "pa-1",
                    source_type: "purchase_order",
                    source_document_id: "po-1",
                    source_document_no: "PO-1001",
                    allocated_amount: "10.00",
                    allocated_at: 1,
                },
            ],
        })
        expect(row.allocations[0]?.sourceDocumentNo).toBe("PO-1001")
        expect(row.allocations[0]?.sourceHref).toBe("/procurement/orders/po-1")
        expect(row.allocations[0]?.payableHref).toContain("view=payable")
    })

    test("核销缺少来源单号时使用占位，不上屏分录 ID", () => {
        const row = projectPayment({
            ...paymentBase,
            allocations: [
                {
                    id: "alloc-1",
                    allocation_seq: 1,
                    allocation_action: "apply",
                    payable_entry_id: "pe-secret",
                    allocated_amount: "10.00",
                    allocated_at: 1,
                },
            ],
        })
        expect(row.allocations[0]?.sourceDocumentNo).toBe("采购单号待补全")
        expect(row.allocations[0]?.sourceDocumentNo).not.toContain("pe-secret")
    })
})

describe("projectPayable", () => {
    test("供应商与来源单号缺失时不上屏内部 ID", () => {
        const row = projectPayable({
            id: "pa-1",
            source_document_id: "po-secret",
            supplier_id: "sup-secret",
            source_type: "purchase_order",
            gross_total: "10.00",
            settled_total: "0.00",
            open_total: "10.00",
            invoiceable_total: "10.00",
            invoiced_total: "0.00",
            open_invoiceable_total: "10.00",
            status: "open",
            version: 1,
            created_at: 1,
            entries: [],
        })
        expect(row.supplierName).toBe("供应商名称待补全")
        expect(row.sourceDocumentNo).toBe("采购单号待补全")
        expect(row.sourceHref).toBe("/procurement/orders/po-secret")
    })
})
