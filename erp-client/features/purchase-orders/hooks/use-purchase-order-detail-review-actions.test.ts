import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act } from "@testing-library/react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import { responsibilityText } from "@/lib/ui-text"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

import { usePurchaseOrderDetailReviewActions } from "./use-purchase-order-detail-review-actions"
import { makePurchaseOrderCenter } from "./use-purchase-order-detail-fixtures"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

const apiMocks = vi.hoisted(() => ({
    acquireDraftEditToken: vi.fn(),
    createPurchaseOrderFromBasis: vi.fn(),
    fetchCreationBases: vi.fn(),
    fetchPurchaseOrderCenter: vi.fn(),
    fetchPurchaseOrderExportData: vi.fn(),
    fetchPurchaseOrders: vi.fn(),
    reviewPurchaseOrder: vi.fn(),
    savePurchaseOrderDraft: vi.fn(),
    startPurchaseChange: vi.fn(),
    submitPurchaseOrderForReview: vi.fn(),
}))

vi.mock("@/features/purchase-orders/api/purchase-orders", () => ({
    acquireDraftEditToken: apiMocks.acquireDraftEditToken,
    createPurchaseOrderFromBasis: apiMocks.createPurchaseOrderFromBasis,
    fetchCreationBases: apiMocks.fetchCreationBases,
    fetchPurchaseOrderCenter: apiMocks.fetchPurchaseOrderCenter,
    fetchPurchaseOrderExportData: apiMocks.fetchPurchaseOrderExportData,
    fetchPurchaseOrders: apiMocks.fetchPurchaseOrders,
    reviewPurchaseOrder: apiMocks.reviewPurchaseOrder,
    savePurchaseOrderDraft: apiMocks.savePurchaseOrderDraft,
    startPurchaseChange: apiMocks.startPurchaseChange,
    submitPurchaseOrderForReview: apiMocks.submitPurchaseOrderForReview,
}))

const responsibilityMocks = vi.hoisted(() => ({
    mutateAsync: vi.fn(),
}))

vi.mock("@/features/work-items", () => ({
    useWorkItemResponsibilityMutation: () => ({
        mutateAsync: responsibilityMocks.mutateAsync,
        isPending: false,
    }),
}))

const navMocks = vi.hoisted(() => ({
    push: vi.fn(),
    replace: vi.fn(),
    back: vi.fn(),
}))

vi.mock("next/navigation", () => ({
    useRouter: () => ({
        push: navMocks.push,
        replace: navMocks.replace,
        back: navMocks.back,
    }),
}))

type ReviewActionsProps = Parameters<
    typeof usePurchaseOrderDetailReviewActions
>[0]

function makeReviewOrder(): PurchaseOrderCenterView {
    return makePurchaseOrderCenter({
        identity: {
            purchaseOrderId: "po-1",
            purchaseNo: "PO-2026-001",
            status: "PENDING_REVIEW",
            statusLabel: "待财务审核",
            statusTone: "warning",
            reviewStatus: "PENDING",
            reviewLabel: "待审核",
            lockVersion: 3,
            currentSubmissionId: "sub-1",
            revisionNo: 1,
        },
        header: {
            salesOrderId: "so-1",
            salesOrderNo: "SO-001",
            supplierId: "sup-1",
            supplierSnapshot: "示例供应商有限公司",
            purchaseType: "PHYSICAL",
            fulfillmentResponsibility: "WAREHOUSE",
            paymentTermCode: "POSTPAY_NET15",
            paymentTermLabel: "货到 15 天",
            ownerName: "经办人",
        },
        reviewWorkItem: {
            workItemId: "wi-1",
            workItemType: "PURCHASE_ORDER_REVIEW",
            taskVersion: "v1",
            subjectVersion: "v3",
            status: "OPEN",
            assignmentMode: "DIRECT",
            ownerRole: "FINANCE",
            ownerOrganizationId: "org-1",
            processingState: "READY",
            responsibilityActions: ["START_PROCESSING", "RELEASE_TO_TEAM"],
            domainAllowedActions: ["APPROVE", "REJECT"],
            actionBlockers: [],
        },
    })
}

function makeProps(
    overrides: Partial<ReviewActionsProps> = {},
): ReviewActionsProps {
    return {
        purchaseOrderId: "po-1",
        order: makeReviewOrder(),
        refetch: vi.fn(async () => ({ data: undefined })),
        commandLedger: new FormalCommandKeyLedger(),
        setResult: vi.fn(),
        ...overrides,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
    vi.stubGlobal("crypto", {
        randomUUID: vi.fn(() => "uuid-1"),
    })
})

afterEach(() => {
    vi.unstubAllGlobals()
})

describe("usePurchaseOrderDetailReviewActions", () => {
    it("approves the review, refreshes and returns to the base url", async () => {
        apiMocks.reviewPurchaseOrder.mockResolvedValue({
            status: "succeeded",
            data: { revisionNo: 2, payableOpenAmount: "1130.00" },
            reference: "REF-APPROVE",
        })
        const order = makeReviewOrder()
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({ data: order }))
        let props: ReviewActionsProps = makeProps({ order, setResult, refetch })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ order, setResult, refetch })
        rerender()
        act(() => {
            result.current.setApproveConfirmOpen(true)
        })

        await act(async () => {
            await result.current.handleApprove()
        })

        expect(apiMocks.reviewPurchaseOrder.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                workItemId: "wi-1",
                expectedTaskVersion: "v1",
                expectedSubjectVersion: "v3",
                decision: expect.objectContaining({
                    purchaseOrderId: "po-1",
                    submissionId: "sub-1",
                    reviewResult: "APPROVED",
                }),
                idempotencyKey: expect.any(String),
            }),
        )
        expect(result.current.approveConfirmOpen).toBe(false)
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/procurement/orders/po-1",
        )
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "财务审核已通过",
                reference: "REF-APPROVE",
            }),
        )
    })

    it("refuses to approve while the reject outcome is still unknown", async () => {
        const order = makeReviewOrder()
        const ledger = new FormalCommandKeyLedger()
        ledger.acquire("review-reject", "w08:wi-1:v1:reject", { a: 1 })
        const setResult = vi.fn()
        let props: ReviewActionsProps = makeProps({
            order,
            setResult,
            commandLedger: ledger,
        })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await expect(
            act(async () => {
                await result.current.handleApprove()
            }),
        ).rejects.toThrow("原驳回操作的结果仍待确认")
        expect(apiMocks.reviewPurchaseOrder).not.toHaveBeenCalled()
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "unknown",
                title: "审核结果待确认",
            }),
        )
    })

    it("does nothing without a review work item or submission", async () => {
        const order = makePurchaseOrderCenter({
            identity: {
                purchaseOrderId: "po-1",
                status: "DRAFT",
                statusLabel: "草稿",
                statusTone: "neutral",
                reviewStatus: "NONE",
                reviewLabel: "—",
                lockVersion: 3,
            },
        })
        let props: ReviewActionsProps = makeProps({ order })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.handleApprove()
        })
        await act(async () => {
            await result.current.handleReject("COST_TAX", "税率有误")
        })

        expect(apiMocks.reviewPurchaseOrder).not.toHaveBeenCalled()
    })

    it("rejects with reason and navigates back to edit mode", async () => {
        apiMocks.reviewPurchaseOrder.mockResolvedValue({
            status: "succeeded",
            data: { revisionNo: 1 },
            reference: "REF-REJECT",
        })
        const order = makeReviewOrder()
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({ data: order }))
        let props: ReviewActionsProps = makeProps({ order, setResult, refetch })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ order, setResult, refetch })
        rerender()

        await act(async () => {
            await result.current.handleReject("COST_TAX", "税率有误")
        })

        expect(apiMocks.reviewPurchaseOrder.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                decision: expect.objectContaining({
                    reviewResult: "REJECTED",
                    reasonCode: "COST_TAX",
                    comment: "税率有误",
                }),
            }),
        )
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/procurement/orders/po-1?mode=edit",
        )
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "rejected",
                title: "财务已驳回",
                facts: expect.arrayContaining([
                    { label: "原因", value: "成本/税率不符" },
                    { label: "说明", value: "税率有误" },
                ]),
            }),
        )
    })

    it("refuses to reject while the approve outcome is still unknown", async () => {
        const order = makeReviewOrder()
        const ledger = new FormalCommandKeyLedger()
        ledger.acquire("review-approve", "w08:wi-1:v1:approve", { a: 1 })
        let props: ReviewActionsProps = makeProps({
            order,
            commandLedger: ledger,
        })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await expect(
            act(async () => {
                await result.current.handleReject("COST_TAX", "税率有误")
            }),
        ).rejects.toThrow("原通过操作的结果仍待确认")
        expect(apiMocks.reviewPurchaseOrder).not.toHaveBeenCalled()
    })

    it("starts processing and refreshes", async () => {
        responsibilityMocks.mutateAsync.mockResolvedValue(undefined)
        const order = makeReviewOrder()
        const refetch = vi.fn(async () => ({ data: order }))
        const ledger = new FormalCommandKeyLedger()
        let props: ReviewActionsProps = makeProps({
            order,
            refetch,
            commandLedger: ledger,
        })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.handleStartProcessing()
        })

        expect(responsibilityMocks.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: "START_PROCESSING",
                workItemId: "wi-1",
                expectedTaskVersion: "v1",
                idempotencyKey: expect.any(String),
            }),
        )
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(ledger.peek("review-responsibility")).toBeUndefined()
    })

    it("reports a blocked result when starting processing fails definitively", async () => {
        responsibilityMocks.mutateAsync.mockRejectedValue(
            Object.assign(new Error("处理权已变化"), {
                kind: "Http",
                status: 409,
            }),
        )
        const order = makeReviewOrder()
        const setResult = vi.fn()
        const ledger = new FormalCommandKeyLedger()
        let props: ReviewActionsProps = makeProps({
            order,
            setResult,
            commandLedger: ledger,
        })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.handleStartProcessing()
        })

        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: responsibilityText.changed,
            }),
        )
        expect(ledger.peek("review-responsibility")).toBeUndefined()
    })

    it("keeps the responsibility command when the outcome is unknown", async () => {
        responsibilityMocks.mutateAsync.mockRejectedValue(
            Object.assign(new Error("网络中断"), { kind: "Network" }),
        )
        const order = makeReviewOrder()
        const setResult = vi.fn()
        const ledger = new FormalCommandKeyLedger()
        let props: ReviewActionsProps = makeProps({
            order,
            setResult,
            commandLedger: ledger,
        })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.handleStartProcessing()
        })

        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "unknown",
                title: responsibilityText.changed,
                reference: "PO-2026-001",
            }),
        )
        expect(ledger.peek("review-responsibility")).toBeDefined()
    })

    it("releases to the team with a reason and clears the dialog", async () => {
        responsibilityMocks.mutateAsync.mockResolvedValue(undefined)
        const order = makeReviewOrder()
        const setResult = vi.fn()
        let props: ReviewActionsProps = makeProps({ order, setResult })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )
        act(() => {
            result.current.setReleaseConfirmOpen(true)
            result.current.setReleaseReason("原因说明")
        })

        await act(async () => {
            await result.current.handleReleaseToTeam()
        })

        expect(responsibilityMocks.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                kind: "RELEASE_TO_TEAM",
                workItemId: "wi-1",
                reason: "原因说明",
            }),
        )
        expect(result.current.releaseConfirmOpen).toBe(false)
        expect(result.current.releaseReason).toBe("")
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: responsibilityText.releaseToTeam,
            }),
        )
    })

    it("does not release without a reason", async () => {
        const order = makeReviewOrder()
        let props: ReviewActionsProps = makeProps({ order })
        const { result } = renderHookWithProviders(
            () => usePurchaseOrderDetailReviewActions(props),
            { queryClient: createFreshQueryClient() },
        )

        await act(async () => {
            await result.current.handleReleaseToTeam()
        })

        expect(responsibilityMocks.mutateAsync).not.toHaveBeenCalled()
    })
})
