import { describe, expect, it } from "vitest"

import { allocationSessionMatchesIdentity } from "@/features/supplier-payables/lib/allocation-session-identity"
import type { AllocationSessionView } from "@/features/supplier-payables/types"

const session = {
    draftSessionId: "draft-a",
    track: "payment",
    supplierId: "supplier-a",
    supplierName: "供应商 A",
    pool: [],
    payablePriorityPolicy: {
        state: "MISSING",
        mixedAutoAllocationAllowed: false,
        blockerMessage: "请显式选择",
    },
    preselectedPayableAccountIds: ["payable-a"],
    purchaseOrderId: "purchase-a",
    dataWatermark: "wm-a",
    queriedAt: "2026-08-27T19:00:00.000Z",
} satisfies AllocationSessionView

describe("allocationSessionMatchesIdentity", () => {
    const identity = {
        track: "payment" as const,
        supplierId: "supplier-a",
        purchaseOrderId: "purchase-a",
        preselectPayableAccountId: "payable-a",
    }

    it("accepts a draft belonging to the current payable task", () => {
        expect(allocationSessionMatchesIdentity(session, identity)).toBe(true)
    })

    it.each([
        ["supplier", { ...identity, supplierId: "supplier-b" }],
        ["purchase order", { ...identity, purchaseOrderId: "purchase-b" }],
        [
            "payable account",
            { ...identity, preselectPayableAccountId: "payable-b" },
        ],
    ])("rejects a draft from another %s", (_name, otherIdentity) => {
        expect(allocationSessionMatchesIdentity(session, otherIdentity)).toBe(
            false,
        )
    })

    it("does not reuse a task-scoped draft in an unscoped session", () => {
        expect(
            allocationSessionMatchesIdentity(session, {
                track: "payment",
                supplierId: "supplier-a",
                purchaseOrderId: "purchase-a",
            }),
        ).toBe(false)
    })

    it("keeps the originating task identity after the session creates a payment", () => {
        expect(
            allocationSessionMatchesIdentity(
                { ...session, existingPaymentId: "payment-a" },
                identity,
            ),
        ).toBe(true)
    })

    it("rejects a draft opened for another existing payment", () => {
        expect(
            allocationSessionMatchesIdentity(
                { ...session, existingPaymentId: "payment-a" },
                { ...identity, existingPaymentId: "payment-b" },
            ),
        ).toBe(false)
    })
})
