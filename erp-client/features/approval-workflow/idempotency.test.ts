import { describe, expect, it, vi } from "vitest"

import {
    createApprovalIdempotencyKey,
    decisionIntentFingerprint,
    slotForIntent,
} from "./idempotency"

describe("approval idempotency lifecycle", () => {
    it("keeps the same key while the decision intent stays unchanged", () => {
        vi.spyOn(crypto, "randomUUID")
            .mockReturnValueOnce("aaa")
            .mockReturnValueOnce("bbb")
        const first = slotForIntent(
            null,
            "decision",
            "wi-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const retry = slotForIntent(
            first,
            "decision",
            "wi-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        expect(retry.key).toBe(first.key)
        expect(first.key).toContain("approval:decision:wi-1:")
    })

    it("rotates the key after the user changes the decision or reason", () => {
        vi.spyOn(crypto, "randomUUID")
            .mockReturnValueOnce("aaa")
            .mockReturnValueOnce("bbb")
        const approved = slotForIntent(
            null,
            "decision",
            "wi-1",
            decisionIntentFingerprint("APPROVE", ""),
        )
        const rejected = slotForIntent(
            approved,
            "decision",
            "wi-1",
            decisionIntentFingerprint("REJECT", "资料不全"),
        )
        expect(rejected.key).not.toBe(approved.key)
        expect(createApprovalIdempotencyKey("resume", "inst-1")).toContain(
            "approval:resume:inst-1:",
        )
    })
})
