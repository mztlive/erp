import { act, renderHook } from "@testing-library/react"
import { beforeEach, describe, expect, it, vi } from "vitest"

import {
    useSupplierOrderCenterActions,
    useSupplierOrderCenterResult,
} from "./use-supplier-order-center-actions"
import { useSupplierOrderCenterOrderActions } from "./use-supplier-order-center-order-actions"
import {
    makeDetail,
    makeEvidence,
    makeMutation,
} from "./use-supplier-order-center-fixtures"
import { useSupplierOrderCenterCommandIdentity } from "./use-supplier-order-center-identity"
import type {
    AfterSalesActionInput,
    AfterSalesActionResult,
    FormalActionResponse,
    QueryResultData,
    QueryResultInput,
    ReplayInput,
    ReplayResultData,
    RevealAddressInput,
    RevealAddressResult,
    SupplierOrderDetailView,
} from "@/features/supplier-orders/types"

const querySuccess = (
    status: FormalActionResponse["status"] = "succeeded",
): FormalActionResponse<QueryResultData> => ({
    status,
    message: "ok",
    reference: "ref1",
    data: {
        evidence: makeEvidence(),
        lockVersion: 8,
        workItemStatus: "OPEN",
        allowedActions: [],
        actionBlockers: [],
    },
})

const replaySuccess = (
    status: FormalActionResponse["status"] = "succeeded",
): FormalActionResponse<ReplayResultData> => ({
    status,
    message: "ok",
    reference: "ref2",
    data: {
        evidence: makeEvidence({ summary: "replayed" }),
        lockVersion: 8,
        workItemStatus: "OPEN",
        externalOrderNo: "EXT-2",
        fulfillmentStatus: "SUBMITTING",
        allowedActions: [],
        actionBlockers: [],
    },
})

function renderActions(input: {
    detail: SupplierOrderDetailView | undefined
    workItemId?: string
    currentUserId?: string
}) {
    const setResult = vi.fn()
    const queryResultMutation = makeMutation<
        FormalActionResponse<QueryResultData>,
        QueryResultInput
    >()
    const replayMutation = makeMutation<
        FormalActionResponse<ReplayResultData>,
        ReplayInput
    >()
    const identity = renderHook(() => useSupplierOrderCenterCommandIdentity())
    const { result } = renderHook(() =>
        useSupplierOrderCenterActions({
            orderId: "o1",
            workItemId: input.workItemId,
            detail: input.detail,
            currentUserId: input.currentUserId,
            setResult,
            queryResultMutation,
            replayMutation,
            commandIdentity: identity.result.current.commandIdentity,
            forgetCommandIdentity: identity.result.current.forgetCommandIdentity,
        }),
    )
    return {
        result,
        setResult,
        queryResultMutation,
        replayMutation,
    }
}

function renderOrderActions(detail: SupplierOrderDetailView | undefined) {
    const setResult = vi.fn()
    const afterSalesMutation = makeMutation<
        FormalActionResponse<AfterSalesActionResult>,
        AfterSalesActionInput
    >()
    const revealMutation = makeMutation<
        FormalActionResponse<RevealAddressResult>,
        RevealAddressInput
    >()
    const { result } = renderHook(() =>
        useSupplierOrderCenterOrderActions({
            orderId: "o1",
            detail,
            setResult,
            afterSalesMutation,
            revealMutation,
        }),
    )
    return { result, setResult, afterSalesMutation, revealMutation }
}

beforeEach(() => {
    vi.clearAllMocks()
})

describe("useSupplierOrderCenterResult", () => {
    it("clears the result on demand", () => {
        const { result } = renderHook(() => useSupplierOrderCenterResult())
        expect(result.current.result).toBeNull()
        act(() => {
            result.current.setResult({
                status: "succeeded",
                title: "完成",
                description: "ok",
            })
        })
        expect(result.current.result?.title).toBe("完成")
        act(() => {
            result.current.clearResult()
        })
        expect(result.current.result).toBeNull()
    })
})

describe("handleQueryResult", () => {
    it("blocks when a task entry is required but missing", async () => {
        const detail = makeDetail({
            workItem: undefined,
            workItemBlocker: {
                action: "QUERY_RESULT",
                code: "NO_TASK",
                message: "未查询到正式任务",
            },
        })
        const { result, setResult, queryResultMutation } = renderActions({
            detail,
            workItemId: "wi1",
        })
        await act(async () => {
            await result.current.handleQueryResult()
        })
        expect(queryResultMutation.mutateAsync).not.toHaveBeenCalled()
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "正式任务不可处理",
                description: "未查询到正式任务",
            }),
        )
    })

    it("blocks when the current user has no processing right", async () => {
        const { result, setResult, queryResultMutation } = renderActions({
            detail: makeDetail(),
            currentUserId: "u2",
        })
        await act(async () => {
            await result.current.handleQueryResult()
        })
        expect(queryResultMutation.mutateAsync).not.toHaveBeenCalled()
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "当前没有处理权",
            }),
        )
    })

    it("blocks when the action is not allowed and surfaces the blocker", async () => {
        const detail = makeDetail({
            allowedActions: ["OPEN_CENTER", "NOTE"],
            actionBlockers: [
                {
                    action: "QUERY_RESULT",
                    code: "NOT_ALLOWED",
                    message: "当前不可查询",
                },
            ],
        })
        const { result, setResult, queryResultMutation } = renderActions({
            detail,
            currentUserId: "u1",
        })
        await act(async () => {
            await result.current.handleQueryResult()
        })
        expect(queryResultMutation.mutateAsync).not.toHaveBeenCalled()
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "无法查询原结果",
                description: "当前不可查询",
            }),
        )
    })

    it("submits a TASK command when a work item exists", async () => {
        const detail = makeDetail()
        const { result, setResult, queryResultMutation } = renderActions({
            detail,
            currentUserId: "u1",
        })
        queryResultMutation.mutateAsync.mockResolvedValue(querySuccess())
        await act(async () => {
            await result.current.handleQueryResult()
        })
        const command = queryResultMutation.mutateAsync.mock.calls[0][0] as {
            commandKind: string
            action: { type: string }
            idempotencyKey: string
        }
        expect(command.commandKind).toBe("TASK")
        expect(command.action.type).toBe("QUERY_RESULT")
        expect(command.idempotencyKey).toMatch(/^w26:query:/)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "查询原结果已完成",
                reference: "ref1",
            }),
        )
        expect(result.current.latestInvestigation?.evidenceId).toBe("ev1")
    })

    it("submits an OBJECT command without a work item", async () => {
        const detail = makeDetail({ workItem: undefined })
        const { result, queryResultMutation } = renderActions({
            detail,
            currentUserId: "u1",
        })
        queryResultMutation.mutateAsync.mockResolvedValue(querySuccess())
        await act(async () => {
            await result.current.handleQueryResult()
        })
        const command = queryResultMutation.mutateAsync.mock.calls[0][0] as {
            commandKind: string
            action: string
        }
        expect(command.commandKind).toBe("OBJECT")
        expect(command.action).toBe("QUERY_RESULT")
    })

    it("keeps unknown results as unknown", async () => {
        const { result, setResult, queryResultMutation } = renderActions({
            detail: makeDetail({ workItem: undefined }),
            currentUserId: "u1",
        })
        queryResultMutation.mutateAsync.mockResolvedValue(
            querySuccess("unknown"),
        )
        await act(async () => {
            await result.current.handleQueryResult()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "unknown",
                title: "查询结果仍未知",
            }),
        )
    })

    it("maps failures to a rejected result", async () => {
        const { result, setResult, queryResultMutation } = renderActions({
            detail: makeDetail({ workItem: undefined }),
            currentUserId: "u1",
        })
        queryResultMutation.mutateAsync.mockRejectedValue(
            new Error("网络不可用"),
        )
        await act(async () => {
            await result.current.handleQueryResult()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "rejected",
                title: "查询未完成",
                description: "网络不可用",
            }),
        )
    })
})

describe("handleReplay", () => {
    it("closes the dialog and blocks when the task entry is missing", async () => {
        const detail = makeDetail({
            workItem: undefined,
            workItemBlocker: {
                action: "REPLAY",
                code: "NO_TASK",
                message: "未查询到正式任务",
            },
        })
        const { result, setResult, replayMutation } = renderActions({
            detail,
            workItemId: "wi1",
        })
        act(() => {
            result.current.setReplayOpen(true)
        })
        expect(result.current.replayOpen).toBe(true)
        await act(async () => {
            await result.current.handleReplay()
        })
        expect(replayMutation.mutateAsync).not.toHaveBeenCalled()
        expect(result.current.replayOpen).toBe(false)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "blocked",
                title: "正式任务不可处理",
            }),
        )
    })

    it("submits a TASK replay command and closes the dialog on success", async () => {
        const { result, setResult, replayMutation } = renderActions({
            detail: makeDetail(),
            currentUserId: "u1",
        })
        replayMutation.mutateAsync.mockResolvedValue(replaySuccess())
        act(() => {
            result.current.setReplayOpen(true)
        })
        await act(async () => {
            await result.current.handleReplay()
        })
        const command = replayMutation.mutateAsync.mock.calls[0][0] as {
            commandKind: string
            action: { type: string }
        }
        expect(command.commandKind).toBe("TASK")
        expect(command.action.type).toBe("REPLAY")
        expect(result.current.replayOpen).toBe(false)
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "已安全重发",
            }),
        )
        expect(result.current.latestInvestigation?.summary).toBe("replayed")
    })

    it("maps rejected replays to a failed result", async () => {
        const { result, setResult, replayMutation } = renderActions({
            detail: makeDetail({ workItem: undefined }),
            currentUserId: "u1",
        })
        replayMutation.mutateAsync.mockResolvedValue(replaySuccess("blocked"))
        await act(async () => {
            await result.current.handleReplay()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({ status: "blocked", title: "未重发" }),
        )
    })
})

describe("handleAfterSales", () => {
    it("submits the action, clears the confirm and reports the result", async () => {
        const { result, setResult, afterSalesMutation } = renderOrderActions(
            makeDetail(),
        )
        afterSalesMutation.mutateAsync.mockResolvedValue({
            status: "succeeded",
            message: "取消动作已提交供应商",
            reference: "ar1",
            data: {
                lockVersion: 8,
                cancelStatus: "CANCEL_PENDING",
                refundStatus: "NONE",
                actionRecordId: "ar1",
                note: "动作已登记",
            },
        } satisfies FormalActionResponse<AfterSalesActionResult>)
        act(() => {
            result.current.setAfterSalesConfirm({
                requestId: "r1",
                requestNo: "AS-1",
                mallRequestRef: "MR-1",
                action: "CANCEL",
            })
        })
        await act(async () => {
            await result.current.handleAfterSales("CANCEL", "r1")
        })
        const command = afterSalesMutation.mutateAsync.mock
            .calls[0][0] as {
            orderId: string
            action: string
            afterSalesRequestId: string
            idempotencyKey: string
        }
        expect(command).toEqual(
            expect.objectContaining({
                orderId: "o1",
                expectedLockVersion: 7,
                action: "CANCEL",
                afterSalesRequestId: "r1",
                idempotencyKey: "as-CANCEL-r1",
            }),
        )
        expect(result.current.afterSalesConfirm).toBeNull()
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "取消已提交",
            }),
        )
    })

    it("maps failures to a rejected result", async () => {
        const { result, setResult, afterSalesMutation } = renderOrderActions(
            makeDetail(),
        )
        afterSalesMutation.mutateAsync.mockRejectedValue(new Error("boom"))
        await act(async () => {
            await result.current.handleAfterSales("REFUND", "r1")
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "rejected",
                title: "售后动作未提交",
                description: "提交失败，请稍后重试",
            }),
        )
    })
})

describe("handleReveal", () => {
    it("reports a successful short reveal", async () => {
        const { result, setResult, revealMutation } = renderOrderActions(
            makeDetail(),
        )
        revealMutation.mutateAsync.mockResolvedValue({
            status: "succeeded",
            message: "已揭示",
            data: { address: makeDetail().address, auditEventId: "a1" },
        })
        await act(async () => {
            await result.current.handleReveal()
        })
        expect(revealMutation.mutateAsync).toHaveBeenCalledWith({
            orderId: "o1",
            reason: "履约处理需要核对收货信息",
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({
                status: "succeeded",
                title: "已短时揭示地址",
            }),
        )
    })

    it("maps blocked reveals and errors", async () => {
        const { result, setResult, revealMutation } = renderOrderActions(
            makeDetail(),
        )
        revealMutation.mutateAsync.mockResolvedValue({
            status: "blocked",
            message: "端点未交付",
        })
        await act(async () => {
            await result.current.handleReveal()
        })
        expect(setResult).toHaveBeenCalledWith(
            expect.objectContaining({ status: "blocked", title: "无法揭示" }),
        )
    })
})
