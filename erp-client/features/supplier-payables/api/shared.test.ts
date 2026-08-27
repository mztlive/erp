import { afterEach, describe, expect, it } from "vitest"

import {
    beginFreshAllocationAttempt,
    draftSnapshots,
    sessions,
    submitIdempotency,
    submitUnknownResolvers,
} from "@/features/supplier-payables/api/shared"

afterEach(() => {
    sessions.clear()
    draftSnapshots.clear()
    submitIdempotency.clear()
    submitUnknownResolvers.clear()
})

describe("beginFreshAllocationAttempt", () => {
    it("discards the previous partial-payment session and idempotency result", () => {
        const previousSessionId = "alloc_sup_previous"
        const previousKey = "payment-attempt-1"
        sessions.set(previousSessionId, {
            draftSessionId: previousSessionId,
            track: "payment",
            supplierId: "supplier-1",
            supplierName: "示例供应商",
            pool: [],
            payablePriorityPolicy: {
                state: "MISSING",
                mixedAutoAllocationAllowed: false,
            },
            preselectedPayableAccountIds: [],
            dataWatermark: "wm-1",
            queriedAt: "2026-08-27T00:00:00.000Z",
        })
        draftSnapshots.set(previousSessionId, { amount: "10.00" })
        submitIdempotency.set(previousKey, {
            status: "succeeded",
            title: "付款已登记",
            description: "第一笔分次付款已完成",
        })
        submitUnknownResolvers.set(previousKey, async () => ({
            status: "unknown",
            title: "结果待确认",
            description: "等待查询",
        }))

        const nextSessionId = beginFreshAllocationAttempt(
            previousSessionId,
            previousKey,
        )

        expect(nextSessionId).not.toBe(previousSessionId)
        expect(sessions.has(previousSessionId)).toBe(false)
        expect(draftSnapshots.has(previousSessionId)).toBe(false)
        expect(submitIdempotency.has(previousKey)).toBe(false)
        expect(submitUnknownResolvers.has(previousKey)).toBe(false)
    })
})
