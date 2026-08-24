import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, renderHook } from "@testing-library/react"

import { usePurchaseOrderDetailEditGuard } from "./use-purchase-order-detail-edit-guard"
import { makePurchaseOrderCenter } from "./use-purchase-order-detail-fixtures"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import type { PurchaseOrderDetailLineEdits } from "./use-purchase-order-detail-edit-actions"

type GuardProps = {
    mode: "view" | "edit" | "review"
    order: PurchaseOrderCenterView | undefined
    paymentTermCode: string
    note: string
    lineEdits: PurchaseOrderDetailLineEdits
    onSave: () => Promise<boolean>
}

function makeGuardProps(overrides: Partial<GuardProps> = {}): GuardProps {
    const order = makePurchaseOrderCenter()
    return {
        mode: "edit",
        order,
        paymentTermCode: order.header.paymentTermCode,
        note: "",
        lineEdits: {
            "line-1": {
                quantity: order.currentContent.lines[0].quantity,
                unitCostGross: order.currentContent.lines[0].unitCostGross,
                inputTaxRate: order.currentContent.lines[0].inputTaxRate,
            },
        },
        onSave: async () => true,
        ...overrides,
    }
}

describe("usePurchaseOrderDetailEditGuard", () => {
    beforeEach(() => {
        vi.restoreAllMocks()
    })

    it("is not dirty in view mode", () => {
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ mode: "view" }) },
        )
        expect(result.current.editDirty).toBe(false)
    })

    it("is not dirty in edit mode when nothing changed", () => {
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps() },
        )
        expect(result.current.editDirty).toBe(false)
    })

    it("is dirty when the payment term differs", () => {
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ paymentTermCode: "PREPAY_100" }) },
        )
        expect(result.current.editDirty).toBe(true)
    })

    it("is dirty when a non-empty note is entered", () => {
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ note: "改一下" }) },
        )
        expect(result.current.editDirty).toBe(true)
    })

    it("is dirty when a line edit diverges from current content", () => {
        const order = makePurchaseOrderCenter()
        const { result } = renderHook(() =>
            usePurchaseOrderDetailEditGuard({
                mode: "edit",
                order,
                paymentTermCode: order.header.paymentTermCode,
                note: "",
                lineEdits: {
                    "line-1": {
                        quantity: "99",
                        unitCostGross:
                            order.currentContent.lines[0].unitCostGross,
                        inputTaxRate:
                            order.currentContent.lines[0].inputTaxRate,
                    },
                },
                onSave: async () => true,
            }),
        )
        expect(result.current.editDirty).toBe(true)
    })

    it("registers a beforeunload listener only while dirty in edit mode", () => {
        const addSpy = vi.spyOn(window, "addEventListener")
        const removeSpy = vi.spyOn(window, "removeEventListener")

        const { rerender } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps() },
        )
        expect(
            addSpy.mock.calls.filter(([name]) => name === "beforeunload"),
        ).toHaveLength(0)

        rerender(makeGuardProps({ paymentTermCode: "PREPAY_100" }))
        expect(
            addSpy.mock.calls.filter(([name]) => name === "beforeunload"),
        ).toHaveLength(1)

        rerender(makeGuardProps())
        expect(
            removeSpy.mock.calls.filter(([name]) => name === "beforeunload"),
        ).toHaveLength(1)

        addSpy.mockRestore()
        removeSpy.mockRestore()
    })

    it("leaves immediately when not dirty", () => {
        const go = vi.fn()
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ mode: "view" }) },
        )
        act(() => {
            result.current.requestLeave(go)
        })
        expect(go).toHaveBeenCalledTimes(1)
        expect(result.current.leaveGuardOpen).toBe(false)
    })

    it("opens the guard instead of leaving when dirty", () => {
        const go = vi.fn()
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ note: "未保存" }) },
        )
        act(() => {
            result.current.requestLeave(go)
        })
        expect(go).not.toHaveBeenCalled()
        expect(result.current.leaveGuardOpen).toBe(true)
    })

    it("saveAndLeave leaves only after a successful save", async () => {
        const go = vi.fn()
        const onSave = vi.fn(async () => false)
        const { result, rerender } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ note: "未保存", onSave }) },
        )
        act(() => {
            result.current.requestLeave(go)
        })
        rerender(makeGuardProps({ note: "未保存", onSave }))
        await act(async () => {
            await result.current.saveAndLeave()
        })
        expect(onSave).toHaveBeenCalledTimes(1)
        expect(go).not.toHaveBeenCalled()
        expect(result.current.leaveGuardOpen).toBe(true)

        const onSaveOk = vi.fn(async () => true)
        rerender(makeGuardProps({ note: "未保存", onSave: onSaveOk }))
        await act(async () => {
            await result.current.saveAndLeave()
        })
        expect(onSaveOk).toHaveBeenCalledTimes(1)
        expect(go).toHaveBeenCalledTimes(1)
        expect(result.current.leaveGuardOpen).toBe(false)
    })

    it("discardAndLeave closes the guard and leaves", () => {
        const go = vi.fn()
        const { result } = renderHook(
            (props: GuardProps) => usePurchaseOrderDetailEditGuard(props),
            { initialProps: makeGuardProps({ note: "未保存" }) },
        )
        act(() => {
            result.current.requestLeave(go)
        })
        act(() => {
            result.current.discardAndLeave()
        })
        expect(go).toHaveBeenCalledTimes(1)
        expect(result.current.leaveGuardOpen).toBe(false)
    })
})
