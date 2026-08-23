import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import { useSupplierOrderCenterTaskActions } from "./use-supplier-order-center-task-actions"
import {
    makeDetail,
    makeEvidence,
    makeMutation,
} from "./use-supplier-order-center-fixtures"
import { useSupplierOrderCenterCommandIdentity } from "./use-supplier-order-center-identity"
import type {
    CompleteSupplierOrderTaskInput,
    CompleteSupplierOrderTaskResult,
    FormalActionResponse,
    SupplierOrderDetailView,
} from "@/features/supplier-orders/types"

function renderTaskActions(input: {
    detail: SupplierOrderDetailView | undefined
    completionEvidence?: NonNullable<
        SupplierOrderDetailView["lastInvestigation"]
    >
}) {
    const setResult = vi.fn()
    const refetch = vi.fn()
    const completeTaskMutation = makeMutation<
        FormalActionResponse<CompleteSupplierOrderTaskResult>,
        CompleteSupplierOrderTaskInput
    >()
    const identity = renderHook(() => useSupplierOrderCenterCommandIdentity())
    const { result } = renderHook(() =>
        useSupplierOrderCenterTaskActions({
            detail: input.detail,
            completionEvidence: input.completionEvidence,
            refetch,
            setResult,
            completeTaskMutation,
            commandIdentity: identity.result.current.commandIdentity,
            forgetCommandIdentity:
                identity.result.current.forgetCommandIdentity,
        }),
    )
    return {
        result,
        setResult,
        refetch,
        completeTaskMutation,
    }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("handleCompleteTask", () => {
    it("blocks when the verified evidence is missing", async () => {
        const { result, setResult, completeTaskMutation } = renderTaskActions({
            detail: makeDetail(),
        })
        act(() => {
            result.current.setCompleteOpen(true)
        })
        await act(async () => {
            await result.current.handleCompleteTask()
        })
        expect(completeTaskMutation.mutateAsync).not.toHaveBeenCalled()
        expect(result.current.completeOpen).toBe(false)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "尚不能完成任务",
            }),
        )
    })

    it("uses the in-flight investigation when the detail has none", async () => {
        const evidence = makeEvidence()
        const { result, completeTaskMutation } = renderTaskActions({
            detail: makeDetail({ lastInvestigation: undefined }),
            completionEvidence: evidence,
        })
        completeTaskMutation.mutateAsync.mockResolvedValue({
            status: "succeeded",
            message: "已根据可验证结果完成任务。",
            reference: "op1",
            data: {
                operationId: "op1",
                workItemId: "wi1",
                workItemStatus: "COMPLETED",
                taskVersion: "4",
                lockVersion: 9,
                resolution: "ORDER_COMPLETED",
            },
        } satisfies FormalActionResponse<CompleteSupplierOrderTaskResult>)
        await act(async () => {
            await result.current.handleCompleteTask()
        })
        const command = completeTaskMutation.mutateAsync.mock
            .calls[0][0] as CompleteSupplierOrderTaskInput
        expect(command.workItemId).toBe("wi1")
        expect(command.decision).toEqual(
            expect.objectContaining({
                type: "CONFIRM_VERIFIED_TERMINAL_RESULT",
                orderId: "o1",
                expectedOrderLockVersion: 7,
                verifiedSupplierActionResultId: "act1",
                resolution: "ORDER_COMPLETED",
            }),
        )
    })

    it("prefers the detail investigation over the in-flight one", async () => {
        const fresh = makeEvidence({ verifiedResolution: "ORDER_ACCEPTED" })
        const { result, completeTaskMutation } = renderTaskActions({
            detail: makeDetail({ lastInvestigation: fresh }),
            completionEvidence: makeEvidence({
                verifiedResolution: "ORDER_COMPLETED",
            }),
        })
        completeTaskMutation.mutateAsync.mockResolvedValue({
            status: "succeeded",
            message: "ok",
            reference: "op1",
            data: {
                operationId: "op1",
                workItemId: "wi1",
                workItemStatus: "COMPLETED",
                taskVersion: "4",
                lockVersion: 9,
                resolution: "ORDER_ACCEPTED",
            },
        })
        await act(async () => {
            await result.current.handleCompleteTask()
        })
        const command = completeTaskMutation.mutateAsync.mock
            .calls[0][0] as CompleteSupplierOrderTaskInput
        expect(command.decision.resolution).toBe("ORDER_ACCEPTED")
    })

    it("refetches and reports the outcome on success", async () => {
        const { result, setResult, refetch, completeTaskMutation } =
            renderTaskActions({
                detail: makeDetail({ lastInvestigation: makeEvidence() }),
            })
        act(() => {
            result.current.setCompleteOpen(true)
        })
        completeTaskMutation.mutateAsync.mockResolvedValue({
            status: "succeeded",
            message: "已根据可验证结果完成任务。",
            reference: "op1",
            data: {
                operationId: "op1",
                workItemId: "wi1",
                workItemStatus: "COMPLETED",
                taskVersion: "4",
                lockVersion: 9,
                resolution: "ORDER_COMPLETED",
            },
        })
        await act(async () => {
            await result.current.handleCompleteTask()
        })
        expect(result.current.completeOpen).toBe(false)
        expect(refetch).toHaveBeenCalledTimes(1)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "任务已完成",
            }),
        )
    })

    it("keeps unknown outcomes as unknown", async () => {
        const { result, setResult, completeTaskMutation } = renderTaskActions({
            detail: makeDetail({ lastInvestigation: makeEvidence() }),
        })
        completeTaskMutation.mutateAsync.mockResolvedValue({
            status: "unknown",
            message: "结果未回",
            reference: "op1",
        })
        await act(async () => {
            await result.current.handleCompleteTask()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "unknown",
                title: "任务未完成",
            }),
        )
    })

    it("maps failures to a rejected result", async () => {
        const { result, setResult, completeTaskMutation } = renderTaskActions({
            detail: makeDetail({ lastInvestigation: makeEvidence() }),
        })
        completeTaskMutation.mutateAsync.mockRejectedValue(new Error("boom"))
        await act(async () => {
            await result.current.handleCompleteTask()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "rejected",
                title: "任务完成未提交",
                description: "boom",
            }),
        )
    })
})
