import { describe, expect, it } from "vitest"

import { createCommandIdentityStore } from "./command-identity"

describe("createCommandIdentityStore", () => {
    it("generates unique idempotency and operation keys per command", () => {
        const store = createCommandIdentityStore()
        const first = store.get("RESOLVE", "task-1")
        const second = store.get("RESOLVE", "task-2")

        expect(first.key).toBe("RESOLVE:task-1")
        expect(first.idempotencyKey).toMatch(/^w29:RESOLVE:task-1:/)
        expect(first.operationId).toMatch(/^w29:RESOLVE:/)
        expect(second.idempotencyKey).not.toBe(first.idempotencyKey)
        expect(second.operationId).not.toBe(first.operationId)
    })

    it("reuses the identity for the same kind and object", () => {
        const store = createCommandIdentityStore()
        const first = store.get("RESOLVE", "wi_1")
        const again = store.get("RESOLVE", "wi_1")
        expect(again).toEqual(first)
    })

    it("forgets identities that are deleted", () => {
        const store = createCommandIdentityStore()
        const first = store.get("CLOSE_DUPLICATE", "wi_1")
        store.delete(first.key)
        const after = store.get("CLOSE_DUPLICATE", "wi_1")
        expect(after.idempotencyKey).not.toBe(first.idempotencyKey)
    })
})
