import { renderHook } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { useSupplierOrderCenterCommandIdentity } from "./use-supplier-order-center-identity"

describe("useSupplierOrderCenterCommandIdentity", () => {
    it("reuses the same identity for the same kind and object", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterCommandIdentity(),
        )
        const first = result.current.commandIdentity("query", "o1")
        const second = result.current.commandIdentity("query", "o1")
        expect(second.key).toBe(first.key)
        expect(second.operationId).toBe(first.operationId)
        expect(second.idempotencyKey).toBe(first.idempotencyKey)
    })

    it("issues distinct identities for different kinds or objects", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterCommandIdentity(),
        )
        const query = result.current.commandIdentity("query", "o1")
        const replay = result.current.commandIdentity("replay", "o1")
        const otherOrder = result.current.commandIdentity("query", "o2")
        expect(replay.operationId).not.toBe(query.operationId)
        expect(replay.idempotencyKey).not.toBe(query.idempotencyKey)
        expect(otherOrder.idempotencyKey).not.toBe(query.idempotencyKey)
    })

    it("issues a fresh identity after the previous one is forgotten", () => {
        const { result } = renderHook(() =>
            useSupplierOrderCenterCommandIdentity(),
        )
        const first = result.current.commandIdentity("complete", "wi1")
        result.current.forgetCommandIdentity(first.key)
        const second = result.current.commandIdentity("complete", "wi1")
        expect(second.operationId).not.toBe(first.operationId)
    })
})
