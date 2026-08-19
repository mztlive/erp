import { describe, it, expect, vi, beforeEach } from "vitest"
import { act } from "@testing-library/react"

import { renderHookWithProviders } from "@/features/test-utils"
import { usePaymentReversalFlow } from "./use-payment-reversal-flow"
import type { PaymentReversalRow } from "@/features/supplier-payables/types"

const { ensureReversalMock, submitReversalMock } = vi.hoisted(() => ({
    ensureReversalMock: vi.fn(),
    submitReversalMock: vi.fn(),
}))

vi.mock("@/features/supplier-payables/hooks/queries", () => ({
    useEnsurePaymentReversalDraftMutation: () => ({
        mutateAsync: ensureReversalMock,
        isPending: false,
    }),
    useSubmitPaymentReversalMutation: () => ({
        mutateAsync: submitReversalMock,
        isPending: false,
    }),
}))

const reversalRow = (
    overrides: Partial<PaymentReversalRow> = {},
): PaymentReversalRow => ({
    reversalId: "pr-1",
    reversalNo: "PCZ-1",
    originalPaymentId: "src_3",
    reasonText: "录入错误",
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
            id: "def-pr-1",
            name: "付款冲正审批",
            version: 1,
            nodes: [{ key: "n1", name: "冲正复核", assigneeName: "张三" }],
            publishedNodes: [],
        },
        recentHistory: [],
        historyHasMore: false,
        allowedActions: ["SUBMIT"],
    },
    ...overrides,
})

function setup() {
    const openReversalPreview = vi.fn()
    const setLastResult = vi.fn()
    const setActionError = vi.fn()
    const args = { openReversalPreview, setLastResult, setActionError }
    const rendered = renderHookWithProviders(() =>
        usePaymentReversalFlow(args),
    )
    return {
        ...rendered,
        args,
        openReversalPreview,
        setLastResult,
        setActionError,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("usePaymentReversalFlow", () => {
    it("creates a reversal draft then submits approval without posting", async () => {
        ensureReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow(),
        })
        submitReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow({
                status: "in_approval",
                statusLabel: "审批中",
                baselineVersion: 2,
            }),
        })
        const { result, openReversalPreview, setLastResult, setActionError } =
            setup()
        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        expect(ensureReversalMock).toHaveBeenCalledWith(
            expect.objectContaining({
                sourcePaymentId: "src_3",
                amount: "50.00",
                reason: "录入错误",
            }),
        )
        expect(openReversalPreview).toHaveBeenCalledWith("pr-1")
        expect(result.current.reversalSubmitOpen).toBe(true)
        expect(result.current.reversalRequest).toBeNull()

        await act(async () => {
            await result.current.confirmReversalSubmit()
        })
        expect(submitReversalMock).toHaveBeenCalledWith({
            reversalId: "pr-1",
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w12-pr-src_3-/),
        })
        expect(setLastResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "冲正已提交审批",
            description: "已按已绑定的审批流程启动审批，原付款保留。",
            reference: expect.stringMatching(/^w12-pr-src_3-/),
            facts: [
                { label: "冲正单号", value: "PCZ-1" },
                { label: "当前状态", value: "审批中" },
            ],
        })
        expect(setActionError).not.toHaveBeenCalled()
        expect(result.current.reversalSubmitOpen).toBe(false)
    })

    it("reuses the reversal key for the same payment and reason retry", async () => {
        ensureReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow(),
        })
        const { result } = setup()
        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        const firstKey = ensureReversalMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        expect(ensureReversalMock.mock.calls[1][0].idempotencyKey).toBe(
            firstKey,
        )
        expect(ensureReversalMock.mock.calls[1][0].sourcePaymentId).toBe(
            "src_3",
        )
    })

    it("rotates the reversal key when the source payment or reason changes", async () => {
        ensureReversalMock
            .mockResolvedValueOnce({
                status: "succeeded",
                reversal: reversalRow({
                    reversalId: "pr-1",
                    originalPaymentId: "src_3",
                }),
            })
            .mockResolvedValueOnce({
                status: "succeeded",
                reversal: reversalRow({
                    reversalId: "pr-2",
                    reversalNo: "PCZ-2",
                    originalPaymentId: "src_9",
                }),
            })
            .mockResolvedValueOnce({
                status: "succeeded",
                reversal: reversalRow({
                    reversalId: "pr-3",
                    reversalNo: "PCZ-3",
                    originalPaymentId: "src_9",
                    reasonText: "金额错误",
                }),
            })
        const { result } = setup()
        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
                amount: "50.00",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        const firstKey = ensureReversalMock.mock.calls[0][0].idempotencyKey

        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_9",
                sourcePaymentNo: "FK-9",
                amount: "80.00",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        const secondKey = ensureReversalMock.mock.calls[1][0].idempotencyKey
        expect(secondKey).not.toBe(firstKey)
        expect(ensureReversalMock.mock.calls[1][0].sourcePaymentId).toBe(
            "src_9",
        )

        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_9",
                sourcePaymentNo: "FK-9",
                amount: "80.00",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("金额错误")
        })
        expect(ensureReversalMock.mock.calls[2][0].idempotencyKey).not.toBe(
            secondKey,
        )
        expect(ensureReversalMock.mock.calls[2][0].reason).toBe("金额错误")
    })

    it("keeps the prepare key when submitting the same draft", async () => {
        ensureReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow(),
        })
        submitReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow({
                status: "in_approval",
                statusLabel: "审批中",
            }),
        })
        const { result } = setup()
        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        const prepareKey = ensureReversalMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.beginReversalSubmit(reversalRow())
        })
        await act(async () => {
            await result.current.confirmReversalSubmit()
        })
        expect(submitReversalMock).toHaveBeenCalledWith({
            reversalId: "pr-1",
            expectedVersion: 1,
            idempotencyKey: prepareKey,
        })
    })

    it("does not reuse the first reversal key when submitting another draft", async () => {
        ensureReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow(),
        })
        submitReversalMock.mockResolvedValue({
            status: "succeeded",
            reversal: reversalRow({
                reversalId: "pr-9",
                status: "in_approval",
                statusLabel: "审批中",
            }),
        })
        const { result } = setup()
        act(() => {
            result.current.setReversalRequest({
                sourcePaymentId: "src_3",
                sourcePaymentNo: "FK-3",
            })
        })
        await act(async () => {
            await result.current.prepareReversalDraft("录入错误")
        })
        const firstKey = ensureReversalMock.mock.calls[0][0].idempotencyKey
        act(() => {
            result.current.beginReversalSubmit(
                reversalRow({
                    reversalId: "pr-9",
                    reversalNo: "PCZ-9",
                    originalPaymentId: "src_9",
                    reasonText: "另一笔",
                }),
            )
        })
        await act(async () => {
            await result.current.confirmReversalSubmit()
        })
        expect(submitReversalMock).toHaveBeenCalledWith({
            reversalId: "pr-9",
            expectedVersion: 1,
            idempotencyKey: expect.stringMatching(/^w12-pr-pr-9-/),
        })
        expect(submitReversalMock.mock.calls[0][0].idempotencyKey).not.toBe(
            firstKey,
        )
    })
})
