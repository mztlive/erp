import { describe, expect, test } from "vitest"

import { buildPayableActivity } from "./payable-preview-activity"
import type {
    PaymentAllocationLine,
    PayableDetailView,
} from "@/features/supplier-payables/types"

const paymentAlloc = (
    patch: Partial<PaymentAllocationLine> &
        Pick<PaymentAllocationLine, "allocationId" | "amount" | "occurredAt">,
): PaymentAllocationLine => ({
    action: "APPLY",
    payableAccountId: "pa-1",
    payableEntryId: "pe-1",
    sourceType: "PURCHASE_ORDER",
    sourceDocumentNo: "PO-1001",
    ...patch,
})

describe("buildPayableActivity", () => {
    test("按发生时间倒序，空时间排在末尾", () => {
        const items = buildPayableActivity({
            paymentAllocations: [
                paymentAlloc({
                    allocationId: "pay-old",
                    amount: "5.00",
                    occurredAt: "2026-03-01T00:00:00.000Z",
                    sourceDocumentNo: "FK-1",
                }),
                paymentAlloc({
                    allocationId: "pay-new",
                    amount: "3.00",
                    occurredAt: "2026-03-18T00:00:00.000Z",
                    sourceDocumentNo: "FK-2",
                }),
            ],
            invoiceAllocations: [
                {
                    allocationId: "inv-empty",
                    action: "APPLY",
                    payableAccountId: "pa-1",
                    sourceType: "PURCHASE_ORDER",
                    sourceDocumentNo: "进项-1",
                    amountGross: "4.00",
                    occurredAt: "",
                },
            ],
        } satisfies Pick<
            PayableDetailView,
            "paymentAllocations" | "invoiceAllocations"
        >)

        expect(items.map((item) => item.id)).toEqual([
            "payment:pay-new",
            "payment:pay-old",
            "invoice:inv-empty",
        ])
        expect(items[0]?.trackLabel).toBe("付款")
        expect(items[2]?.trackLabel).toBe("进项发票")
    })
})
