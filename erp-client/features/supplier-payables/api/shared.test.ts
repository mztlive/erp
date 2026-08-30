import { describe, expect, it } from "vitest"

import {
    beginFreshAllocationAttempt,
    nextSessionId,
} from "@/features/supplier-payables/api/shared"

describe("allocation session identity", () => {
    it("creates a fresh opaque identity without retaining prior business state", () => {
        const previousSessionId = "alloc_sup_previous"
        const next = beginFreshAllocationAttempt(
            previousSessionId,
            "payment-attempt-1",
        )

        expect(next).toMatch(/^alloc_sup_[0-9a-f-]+$/)
        expect(next).not.toBe(previousSessionId)
        expect(nextSessionId()).not.toBe(next)
    })
})
