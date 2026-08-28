import { describe, expect, test } from "vitest"

import type { PaymentAllocationLine } from "@/features/supplier-payables/types"
import {
    paymentRelatedDocumentRefs,
    sourceDocumentOpenLabel,
} from "./related-documents"

function allocation(
    overrides: Partial<PaymentAllocationLine> = {},
): PaymentAllocationLine {
    return {
        allocationId: "alloc-1",
        action: "APPLY",
        payableAccountId: "pa-1",
        payableEntryId: "pe-1",
        sourceType: "PURCHASE_ORDER",
        sourceDocumentId: "po-1",
        sourceDocumentNo: "PO-1001",
        sourceHref: "/procurement/orders/po-1",
        payableHref:
            "/finance/supplier-accounts?view=payable&detailId=pa-1&previewKind=payable",
        amount: "10.00",
        occurredAt: "2026-01-01T00:00:00.000Z",
        ...overrides,
    }
}

describe("paymentRelatedDocumentRefs", () => {
    test("同一应付只生成一行，采购单作为来源动作而不是重复单据", () => {
        const refs = paymentRelatedDocumentRefs({
            allocations: [allocation()],
            amount: "10.00",
        })

        expect(refs).toHaveLength(1)
        expect(refs[0]).toMatchObject({
            kind: "payable",
            documentType: "应付台账",
            documentNumber: "PO-1001",
            payableAccountId: "pa-1",
            sourceHref: "/procurement/orders/po-1",
            sourceType: "PURCHASE_ORDER",
        })
        expect(refs.some((item) => item.documentType === "采购单")).toBe(false)
    })

    test("多笔核销到同一应付时仍只保留一行", () => {
        const refs = paymentRelatedDocumentRefs({
            allocations: [
                allocation(),
                allocation({
                    allocationId: "alloc-2",
                    action: "REVERSE",
                    amount: "2.00",
                }),
            ],
            amount: "10.00",
        })

        expect(refs).toHaveLength(1)
        expect(refs[0]?.payableAccountId).toBe("pa-1")
    })

    test("冲正付款附带原付款单", () => {
        const refs = paymentRelatedDocumentRefs({
            allocations: [],
            amount: "10.00",
            reverseOfPaymentId: "pay-origin",
        })

        expect(refs).toEqual([
            expect.objectContaining({
                kind: "original-payment",
                documentType: "原付款单",
                documentNumber: "查看原付款",
            }),
        ])
    })
})

describe("sourceDocumentOpenLabel", () => {
    test("按来源类型区分打开文案", () => {
        expect(sourceDocumentOpenLabel("PURCHASE_ORDER")).toBe("打开采购单")
        expect(sourceDocumentOpenLabel("SUPPLIER_SETTLEMENT")).toBe(
            "打开结算单",
        )
    })
})
