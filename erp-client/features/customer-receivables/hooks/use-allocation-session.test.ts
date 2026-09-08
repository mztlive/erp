import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { useAllocationSession } from "./use-allocation-session"
import type { AllocationSessionView } from "../types"
const mutate = vi.hoisted(() => vi.fn())
vi.mock("./queries", () => ({
    useSaveAllocationDraftMutation: () => ({
        mutateAsync: mutate,
        isPending: false,
    }),
    usePostAllocationMutation: () => ({
        mutateAsync: mutate,
        isPending: false,
    }),
    useResolvePostUnknownMutation: () => ({
        mutateAsync: mutate,
        isPending: false,
    }),
}))
const session: AllocationSessionView = {
    draftSessionId: "draft-1",
    mode: "invoice",
    counterpartyPartyId: "party-1",
    counterpartyPartyName: "客户",
    customerId: "customer-1",
    customerName: "客户",
    status: "draft",
    fact: { grossAmount: "200.00" },
    pool: [],
    allocations: [1, 2, 3].map((i) => ({
        baselineVersion: 1,
        lineKey: `line-${i}`,
        targetId: `target-${i}`,
        targetKind: "receivable_account",
        label: `应收-${i}`,
        amount: `${i}0.00`,
        openAmount: "100.00",
        salesOrderId: `order-${i}`,
        salesOrderNo: `XS-${i}`,
        counterpartyPartyId: "party-1",
    })),
    proposedAllocatedTotal: "60.00",
    proposedUnallocated: "140.00",
    factAmount: "200.00",
    submitPolicy: { allowUnallocatedRemainder: true, label: "可保留未分配" },
    leaseValid: true,
    editVersion: 1,
    note: "",
}

describe("local allocation removal", () => {
    it("restores the exact amount and row position without a business write", () => {
        const { result } = renderHook(() =>
            useAllocationSession({
                session,
                onClose: vi.fn(),
                onPosted: vi.fn(),
            }),
        )
        act(() => result.current.updateAmount("line-2", "23.4567"))
        const before = structuredClone(result.current.allocations)
        act(() => result.current.removeLine("line-2"))
        expect(result.current.allocations.map((line) => line.lineKey)).toEqual([
            "line-1",
            "line-3",
        ])
        act(() => result.current.undoRemoveLine())
        expect(result.current.allocations).toEqual(before)
        expect(mutate).not.toHaveBeenCalled()
    })
    it("does not remove rows when the operator has no permission", () => {
        const { result } = renderHook(() =>
            useAllocationSession({
                session,
                canOperate: false,
                onClose: vi.fn(),
                onPosted: vi.fn(),
            }),
        )
        act(() => result.current.removeLine("line-2"))
        expect(result.current.allocations).toEqual(session.allocations)
    })
})
