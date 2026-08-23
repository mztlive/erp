import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook } from "@testing-library/react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"

import { usePurchaseOrderDetailPermissions } from "./use-purchase-order-detail-permissions"
import { makePurchaseOrderCenter } from "./use-purchase-order-detail-fixtures"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

type ReviewWorkItem = NonNullable<PurchaseOrderCenterView["reviewWorkItem"]>

function makeReviewWorkItem(): ReviewWorkItem {
    return {
        workItemId: "wi-1",
        workItemType: "PURCHASE_ORDER_REVIEW" as const,
        taskVersion: "v1",
        subjectVersion: "v3",
        status: "OPEN" as const,
        ownerRole: "FINANCE",
        ownerOrganizationId: "org-1",
        processingState: "READY" as const,
        domainAllowedActions: ["APPROVE", "REJECT"],
        actionBlockers: [],
    }
}

beforeEach(() => {
    vi.stubGlobal("crypto", {
        randomUUID: vi.fn(() => "uuid-1"),
    })
})

afterEach(() => {
    vi.unstubAllGlobals()
})

describe("usePurchaseOrderDetailPermissions", () => {
    it("derives false flags for a missing order", () => {
        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(
                undefined,
                new FormalCommandKeyLedger(),
            ),
        )
        expect(result.current).toEqual({
            canEdit: false,
            canSubmit: false,
            canOpenReview: false,
            canApprove: false,
            canReject: false,
            canChange: false,
            canFulfill: false,
            canPay: false,
            fulfillBlocker: undefined,
            changeBlocker: undefined,
        })
    })

    it("maps allowedActions to action flags", () => {
        const order = makePurchaseOrderCenter({
            allowedActions: [
                "EDIT",
                "SUBMIT",
                "FULFILL",
                "PAY",
                "START_CHANGE",
            ],
        })
        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(
                order,
                new FormalCommandKeyLedger(),
            ),
        )
        expect(result.current.canEdit).toBe(true)
        expect(result.current.canSubmit).toBe(true)
        expect(result.current.canFulfill).toBe(true)
        expect(result.current.canPay).toBe(true)
        expect(result.current.canChange).toBe(true)
        expect(result.current.canOpenReview).toBe(false)
    })

    it("derives review flags from a ready review work item", () => {
        const order = makePurchaseOrderCenter({
            allowedActions: [],
            reviewWorkItem: makeReviewWorkItem(),
        })
        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(
                order,
                new FormalCommandKeyLedger(),
            ),
        )
        expect(result.current.canOpenReview).toBe(true)
        expect(result.current.canApprove).toBe(true)
        expect(result.current.canReject).toBe(true)
    })

    it("does not offer review decisions while a review work item is not ready", () => {
        const order = makePurchaseOrderCenter({
            reviewWorkItem: {
                ...makeReviewWorkItem(),
                processingState: "APPROVAL_BLOCKED",
            },
        })
        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(
                order,
                new FormalCommandKeyLedger(),
            ),
        )
        expect(result.current.canApprove).toBe(false)
        expect(result.current.canReject).toBe(false)
    })

    it("blocks the opposite decision while an outcome is still unknown", () => {
        const order = makePurchaseOrderCenter({
            reviewWorkItem: makeReviewWorkItem(),
        })
        const ledger = new FormalCommandKeyLedger()
        ledger.acquire("review-approve", "w08:wi-1:v1:approve", { a: 1 })

        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(order, ledger),
        )
        expect(result.current.canApprove).toBe(true)
        expect(result.current.canReject).toBe(false)
    })

    it("blocks approval while a reject outcome is still unknown", () => {
        const order = makePurchaseOrderCenter({
            reviewWorkItem: makeReviewWorkItem(),
        })
        const ledger = new FormalCommandKeyLedger()
        ledger.acquire("review-reject", "w08:wi-1:v1:reject", { a: 1 })

        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(order, ledger),
        )
        expect(result.current.canReject).toBe(true)
        expect(result.current.canApprove).toBe(false)
    })

    it("surfaces action blockers for fulfill and change", () => {
        const order = makePurchaseOrderCenter({
            allowedActions: ["FULFILL"],
            actionBlockers: [
                {
                    action: "FULFILL",
                    code: "PREPAYMENT_GATE",
                    message: "先款未到",
                },
            ],
        })
        const { result } = renderHook(() =>
            usePurchaseOrderDetailPermissions(
                order,
                new FormalCommandKeyLedger(),
            ),
        )
        expect(result.current.fulfillBlocker?.code).toBe("PREPAYMENT_GATE")
        expect(result.current.changeBlocker).toBeUndefined()
    })
})
