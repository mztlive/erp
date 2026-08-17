import { describe, it, expect, vi, beforeEach, afterEach } from "vitest"
import { renderHook, act, cleanup } from "@testing-library/react"
import * as React from "react"

import type {
    ConfirmationLineDraft,
    FormalActionResponse,
    FormalOutcome,
} from "@/features/procurement-confirmation/types"
import {
    useProcurementConfirmationActions,
    type ProcurementConfirmationActionsOptions,
} from "./use-procurement-confirmation-actions"
import { makeRecommendation, makeTask } from "./test-data"

type ResultState =
    import("@/components/business/feedback").ResultState<FormalOutcome>

type SaveMutation = ReturnType<
    typeof import("@/features/procurement-confirmation/hooks/queries").useSaveProcurementConfirmationMutation
>
type CompleteMutation = ReturnType<
    typeof import("@/features/procurement-confirmation/hooks/queries").useCompleteProcurementMutation
>

function makeSaveMutation() {
    const mutateAsync = vi.fn(async (_cmd: unknown) => ({ editVersion: 3 }))
    return { mutateAsync } as unknown as SaveMutation & {
        mutateAsync: typeof mutateAsync
    }
}

function makeCompleteMutation() {
    const mutateAsync = vi.fn(async (_cmd: unknown) => approveResponse())
    return { mutateAsync } as unknown as CompleteMutation & {
        mutateAsync: typeof mutateAsync
    }
}

function approveResponse(): FormalActionResponse {
    return {
        status: "succeeded",
        outcome: {
            kind: "APPROVED_AND_SALES_EFFECTIVE",
            procurementConfirmationId: "conf_1",
            salesOrderId: "so_1",
            salesOrderNo: "SO-2026-001",
            submissionId: "sub_1",
            subjectHash: "sub_1",
            salesOrderRevisionId: "sr_1",
            receivableAccountId: "ra_1",
            procurementCreationBasisId: "pcb_1",
            reference: "pcb_1",
        },
    }
}

function rejectResponse(): FormalActionResponse {
    return {
        status: "succeeded",
        outcome: {
            kind: "REJECTED_TO_SALES",
            procurementConfirmationId: "conf_1",
            salesOrderId: "so_1",
            salesOrderNo: "SO-2026-001",
            rejectedSubmissionId: "sub_1",
            rejectedSubjectHash: "sub_1",
            workflowActionId: "wa_1",
            nextSalesResolutions: [
                "RESUBMIT_CHANGED_TERMS",
                "REQUEST_LOW_MARGIN_ACCEPTANCE",
                "VOID_AFTER_REJECTION",
            ],
            reference: "wa_1",
            rejectReasonCode: "UNFULFILLABLE",
            comment: "无法履约",
        },
    }
}

function renderActions(
    overrides: Partial<ProcurementConfirmationActionsOptions> = {},
) {
    const task = overrides.task ?? makeTask()
    const state = {
        dirty: overrides.dirty ?? false,
        actionError: null as string | null,
        saveMessage: null as string | null,
        confirmOpen: false,
        rejectOpen: false,
        advanceAfterConfirm: overrides.advanceAfterConfirm ?? true,
        lastResult: null as ResultState,
        finishedResult: null as ResultState,
    }
    const callbacks = {
        neighborId: vi.fn((delta: number) =>
            delta === 1 ? "wi_2" : undefined,
        ),
        goToWorkItem: vi.fn(),
        replaceUrl: vi.fn(),
        queueRefetch: vi.fn(async () => ({
            data: { tasks: [task] },
            isError: false,
            error: null,
        })),
    }
    const setters = {
        setDirty: vi.fn((next: boolean) => {
            state.dirty = next
        }) as unknown as React.Dispatch<React.SetStateAction<boolean>>,
        setActionError: vi.fn((next: string | null) => {
            state.actionError = next
        }) as unknown as React.Dispatch<React.SetStateAction<string | null>>,
        setSaveMessage: vi.fn((next: string | null) => {
            state.saveMessage = next
        }) as unknown as React.Dispatch<React.SetStateAction<string | null>>,
        setConfirmOpen: vi.fn((next: boolean) => {
            state.confirmOpen = next
        }) as unknown as React.Dispatch<React.SetStateAction<boolean>>,
        setRejectOpen: vi.fn((next: boolean) => {
            state.rejectOpen = next
        }) as unknown as React.Dispatch<React.SetStateAction<boolean>>,
        setLastResult: vi.fn((next: ResultState) => {
            state.lastResult = next
        }) as unknown as React.Dispatch<React.SetStateAction<ResultState>>,
        setFinishedResult: vi.fn((next: ResultState) => {
            state.finishedResult = next
        }) as unknown as React.Dispatch<React.SetStateAction<ResultState>>,
        setAdvanceAfterConfirm: vi.fn((next: boolean) => {
            state.advanceAfterConfirm = next
        }) as unknown as React.Dispatch<React.SetStateAction<boolean>>,
    }
    const mutations = {
        saveMutation: makeSaveMutation(),
        completeMutation: makeCompleteMutation(),
    }
    const utils = renderHook(() =>
        useProcurementConfirmationActions({
            task: overrides.task ?? task,
            tasks: overrides.tasks ?? [task],
            lineDrafts:
                overrides.lineDrafts ??
                ([...task.confirmation.lines] as ConfirmationLineDraft[]),
            dirty: overrides.dirty ?? false,
            linesValid: overrides.linesValid ?? true,
            allCovered: overrides.allCovered ?? true,
            autoNext: overrides.autoNext ?? true,
            advanceAfterConfirm: overrides.advanceAfterConfirm ?? true,
            recommendation: overrides.recommendation ?? makeRecommendation(),
            saveMutation: mutations.saveMutation,
            completeMutation: mutations.completeMutation,
            queueRefetch: callbacks.queueRefetch,
            replaceUrl: callbacks.replaceUrl,
            neighborId: callbacks.neighborId,
            goToWorkItem: callbacks.goToWorkItem,
            ...setters,
        }),
    )
    return { ...utils, state, callbacks, setters, mutations, task }
}

beforeEach(() => {
    vi.clearAllMocks()
})

afterEach(() => {
    cleanup()
})

describe("useProcurementConfirmationActions", () => {
    it("blocks saving when the draft lines are invalid", async () => {
        const { result, state, mutations } = renderActions({
            linesValid: false,
        })
        await act(async () => {
            await expect(result.current.handleSave()).resolves.toBe(false)
        })
        expect(state.actionError).toBe(
            "请先补齐供应商、数量、成本、税率、交期和供应资质后再保存",
        )
        expect(mutations.saveMutation.mutateAsync).not.toHaveBeenCalled()
    })

    it("saves with the expected versioned payload and clears dirty", async () => {
        const { result, state, mutations, task } = renderActions()
        await act(async () => {
            await expect(result.current.handleSave()).resolves.toBe(true)
        })
        expect(mutations.saveMutation.mutateAsync).toHaveBeenCalledWith({
            workItemId: task.workItemId,
            expectedTaskVersion: task.taskVersion,
            expectedSubjectVersion: task.subjectVersion,
            confirmationId: task.confirmation.confirmationId,
            submissionId: task.salesSubmission.submissionId,
            expectedEditVersion: task.confirmation.editVersion,
            lines: task.confirmation.lines,
            idempotencyKey: `w07:${task.workItemId}:${task.taskVersion}:save:${task.confirmation.editVersion}`,
        })
        expect(state.dirty).toBe(false)
        expect(state.saveMessage).toBe("已保存 · 第 3 次修改")
        expect(state.actionError).toBeNull()
    })

    it("reports save failures without marking the draft clean", async () => {
        const { result, state, mutations } = renderActions({ dirty: true })
        mutations.saveMutation.mutateAsync.mockRejectedValueOnce(
            new Error("版本冲突"),
        )
        await act(async () => {
            await expect(result.current.handleSave()).resolves.toBe(false)
        })
        expect(state.dirty).toBe(true)
        expect(state.actionError).toBe("版本冲突")
    })

    it("guardTerminalOpen saves first when dirty and aborts when saving fails", async () => {
        const { result, state } = renderActions({ dirty: true })
        await act(async () => {
            await expect(
                result.current.handleOpenReject(),
            ).resolves.toBeUndefined()
        })
        expect(state.rejectOpen).toBe(true)
        expect(state.dirty).toBe(false)

        const failing = renderActions({ dirty: true })
        failing.mutations.saveMutation.mutateAsync.mockRejectedValueOnce(
            new Error("保存失败"),
        )
        await act(async () => {
            await failing.result.current.handleOpenReject()
        })
        expect(failing.state.rejectOpen).toBe(false)
        expect(failing.state.actionError).toBe(
            "有未保存的确认分行修改且保存失败，请先处理后再继续",
        )
    })

    it("refuses to approve when the recommendation is not ready", async () => {
        const { result, state, mutations } = renderActions({
            recommendation: makeRecommendation({ ready: false }),
        })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(state.actionError).toBe(
            "当前采购方案还不能执行，请先处理方案中的问题",
        )
        expect(mutations.saveMutation.mutateAsync).not.toHaveBeenCalled()
    })

    it("refuses to approve with uncovered lines", async () => {
        const { result, state } = renderActions({ allCovered: false })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(state.actionError).toBe(
            "请先补齐每项商品的供应商、采购数量、交期和供应资质",
        )
    })

    it("approves by saving, refetching, completing and advancing to the next item", async () => {
        const { result, state, callbacks, mutations, task } = renderActions()
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(mutations.saveMutation.mutateAsync).toHaveBeenCalledTimes(1)
        expect(callbacks.queueRefetch).toHaveBeenCalledTimes(1)
        expect(mutations.completeMutation.mutateAsync).toHaveBeenCalledWith({
            workItemId: task.workItemId,
            expectedTaskVersion: task.taskVersion,
            expectedSubjectVersion: task.subjectVersion,
            idempotencyKey: `w07:${task.workItemId}:${task.taskVersion}:approve`,
            decision: expect.objectContaining({
                reviewResult: "APPROVED",
                confirmationId: task.confirmation.confirmationId,
                salesOrderId: task.salesSubmission.salesOrderId,
            }),
        })
        expect(state.confirmOpen).toBe(false)
        expect(state.lastResult).toMatchObject({
            status: "succeeded",
            title: "采购确认已通过 · 已形成采购创建依据",
            reference: "pcb_1",
            stayOnItem: false,
        })
        expect(state.finishedResult).toMatchObject({ status: "succeeded" })
        expect(callbacks.goToWorkItem).toHaveBeenCalledWith("wi_2")
    })

    it("stays on the item after approving when autoNext is off", async () => {
        const { result, state, callbacks } = renderActions({ autoNext: false })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(state.lastResult).toMatchObject({
            status: "succeeded",
            stayOnItem: true,
        })
        expect(state.finishedResult).toBeNull()
        expect(callbacks.goToWorkItem).not.toHaveBeenCalled()
    })

    it("keeps the result pending when the decision outcome is unknown", async () => {
        const { result, state, mutations } = renderActions()
        mutations.completeMutation.mutateAsync.mockResolvedValueOnce({
            status: "unknown",
            message: "结果暂未返回",
            idempotencyKey: "w07:wi_1:5:approve",
        })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(state.confirmOpen).toBe(false)
        expect(state.lastResult).toMatchObject({
            status: "unknown",
            title: "采购确认结果待核实",
            pendingIdempotencyKey: "w07:wi_1:5:approve",
            stayOnItem: true,
        })
    })

    it("surfaces failed decision responses as action errors", async () => {
        const { result, state, mutations } = renderActions()
        mutations.completeMutation.mutateAsync.mockResolvedValueOnce({
            status: "failed",
            message: "责任已变化",
            code: "CONFLICT",
        })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(state.actionError).toBe("责任已变化")
        expect(state.lastResult).toBeNull()
    })

    it("rejects the confirmation and advances when autoNext is on", async () => {
        const { result, state, mutations, task } = renderActions()
        mutations.completeMutation.mutateAsync.mockResolvedValueOnce(
            rejectResponse(),
        )
        await act(async () => {
            await result.current.handleRejectSubmit({
                reasonCode: "UNFULFILLABLE",
                comment: "无法履约",
            })
        })
        expect(mutations.completeMutation.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                idempotencyKey: `w07:${task.workItemId}:${task.taskVersion}:reject`,
                decision: expect.objectContaining({
                    reviewResult: "REJECTED",
                    rejectReasonCode: "UNFULFILLABLE",
                    comment: "无法履约",
                }),
            }),
        )
        expect(state.rejectOpen).toBe(false)
        expect(state.lastResult).toMatchObject({
            status: "rejected",
            title: "采购确认已驳回 · 本次提交已结束",
        })
        expect(state.finishedResult).toMatchObject({ status: "rejected" })
    })

    it("opens the confirm dialog when the operator can save the confirmation plan", async () => {
        const { result, state } = renderActions({ autoNext: false })
        await act(async () => {
            await result.current.handleOpenConfirm()
        })
        expect(state.advanceAfterConfirm).toBe(false)
        expect(state.confirmOpen).toBe(true)

        const saveOnly = renderActions({
            task: makeTask({ allowedActions: ["SAVE"] }),
        })
        await act(async () => {
            await saveOnly.result.current.handleOpenConfirm()
        })
        expect(saveOnly.state.confirmOpen).toBe(true)
        expect(saveOnly.state.actionError).toBeNull()

        const denied = renderActions({
            task: makeTask({ allowedActions: ["START_PROCESSING"] }),
        })
        await act(async () => {
            await denied.result.current.handleOpenConfirm()
        })
        expect(denied.state.confirmOpen).toBe(false)
        expect(denied.state.actionError).toBe(
            "当前还不能打开确认方案，请先开始处理或刷新任务",
        )
    })

    it("approves a SAVE-only task after the saved plan unlocks APPROVE", async () => {
        const saveOnly = makeTask({ allowedActions: ["SAVE"] })
        const approved = makeTask({
            allowedActions: ["SAVE", "APPROVE"],
            taskVersion: "6",
            confirmation: {
                ...saveOnly.confirmation,
                editVersion: 3,
            },
        })
        const { result, mutations, callbacks } = renderActions({
            task: saveOnly,
        })
        callbacks.queueRefetch.mockResolvedValueOnce({
            data: { tasks: [approved] },
            isError: false,
            error: null,
        })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(mutations.saveMutation.mutateAsync).toHaveBeenCalledTimes(1)
        expect(mutations.completeMutation.mutateAsync).toHaveBeenCalledWith(
            expect.objectContaining({
                expectedTaskVersion: "6",
                decision: expect.objectContaining({
                    reviewResult: "APPROVED",
                    expectedConfirmationEditVersion: 3,
                }),
            }),
        )
    })

    it("stops after save when the refreshed task still cannot be approved", async () => {
        const saveOnly = makeTask({
            allowedActions: ["SAVE"],
            actionBlockers: [
                {
                    action: "APPROVE",
                    code: "CONFIRMATION_LINES_INCOMPLETE",
                    message: "采购确认分行不完整，不能通过",
                },
            ],
        })
        const { result, state, mutations, callbacks } = renderActions({
            task: saveOnly,
        })
        callbacks.queueRefetch.mockResolvedValueOnce({
            data: { tasks: [saveOnly] },
            isError: false,
            error: null,
        })
        await act(async () => {
            await result.current.handleApprove()
        })
        expect(mutations.saveMutation.mutateAsync).toHaveBeenCalledTimes(1)
        expect(mutations.completeMutation.mutateAsync).not.toHaveBeenCalled()
        expect(state.actionError).toBe("采购确认分行不完整，不能通过")
    })
})
