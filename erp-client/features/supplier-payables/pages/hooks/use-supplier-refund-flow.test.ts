import { act, renderHook } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { useSupplierRefundFlow } from "./use-supplier-refund-flow"
import { SubmissionResultUnknownError } from "@/lib/submission-result"
const mocks = vi.hoisted(() => ({ commit: vi.fn(), submit: vi.fn() }))
vi.mock("@/features/supplier-payables/hooks/queries", () => ({
    useCommitSupplierRefundMutation: () => ({
        mutateAsync: mocks.commit,
        isPending: false,
    }),
    useSubmitSupplierRefundMutation: () => ({
        mutateAsync: mocks.submit,
        isPending: false,
    }),
}))

describe("supplier refund consolidated flow", () => {
    it("writes only on final submit and reuses the same idempotency key for an unknown result", async () => {
        mocks.commit.mockReset()
        mocks.submit.mockReset()
        mocks.commit
            .mockResolvedValueOnce({
                status: "unknown",
                message: "结果待确认",
                idempotencyKey: "pending",
            })
            .mockResolvedValueOnce({
                status: "succeeded",
                refund: {
                    refundId: "refund-1",
                    refundNo: "TK-001",
                    statusLabel: "审批中",
                },
            })
        const preview = vi.fn()
        const { result } = renderHook(() =>
            useSupplierRefundFlow({
                openRefundPreview: preview,
                setLastResult: vi.fn(),
                setActionError: vi.fn(),
            }),
        )
        act(() =>
            result.current.setRefundRequest({
                sourcePaymentId: "payment-1",
                amount: "150.00",
                sourcePaymentNo: "FK-001",
                supplierId: "supplier-1",
            }),
        )
        expect(mocks.commit).not.toHaveBeenCalled()
        await act(async () => {
            await expect(
                result.current.prepareRefundDraft("退回重复付款"),
            ).rejects.toBeInstanceOf(SubmissionResultUnknownError)
        })
        expect(result.current.refundRequest).not.toBeNull()
        expect(result.current.refundSubmitOpen).toBe(false)
        await act(async () => {
            await result.current.prepareRefundDraft("退回重复付款")
        })
        expect(mocks.commit.mock.calls[0][0]).toEqual(
            mocks.commit.mock.calls[1][0],
        )
        expect(result.current.refundRequest).toBeNull()
        expect(preview).toHaveBeenCalledWith("refund-1")
        expect(mocks.submit).not.toHaveBeenCalled()
    })
})
