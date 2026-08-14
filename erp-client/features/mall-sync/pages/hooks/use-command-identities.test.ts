import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useCommandIdentities } from "./use-command-identities"

describe("useCommandIdentities", () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it("returns a stable identity for the same kind and object id", () => {
        const { result } = renderHook(() => useCommandIdentities())
        const first = result.current.commandIdentity("single-order", "so-1")
        const second = result.current.commandIdentity("single-order", "so-1")
        expect(second).toEqual(first)
        expect(first.key).toBe("single-order:so-1")
        expect(first.idempotencyKey).toMatch(/^w17:single-order:so-1:/)
        expect(first.operationId).toMatch(/^w17:single-order:/)
        expect(first.operationId).not.toBe(first.idempotencyKey)
    })

    it("creates distinct identities for different keys", () => {
        const { result } = renderHook(() => useCommandIdentities())
        const a = result.current.commandIdentity("incremental", "manual")
        const b = result.current.commandIdentity("single-order", "manual")
        expect(a.idempotencyKey).not.toBe(b.idempotencyKey)
        expect(a.operationId).not.toBe(b.operationId)
        expect(a.key).not.toBe(b.key)
    })

    it("generates a fresh identity after the previous one is cleared", () => {
        const { result } = renderHook(() => useCommandIdentities())
        const first = result.current.commandIdentity("retry-job", "job-1")
        act(() => {
            result.current.clearIdentity(first.key)
        })
        const second = result.current.commandIdentity("retry-job", "job-1")
        expect(second.key).toBe(first.key)
        expect(second.idempotencyKey).not.toBe(first.idempotencyKey)
        expect(second.operationId).not.toBe(first.operationId)
    })

    it("keeps identities independent across renders", () => {
        const { result, rerender } = renderHook(() => useCommandIdentities())
        const first = result.current.commandIdentity("confirm-mapping", "mt-1")
        rerender()
        const second = result.current.commandIdentity("confirm-mapping", "mt-1")
        expect(second.idempotencyKey).toBe(first.idempotencyKey)
    })
})
