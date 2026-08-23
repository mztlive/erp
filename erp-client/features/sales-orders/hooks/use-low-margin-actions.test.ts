import { describe, it, expect, vi, beforeEach } from "vitest"
import { act, waitFor } from "@testing-library/react"

const apiMocks = vi.hoisted(() => ({
    completeLowMarginManagerConfirmation: vi.fn(),
}))

vi.mock("@/features/sales-orders/api/sales-orders", () => ({
    completeLowMarginManagerConfirmation:
        apiMocks.completeLowMarginManagerConfirmation,
}))

vi.mock("@/features/sales-orders/hooks/queries", () => ({
    salesOrderKeys: {
        detail: (id: string) => ["sales-orders", "detail", id],
    },
}))

import type {
    ActiveLowMarginManagerConfirmation,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import { useLowMarginManagerActions } from "@/features/sales-orders/hooks/use-low-margin-actions"
import { renderHookWithProviders } from "@/features/test-utils"

const makeOrder = (): SalesOrderListItem =>
    ({
        id: "so-1",
        lockVersion: 5,
    }) as SalesOrderListItem

const makeConfirmation = (): ActiveLowMarginManagerConfirmation => ({
    confirmationId: "lm-1",
    workItemId: "wi-1",
    taskVersion: "3",
    subjectVersion: "sv-1",
    lowMarginSubmissionId: "lms-1",
    rejectedProcurementConfirmationId: "pc-1",
    acceptanceReason: "毛利偏低但客户重要",
    evidenceReferenceIds: ["ev-1"],
    ownerUser: { id: "u-1", displayName: "销售领导" },
    allowedActions: ["APPROVE", "REJECT"],
    actionBlockers: [],
})

describe("useLowMarginManagerActions", () => {
    beforeEach(() => {
        vi.clearAllMocks()
        apiMocks.completeLowMarginManagerConfirmation.mockReset()
    })

    const renderActions = (onResult = vi.fn()) => {
        const { result } = renderHookWithProviders(() =>
            useLowMarginManagerActions({
                order: makeOrder(),
                confirmation: makeConfirmation(),
                onResult,
            }),
        )
        return { result, onResult }
    }

    it("approves and reports the procurement reference", async () => {
        apiMocks.completeLowMarginManagerConfirmation.mockResolvedValue({
            outcome: "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED",
            salesOrderId: "so-1",
            lowMarginSubmissionId: "lms-1",
            salesOrderReviewId: "sr-1",
            workflowActionId: "wf-1",
            newProcurementConfirmationId: "pc-2",
            newProcurementWorkItemId: "wi-2",
        })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmApprove()
        })

        expect(
            apiMocks.completeLowMarginManagerConfirmation,
        ).toHaveBeenCalledWith(
            {
                salesOrderId: "so-1",
                workItemId: "wi-1",
                taskVersion: "3",
                subjectVersion: "sv-1",
                lowMarginSubmissionId: "lms-1",
                rejectedProcurementConfirmationId: "pc-1",
                expectedSalesOrderLockVersion: 5,
                decision: "APPROVE",
                idempotencyKey: "w05:wi-1:3:APPROVE",
            },
            expect.anything(),
        )
        expect(onResult).toHaveBeenCalledWith({
            status: "succeeded",
            title: "已同意低毛利承接",
            description: "已创建新的采购确认待办。",
            reference: "pc-2",
            nextResponsible: "采购",
        })
        await waitFor(() => expect(result.current.isPending).toBe(false))
    })

    it("falls back to the workflow action reference for other approve outcomes", async () => {
        apiMocks.completeLowMarginManagerConfirmation.mockResolvedValue({
            outcome: "LOW_MARGIN_REJECTED_TO_SALES",
            salesOrderId: "so-1",
            lowMarginSubmissionId: "lms-1",
            salesOrderReviewId: "sr-1",
            workflowActionId: "wf-9",
        })
        const { result, onResult } = renderActions()

        await act(async () => {
            await result.current.confirmApprove()
        })

        expect(onResult).toHaveBeenCalledWith(
            expect.objectContaining({ reference: "wf-9" }),
        )
    })

    it("blocks an empty reject payload", async () => {
        apiMocks.completeLowMarginManagerConfirmation.mockResolvedValue({
            outcome: "LOW_MARGIN_REJECTED_TO_SALES",
            salesOrderId: "so-1",
            lowMarginSubmissionId: "lms-1",
            salesOrderReviewId: "sr-1",
            workflowActionId: "wf-9",
        })
        const { result, onResult } = renderActions()

        await act(async () => {
            await expect(result.current.confirmReject()).rejects.toThrow(
                "原因代码和驳回说明不能为空",
            )
        })

        expect(
            apiMocks.completeLowMarginManagerConfirmation,
        ).not.toHaveBeenCalled()
        expect(onResult).not.toHaveBeenCalled()
    })

    it("rejects with the recorded reason and reports the outcome", async () => {
        apiMocks.completeLowMarginManagerConfirmation.mockResolvedValue({
            outcome: "LOW_MARGIN_REJECTED_TO_SALES",
            salesOrderId: "so-1",
            lowMarginSubmissionId: "lms-1",
            salesOrderReviewId: "sr-1",
            workflowActionId: "wf-9",
        })
        const { result, onResult } = renderActions()

        act(() => {
            result.current.setReasonCode("MARGIN_TOO_LOW")
            result.current.setComment("毛利过低，不能承接")
        })

        await act(async () => {
            await result.current.confirmReject()
        })

        expect(
            apiMocks.completeLowMarginManagerConfirmation,
        ).toHaveBeenCalledWith(
            {
                salesOrderId: "so-1",
                workItemId: "wi-1",
                taskVersion: "3",
                subjectVersion: "sv-1",
                lowMarginSubmissionId: "lms-1",
                rejectedProcurementConfirmationId: "pc-1",
                expectedSalesOrderLockVersion: 5,
                decision: "REJECT",
                reasonCode: "MARGIN_TOO_LOW",
                comment: "毛利过低，不能承接",
                idempotencyKey: "w05:wi-1:3:REJECT",
            },
            expect.anything(),
        )
        expect(onResult).toHaveBeenCalledWith({
            status: "rejected",
            title: "已驳回低毛利承接",
            description: "销售已回到采购驳回固定处置。",
            reference: "wf-9",
            nextResponsible: "销售",
        })
    })
})
