import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import {
    createFreshQueryClient,
    renderHookWithProviders,
} from "@/features/test-utils"

import { usePurchaseOrderDetailEditActions } from "./use-purchase-order-detail-edit-actions"
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
    submitPurchaseChange: vi.fn(),
    submitPurchaseOrderForReview: vi.fn(),
    voidPurchaseOrderDraft: vi.fn(),
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
    submitPurchaseChange: apiMocks.submitPurchaseChange,
    submitPurchaseOrderForReview: apiMocks.submitPurchaseOrderForReview,
    voidPurchaseOrderDraft: apiMocks.voidPurchaseOrderDraft,
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

type EditActionsProps = Parameters<typeof usePurchaseOrderDetailEditActions>[0]

function makeProps(
    overrides: Partial<EditActionsProps> = {},
): EditActionsProps {
    return {
        purchaseOrderId: "po-1",
        mode: "view",
        order: undefined,
        refetch: vi.fn(async () => ({ data: undefined })),
        commandLedger: new FormalCommandKeyLedger(),
        setResult: vi.fn(),
        getPaymentTermCode: () => "POSTPAY_NET15",
        setDraftPaymentTermCode: vi.fn(),
        ...overrides,
    }
}

const succeededSave = {
    status: "succeeded" as const,
    data: {
        lockVersion: 4,
        draftContentHash: "hash-4",
        totals: { gross: "1130.00", net: "1000.00", tax: "130.00" },
    },
    reference: "REF-SAVE",
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

describe("usePurchaseOrderDetailEditActions", () => {
    it("acquires a draft token and initializes edits when entering edit mode", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        const order = makePurchaseOrderCenter()
        const setDraftPaymentTermCode = vi.fn()
        let props: EditActionsProps = makeProps({ setDraftPaymentTermCode })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )

        props = makeProps({ mode: "edit", order, setDraftPaymentTermCode })
        rerender()

        await waitFor(() =>
            expect(apiMocks.acquireDraftEditToken.mock.calls[0]?.[0]).toBe(
                "po-1",
            ),
        )
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))
        expect(result.current.lineEdits["line-1"]).toEqual({
            quantity: "10",
            unitCostGross: "100.00",
            inputTaxRate: "0.13",
        })
        expect(setDraftPaymentTermCode).toHaveBeenCalledWith("POSTPAY_NET15")
    })

    it("surfaces a blocked result when the draft token cannot be acquired", async () => {
        apiMocks.acquireDraftEditToken.mockRejectedValue(
            new Error("处理权已变化"),
        )
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        let props: EditActionsProps = makeProps({ setResult })
        const { rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )

        props = makeProps({ mode: "edit", order, setResult })
        rerender()

        await waitFor(() => expect(setResult).toHaveBeenCalledTimes(1))
        expect(setResult).toHaveBeenCalledWith({
            status: "blocked",
            title: "无法进入编辑",
            description: "处理权已变化",
        })
    })

    it("does not acquire a token when the order does not allow editing", async () => {
        const order = makePurchaseOrderCenter({ allowedActions: ["SUBMIT"] })
        let props: EditActionsProps = makeProps()
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )

        props = makeProps({ mode: "edit", order })
        rerender()

        await act(async () => {
            await Promise.resolve()
        })
        expect(apiMocks.acquireDraftEditToken).not.toHaveBeenCalled()
        expect(result.current.draftEditToken).toBeNull()
        expect(result.current.lineEdits).toEqual({})
    })

    it("saves a draft with the computed payload and reports success", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.savePurchaseOrderDraft.mockResolvedValue(succeededSave)
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({ data: order }))
        let props: EditActionsProps = makeProps({ setResult, refetch })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult, refetch })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))

        let ok = false
        await act(async () => {
            ok = await result.current.handleSave()
        })

        expect(ok).toBe(true)
        expect(apiMocks.savePurchaseOrderDraft.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                purchaseOrderId: "po-1",
                expectedLockVersion: 3,
                draftEditToken: "tok-1",
                paymentTermCode: "POSTPAY_NET15",
                paymentTermLabel: "货到 15 天",
                lines: [
                    expect.objectContaining({
                        lineId: "line-1",
                        lineType: "ITEM_SERVICE",
                        quantity: "10",
                        unitCostGross: "100.00",
                        inputTaxRate: "0.13",
                    }),
                ],
                idempotencyKey: expect.any(String),
            }),
        )
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "草稿已保存",
                reference: "REF-SAVE",
            }),
        )
        expect(refetch).toHaveBeenCalledTimes(1)
    })

    it("rejects a save when a line edit fails inline validation", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        let props: EditActionsProps = makeProps({ setResult })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))

        act(() => {
            result.current.setLineEdits({
                "line-1": {
                    quantity: "0",
                    unitCostGross: "100.00",
                    inputTaxRate: "0.13",
                },
            })
        })

        let ok = true
        await act(async () => {
            ok = await result.current.handleSave()
        })

        expect(ok).toBe(false)
        expect(apiMocks.savePurchaseOrderDraft).not.toHaveBeenCalled()
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "rejected",
                title: "保存失败",
            }),
        )
    })

    it("reports an unknown save outcome without claiming success", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.savePurchaseOrderDraft.mockResolvedValue({
            status: "unknown",
            message: "网络中断",
            idempotencyKey: "k-1",
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        let props: EditActionsProps = makeProps({ setResult })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))

        let ok = true
        await act(async () => {
            ok = await result.current.handleSave()
        })

        expect(ok).toBe(false)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "unknown",
                title: "保存结果未知",
                reference: "PO-2026-001",
            }),
        )
    })

    it("submits after a save-before-submit and navigates to review mode", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.savePurchaseOrderDraft.mockResolvedValue(succeededSave)
        apiMocks.submitPurchaseOrderForReview.mockResolvedValue({
            status: "succeeded",
            data: {
                submissionId: "sub-2",
                submissionNo: 1,
                purchaseNo: "PO-2026-002",
                lockVersion: 4,
            },
            reference: "REF-SUBMIT",
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({
            data: {
                ...order,
                identity: { ...order.identity, lockVersion: 4 },
            } as PurchaseOrderCenterView,
        }))
        let props: EditActionsProps = makeProps({ setResult, refetch })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult, refetch })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))

        await act(async () => {
            await result.current.handleSubmit()
        })

        expect(
            apiMocks.submitPurchaseOrderForReview.mock.calls[0]?.[0],
        ).toEqual(
            expect.objectContaining({
                purchaseOrderId: "po-1",
                expectedLockVersion: 4,
                expectedDraftContentHash: "hash-4",
                draftEditToken: "tok-1",
                idempotencyKey: expect.any(String),
            }),
        )
        expect(result.current.submitConfirmOpen).toBe(false)
        expect(result.current.draftEditToken).toBeNull()
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/procurement/orders/po-1",
        )
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "已提交审批",
                reference: "REF-SUBMIT",
            }),
        )
    })

    it("stops the submit flow when the save-before-submit fails", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.savePurchaseOrderDraft.mockResolvedValue({
            status: "failed",
            message: "数据已更新，请刷新后重试",
            code: "VERSION_CONFLICT",
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        let props: EditActionsProps = makeProps({ setResult })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))
        act(() => {
            result.current.setSubmitConfirmOpen(true)
        })

        await act(async () => {
            await result.current.handleSubmit()
        })

        expect(apiMocks.submitPurchaseOrderForReview).not.toHaveBeenCalled()
        expect(result.current.submitConfirmOpen).toBe(false)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "rejected",
                title: "提交前保存未成功",
            }),
        )
    })

    it("refuses to save while the submit outcome is still unknown", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.savePurchaseOrderDraft.mockResolvedValue(succeededSave)
        apiMocks.submitPurchaseOrderForReview.mockResolvedValue({
            status: "unknown",
            message: "网络中断",
            idempotencyKey: "k-submit",
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({
            data: {
                ...order,
                identity: { ...order.identity, lockVersion: 4 },
            } as PurchaseOrderCenterView,
        }))
        let props: EditActionsProps = makeProps({ setResult, refetch })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult, refetch })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))

        await act(async () => {
            await result.current.handleSubmit()
        })
        const callsAfterSubmit =
            apiMocks.savePurchaseOrderDraft.mock.calls.length

        let ok = true
        await act(async () => {
            ok = await result.current.handleSave()
        })

        expect(ok).toBe(false)
        expect(apiMocks.savePurchaseOrderDraft.mock.calls.length).toBe(
            callsAfterSubmit,
        )
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "unknown",
                title: "提交结果待确认",
            }),
        )
    })

    it("voids a purchase draft, refreshes related state and returns to view mode", async () => {
        apiMocks.voidPurchaseOrderDraft.mockResolvedValue({
            status: "succeeded",
            data: {
                purchaseOrderId: "po-1",
                status: "VOIDED",
                lockVersion: 4,
            },
            reference: "VOID-V4",
        })
        const order = makePurchaseOrderCenter({
            allowedActions: ["EDIT", "SUBMIT", "VOID"],
        })
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({ data: order }))
        const { result } = renderHookWithProviders(
            () =>
                usePurchaseOrderDetailEditActions(
                    makeProps({ order, setResult, refetch }),
                ),
            { queryClient: createFreshQueryClient() },
        )

        act(() => {
            result.current.setVoidConfirmOpen(true)
        })
        await act(async () => {
            await result.current.handleVoid()
        })

        expect(apiMocks.voidPurchaseOrderDraft.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                purchaseOrderId: "po-1",
                expectedLockVersion: 3,
                reason: "采购草稿不再需要",
                idempotencyKey: expect.any(String),
            }),
        )
        expect(result.current.voidConfirmOpen).toBe(false)
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/procurement/orders/po-1",
        )
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "采购草稿已作废",
                reference: "VOID-V4",
            }),
        )
    })

    it("starts a change and refreshes on success", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.startPurchaseChange.mockResolvedValue({
            status: "succeeded",
            data: { changeId: "chg-1", baseRevisionNo: 2 },
            reference: "REF-CHANGE",
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        const refetch = vi.fn(async () => ({ data: order }))
        let props: EditActionsProps = makeProps({ setResult, refetch })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult, refetch })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))
        act(() => {
            result.current.setChangeConfirmOpen(true)
        })

        await act(async () => {
            await result.current.handleStartChange()
        })

        expect(apiMocks.startPurchaseChange.mock.calls[0]?.[0]).toEqual(
            expect.objectContaining({
                purchaseOrderId: "po-1",
                expectedLockVersion: 3,
                idempotencyKey: expect.any(String),
            }),
        )
        expect(result.current.changeConfirmOpen).toBe(false)
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(navMocks.replace).toHaveBeenCalledWith(
            "/procurement/orders/po-1?section=changes",
        )
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "已创建采购变更工作副本",
            }),
        )
    })

    it("reports a blocked result when the change cannot start", async () => {
        apiMocks.acquireDraftEditToken.mockResolvedValue({
            draftEditToken: "tok-1",
            lockVersion: 3,
        })
        apiMocks.startPurchaseChange.mockResolvedValue({
            status: "failed",
            message: "当前状态不能发起变更",
            code: "NOT_ALLOWED",
        })
        const order = makePurchaseOrderCenter()
        const setResult = vi.fn()
        let props: EditActionsProps = makeProps({ setResult })
        const { result, rerender } = renderHookWithProviders(
            () => usePurchaseOrderDetailEditActions(props),
            { queryClient: createFreshQueryClient() },
        )
        props = makeProps({ mode: "edit", order, setResult })
        rerender()
        await waitFor(() => expect(result.current.draftEditToken).toBe("tok-1"))

        await act(async () => {
            await result.current.handleStartChange()
        })

        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "无法发起变更",
            }),
        )
    })
})
