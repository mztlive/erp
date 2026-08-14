import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { usePurchaseOrderDetailCommandState } from "./use-purchase-order-detail-command-state"

beforeEach(() => {
    vi.stubGlobal("crypto", {
        randomUUID: vi.fn(() => "uuid-1"),
    })
})

afterEach(() => {
    vi.unstubAllGlobals()
})

describe("usePurchaseOrderDetailCommandState", () => {
    it("starts with no result and a ledger able to mint command identities", () => {
        const { result } = renderHook(() =>
            usePurchaseOrderDetailCommandState("po-1"),
        )
        expect(result.current.result).toBeNull()

        const command = result.current.commandLedger.acquire(
            "save-draft",
            "purchase:po-1:save",
            { purchaseOrderId: "po-1" },
        )
        expect(command.idempotencyKey).toBe("purchase:po-1:save:uuid-1")
    })

    it("updates the result through setResult", () => {
        const { result } = renderHook(() =>
            usePurchaseOrderDetailCommandState("po-1"),
        )
        act(() => {
            result.current.setResult({
                status: "succeeded",
                title: "草稿已保存",
                description: "ok",
            })
        })
        expect(result.current.result).toEqual({
            status: "succeeded",
            title: "草稿已保存",
            description: "ok",
        })
        act(() => {
            result.current.setResult(null)
        })
        expect(result.current.result).toBeNull()
    })

    it("keeps the ledger for the same purchase order and replaces it when the id changes", () => {
        const { result, rerender } = renderHook(
            ({ id }: { id: string }) => usePurchaseOrderDetailCommandState(id),
            { initialProps: { id: "po-1" } },
        )
        const firstLedger = result.current.commandLedger
        rerender({ id: "po-1" })
        expect(result.current.commandLedger).toBe(firstLedger)

        rerender({ id: "po-2" })
        expect(result.current.commandLedger).not.toBe(firstLedger)
        expect(result.current.commandLedger.peek("save-draft")).toBeUndefined()
    })

    it("drops settled command identities only on confirmed outcomes", () => {
        const { result } = renderHook(() =>
            usePurchaseOrderDetailCommandState("po-1"),
        )
        const ledger = result.current.commandLedger
        ledger.acquire("submit", "purchase:po-1:submit", { a: 1 })
        ledger.settle("submit", "unknown")
        expect(ledger.peek("submit")).toBeDefined()

        ledger.settle("submit", "succeeded")
        expect(ledger.peek("submit")).toBeUndefined()
    })
})
