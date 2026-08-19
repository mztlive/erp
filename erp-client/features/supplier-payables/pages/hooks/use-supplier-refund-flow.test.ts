import { describe, it, expect, vi, beforeEach } from "vitest"
import { act } from "@testing-library/react"

import { renderHookWithProviders } from "@/features/test-utils"
import { useSupplierRefundFlow } from "./use-supplier-refund-flow"
import type { SupplierRefundRow } from "@/features/supplier-payables/types"

const { ensureRefundMock, submitRefundMock } = vi.hoisted(() => ({
    ensureRefundMock: vi.fn(),
    submitRefundMock: vi.fn(),
}))

vi.mock("@/features/supplier-payables/hooks/queries", () => ({
    useEnsureSupplierRefundDraftMutation: () => ({
        mutateAsync: ensureRefundMock,
        isPending: false,
    }),
    useSubmitSupplierRefundMutation: () => ({
        mutateAsync: submitRefundMock,
        isPending: false,
    }),
}))

const refundRow = (
    overrides: Partial<SupplierRefundRow> = {},
): SupplierRefundRow => ({
    refundId: "srf-1",
    refundNo: "GTK-1",
    supplierId: "sup-1",
    originalPaymentId: "src_3",
    reasonText: "退差额",
    amount: "50.00",
    occurredAt: "",
    status: "draft",
    statusLabel: "草稿",
    statusTone: "neutral",
    baselineVersion: 1,
    allowedActions: ["VIEW_DETAIL"],
    actionBlockers: [],
    approval: {
        requirement: "PROCESS_REQUIRED",
        definition: {
            id: "def-srf-1",
            name: "供应商退款审批",
            version: 1,
            nodes: [{ key: "n1", name: "退款复核", assigneeName: "张三" }],
            publishedNodes: [],
        },
        recentHistory: [],
        historyHasMore: false,
        allowedActions: ["SUBMIT"],
    },
    ...overrides,
})

function setup() {
    const openRefundPreview = vi.fn()
    const setLastResult = vi.fn()
    const setActionError = vi.fn()
    const args = { openRefundPreview, setLastResult, setActionError }
    const rendered = renderHookWithProviders(() => useSupplierRefundFlow(args))
    return { ...rendered, args, openRefundPreview, setLastResult, setActionError }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useSupplierRefundFlow", () => {
    it("creates a refund draft then submits approval without posting", async () => {
        ensureRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow(),
        })
        submitRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow({
                status: "in_approval",
                statusLabel: "审批中",
                baselineVersion: 2,
            }),
        })
        const { result, openRefundPreview, setLastResult, setActionError } =
            setup()
        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                supplierId: "sup-1",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        expect(ensureRefundMock).toHaveBeenCalledWith(
            expect.objectContaining({
                sourcePaymentId: "src_3",
                amount: "50.00",
                reason: "退差额",
            }),
        )
        expect(openRefundPreview).toHaveBeenCalledWith("srf-1")
        expect(result.current.refundSubmitOpen).toBe(true)
        expect(result.current.refundRequest).toBeNull()

        await act(async () => {
            await result.current.confirmRefundSubmit()
        })
        expect(submitRefundMock).toHaveBeenCalledWith({
            refundId: "srf-1",
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w12-rev-src_3-/),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "退款已提交审批",
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: expect.stringMatching(/^w12-rev-src_3-/),
            facts: [
                { label: "退款单号", value: "GTK-1" },
                { label: "当前状态", value: "审批中" },
            ],
        })
        expect(setActionError).not.toHaveBeenCalled()
        expect(result.current.refundSubmitOpen).toBe(false)
    })

    it("reuses the refund key for the same payment and reason retry", async () => {
        ensureRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow(),
        })
        const { result } = setup()
        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                supplierId: "sup-1",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        const firstKey = ensureRefundMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                supplierId: "sup-1",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        expect(ensureRefundMock.mock.calls[1][0].idempotencyKey).toBe(firstKey)
        expect(ensureRefundMock.mock.calls[1][0].sourcePaymentId).toBe("src_3")
    })

    it("rotates the refund key when the source payment or reason changes", async () => {
        ensureRefundMock
            .mockResolvedValueOnce({
                status: "succeeded",
                refund: refundRow({
                    refundId: "srf-1",
                    originalPaymentId: "src_3",
                }),
            })
            .mockResolvedValueOnce({
                status: "succeeded",
                refund: refundRow({
                    refundId: "srf-2",
                    refundNo: "GTK-2",
                    originalPaymentId: "src_9",
                }),
            })
            .mockResolvedValueOnce({
                status: "succeeded",
                refund: refundRow({
                    refundId: "srf-3",
                    refundNo: "GTK-3",
                    originalPaymentId: "src_9",
                    reasonText: "全额退",
                }),
            })
        const { result } = setup()
        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                supplierId: "sup-1",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        const firstKey = ensureRefundMock.mock.calls[0][0].idempotencyKey

        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_9",
                sourcePaymentNo: "FK-9",
                supplierId: "sup-1",
                amount: "80.00",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        const secondKey = ensureRefundMock.mock.calls[1][0].idempotencyKey
        expect(secondKey).not.toBe(firstKey)
        expect(ensureRefundMock.mock.calls[1][0].sourcePaymentId).toBe("src_9")

        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_9",
                sourcePaymentNo: "FK-9",
                supplierId: "sup-1",
                amount: "80.00",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("全额退")
        })
        expect(ensureRefundMock.mock.calls[2][0].idempotencyKey).not.toBe(
            secondKey,
        )
        expect(ensureRefundMock.mock.calls[2][0].reason).toBe("全额退")
    })

    it("keeps the prepare key when submitting the same draft", async () => {
        ensureRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow(),
        })
        submitRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow({
                status: "in_approval",
                statusLabel: "审批中",
            }),
        })
        const { result } = setup()
        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                supplierId: "sup-1",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        const prepareKey = ensureRefundMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.beginRefundSubmit(refundRow())
        })
        await act(async () => {
            await result.current.confirmRefundSubmit()
        })
        expect(submitRefundMock).toHaveBeenCalledWith({
            refundId: "srf-1",
            expectedVersion: 1,
            idempotencyKey: prepareKey,
        })
    })

    it("does not reuse the first refund key when submitting another draft", async () => {
        ensureRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow(),
        })
        submitRefundMock.mockResolvedValue({
            status: "succeeded",
            refund: refundRow({
                refundId: "srf-9",
                status: "in_approval",
                statusLabel: "审批中",
            }),
        })
        const { result } = setup()
        act(() => {
            result.current.setRefundRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                supplierId: "sup-1",
            })
        })
        await act(async () => {
            await result.current.prepareRefundDraft("退差额")
        })
        const firstKey = ensureRefundMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.beginRefundSubmit(
                refundRow({
                    refundId: "srf-9",
                    refundNo: "GTK-9",
                    originalPaymentId: "src_9",
                    reasonText: "另一笔",
                }),
            )
        })
        await act(async () => {
            await result.current.confirmRefundSubmit()
        })
        expect(submitRefundMock).toHaveBeenCalledWith({
            refundId: "srf-9",
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w12-rev-srf-9-/),
        })
        expect(submitRefundMock.mock.calls[0][0].idempotencyKey).not.toBe(
            firstKey,
        )
    })
})
