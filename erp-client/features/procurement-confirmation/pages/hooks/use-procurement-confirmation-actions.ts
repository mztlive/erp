"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import { canOpenProcurementConfirmPlan } from "@/features/procurement-confirmation/lib/actions"
import type {
    ConfirmationLineDraft,
    FormalOutcome,
    ProcurementConfirmationTask,
    ProcurementRecommendation,
    RejectReasonCode,
} from "@/features/procurement-confirmation/types"

type ResultState =
    import("@/components/business/feedback").ResultState<FormalOutcome>
type SaveMutation = ReturnType<
    typeof import("@/features/procurement-confirmation/hooks/queries").useSaveProcurementConfirmationMutation
>
type CompleteMutation = ReturnType<
    typeof import("@/features/procurement-confirmation/hooks/queries").useCompleteProcurementMutation
>

/** queueQuery.refetch() 结果中编排需要的部分。 */
export type ProcurementQueueRefetchResult = {
    data?: { tasks: readonly ProcurementConfirmationTask[] } | undefined
    isError: boolean
    error: unknown
}

export type ProcurementConfirmationActionsOptions = {
    task: ProcurementConfirmationTask | undefined
    tasks: readonly ProcurementConfirmationTask[]
    lineDrafts: ConfirmationLineDraft[]
    dirty: boolean
    linesValid: boolean
    allCovered: boolean
    autoNext: boolean
    advanceAfterConfirm: boolean
    recommendation: ProcurementRecommendation | undefined
    saveMutation: SaveMutation
    completeMutation: CompleteMutation
    queueRefetch: () => Promise<ProcurementQueueRefetchResult>
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    /** 前进 delta 位返回相邻 workItemId；无相邻返回 undefined */
    neighborId: (delta: number) => string | undefined
    goToWorkItem: (workItemId: string | undefined | null) => void
    setDirty: React.Dispatch<React.SetStateAction<boolean>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
    setSaveMessage: React.Dispatch<React.SetStateAction<string | null>>
    setConfirmOpen: React.Dispatch<React.SetStateAction<boolean>>
    setRejectOpen: React.Dispatch<React.SetStateAction<boolean>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    setFinishedResult: React.Dispatch<React.SetStateAction<ResultState>>
    setAdvanceAfterConfirm: React.Dispatch<React.SetStateAction<boolean>>
}

/**
 * W07 任务动作编排：保存草稿、通过确认、驳回、退回团队、开始处理
 * 与终局确认框守卫。只编排 mutation 与结果状态，不做页面级筛选决策。
 */
export function useProcurementConfirmationActions({
    task,
    tasks,
    lineDrafts,
    dirty,
    linesValid,
    allCovered,
    autoNext,
    advanceAfterConfirm,
    recommendation,
    saveMutation,
    completeMutation,
    queueRefetch,
    replaceUrl,
    neighborId,
    goToWorkItem,
    setDirty,
    setActionError,
    setSaveMessage,
    setConfirmOpen,
    setRejectOpen,
    setLastResult,
    setFinishedResult,
    setAdvanceAfterConfirm,
}: ProcurementConfirmationActionsOptions) {
    const assertAllowed = React.useCallback(
        (action: string) => {
            if (!task) throw new Error("无当前任务")
            if (!task.allowedActions.includes(action)) {
                throw new Error("当前责任或任务版本已变化，请刷新后再处理")
            }
        },
        [task],
    )

    const handleSave = React.useCallback(async (): Promise<boolean> => {
        if (!task) return false
        if (!linesValid) {
            setActionError(
                "请先补齐供应商、数量、成本、税率、交期和供应资质后再保存",
            )
            return false
        }
        try {
            assertAllowed("SAVE")
            const result = await saveMutation.mutateAsync({
                workItemId: task.workItemId,
                expectedTaskVersion: task.taskVersion,
                expectedSubjectVersion: task.subjectVersion,
                confirmationId: task.confirmation.confirmationId,
                submissionId: task.salesSubmission.submissionId,
                expectedEditVersion: task.confirmation.editVersion,
                lines: lineDrafts,
                idempotencyKey: `w07:${task.workItemId}:${task.taskVersion}:save:${task.confirmation.editVersion}`,
            })
            setDirty(false)
            setSaveMessage(`已保存 · 第 ${result.editVersion} 次修改`)
            setActionError(null)
            return true
        } catch (error) {
            setActionError(getErrorMessage(error, "保存失败"))
            return false
        }
    }, [
        assertAllowed,
        lineDrafts,
        linesValid,
        saveMutation,
        setActionError,
        setDirty,
        setSaveMessage,
        task,
    ])

    /** 终局操作打开前：dirty 时先保存，保存失败则中止打开（防止按旧草稿提交） */
    const guardTerminalOpen = React.useCallback(async (): Promise<boolean> => {
        if (!dirty) return true
        const saved = await handleSave()
        if (!saved) {
            setActionError("有未保存的确认分行修改且保存失败，请先处理后再继续")
            return false
        }
        return true
    }, [dirty, handleSave, setActionError])

    const advanceIfNeeded = React.useCallback(
        (shouldAdvance: boolean) => {
            if (!shouldAdvance) return
            const nextId =
                neighborId(1) ??
                tasks.find((t) => t.workItemId !== task?.workItemId)?.workItemId
            if (nextId) {
                goToWorkItem(nextId)
            } else {
                replaceUrl({ currentWorkItemId: null })
            }
        },
        [goToWorkItem, neighborId, replaceUrl, task?.workItemId, tasks],
    )

    const handleApprove = React.useCallback(async () => {
        if (!task) return
        if (!recommendation?.ready || recommendation.lines.length === 0) {
            setActionError("当前采购方案还不能执行，请先处理方案中的问题")
            return
        }
        if (!allCovered) {
            setActionError("请先补齐每项商品的供应商、采购数量、交期和供应资质")
            return
        }
        setActionError(null)
        try {
            assertAllowed("SAVE")
            await saveMutation.mutateAsync({
                workItemId: task.workItemId,
                expectedTaskVersion: task.taskVersion,
                expectedSubjectVersion: task.subjectVersion,
                confirmationId: task.confirmation.confirmationId,
                submissionId: task.salesSubmission.submissionId,
                expectedEditVersion: task.confirmation.editVersion,
                lines: lineDrafts,
                idempotencyKey: `w07:${task.workItemId}:${task.taskVersion}:save:${task.confirmation.editVersion}`,
            })
            setDirty(false)
            // 方案落库后编辑版本已变化：正式确认前重读当前任务。
            const latestTask =
                (await queueRefetch()).data?.tasks.find(
                    (t) => t.workItemId === task.workItemId,
                ) ?? task
            if (!latestTask.allowedActions.includes("APPROVE")) {
                setActionError(
                    latestTask.actionBlockers.find(
                        (blocker) => blocker.action === "APPROVE",
                    )?.message ?? "当前任务还不能通过，请刷新后重试",
                )
                return
            }
            const response = await completeMutation.mutateAsync({
                workItemId: latestTask.workItemId,
                expectedTaskVersion: latestTask.taskVersion,
                expectedSubjectVersion: latestTask.subjectVersion,
                idempotencyKey: `w07:${latestTask.workItemId}:${latestTask.taskVersion}:approve`,
                decision: {
                    reviewResult: "APPROVED",
                    confirmationId: latestTask.confirmation.confirmationId,
                    submissionId: latestTask.salesSubmission.submissionId,
                    expectedConfirmationEditVersion:
                        latestTask.confirmation.editVersion,
                    salesOrderId: latestTask.salesSubmission.salesOrderId,
                    salesOrderNo: latestTask.salesSubmission.salesOrderNo,
                    subjectHash: latestTask.salesSubmission.subjectHash,
                },
            })

            if (response.status === "failed") {
                setActionError(response.message)
                return
            }
            if (response.status === "unknown") {
                setConfirmOpen(false)
                setLastResult({
                    status: "unknown",
                    title: "采购确认结果待核实",
                    description: response.message,
                    reference: response.idempotencyKey,
                    pendingIdempotencyKey: response.idempotencyKey,
                    stayOnItem: true,
                })
                return
            }

            const outcome = response.outcome
            if (outcome.kind !== "APPROVED_AND_SALES_EFFECTIVE") return
            setConfirmOpen(false)
            const approvedResult: ResultState = {
                status: "succeeded",
                title: "采购确认已通过 · 已形成采购创建依据",
                description:
                    advanceAfterConfirm && autoNext
                        ? "销售单已生效，采购创建依据已形成；队列将打开下一条。"
                        : "销售单已生效，采购创建依据已形成；后续建单尚未在本次事务中执行。",
                reference: outcome.reference,
                outcome,
                stayOnItem: !(advanceAfterConfirm && autoNext),
            }
            setLastResult(approvedResult)
            if (advanceAfterConfirm && autoNext) {
                setFinishedResult(approvedResult)
                advanceIfNeeded(true)
            } else {
                setFinishedResult(null)
            }
        } catch (error) {
            setActionError(getErrorMessage(error, "通过失败"))
        }
    }, [
        advanceAfterConfirm,
        advanceIfNeeded,
        allCovered,
        autoNext,
        completeMutation,
        assertAllowed,
        lineDrafts,
        queueRefetch,
        recommendation,
        saveMutation,
        setActionError,
        setConfirmOpen,
        setDirty,
        setFinishedResult,
        setLastResult,
        task,
    ])

    const handleRejectSubmit = React.useCallback(
        async (value: { reasonCode: RejectReasonCode; comment: string }) => {
            if (!task) return
            setActionError(null)
            try {
                assertAllowed("REJECT")
                // guard 自动保存后编辑版本可能已 +1：终局提交前重读任务
                const latestTask =
                    (await queueRefetch()).data?.tasks.find(
                        (t) => t.workItemId === task.workItemId,
                    ) ?? task
                const response = await completeMutation.mutateAsync({
                    workItemId: latestTask.workItemId,
                    expectedTaskVersion: latestTask.taskVersion,
                    expectedSubjectVersion: latestTask.subjectVersion,
                    idempotencyKey: `w07:${latestTask.workItemId}:${latestTask.taskVersion}:reject`,
                    decision: {
                        reviewResult: "REJECTED",
                        confirmationId: latestTask.confirmation.confirmationId,
                        submissionId: latestTask.salesSubmission.submissionId,
                        expectedConfirmationEditVersion:
                            latestTask.confirmation.editVersion,
                        salesOrderId: latestTask.salesSubmission.salesOrderId,
                        salesOrderNo: latestTask.salesSubmission.salesOrderNo,
                        subjectHash: latestTask.salesSubmission.subjectHash,
                        rejectReasonCode: value.reasonCode,
                        comment: value.comment,
                    },
                })
                setRejectOpen(false)
                if (response.status === "failed") {
                    setActionError(response.message)
                    return
                }
                if (response.status === "unknown") {
                    setLastResult({
                        status: "unknown",
                        title: "采购驳回结果待核实",
                        description: response.message,
                        reference: response.idempotencyKey,
                        pendingIdempotencyKey: response.idempotencyKey,
                        stayOnItem: true,
                    })
                    return
                }
                const outcome = response.outcome
                if (outcome.kind !== "REJECTED_TO_SALES") return
                const rejectedResult: ResultState = {
                    status: "rejected",
                    title: "采购确认已驳回 · 本次提交已结束",
                    description:
                        "已形成本次采购确认的驳回结论；未创建采购单、变更单或后继任务。销售可在销售单选择三条固定出路。",
                    reference: outcome.reference,
                    outcome,
                    stayOnItem: !autoNext,
                }
                setLastResult(rejectedResult)
                if (autoNext) {
                    setFinishedResult(rejectedResult)
                    advanceIfNeeded(true)
                } else {
                    setFinishedResult(null)
                }
            } catch (error) {
                setActionError(getErrorMessage(error, "驳回失败"))
            }
        },
        [
            advanceIfNeeded,
            autoNext,
            assertAllowed,
            completeMutation,
            queueRefetch,
            setActionError,
            setFinishedResult,
            setLastResult,
            setRejectOpen,
            task,
        ],
    )

    const handleOpenReject = React.useCallback(async () => {
        try {
            assertAllowed("REJECT")
        } catch (error) {
            setActionError(getErrorMessage(error, "当前任务不可驳回"))
            return
        }
        if (!(await guardTerminalOpen())) return
        setRejectOpen(true)
    }, [assertAllowed, guardTerminalOpen, setActionError, setRejectOpen])

    const handleOpenConfirm = React.useCallback(async () => {
        if (!task || !canOpenProcurementConfirmPlan(task.allowedActions)) {
            setActionError("当前还不能打开确认方案，请先开始处理或刷新任务")
            return
        }
        setAdvanceAfterConfirm(autoNext)
        setConfirmOpen(true)
    }, [autoNext, setActionError, setAdvanceAfterConfirm, setConfirmOpen, task])

    return {
        assertAllowed,
        handleSave,
        handleApprove,
        handleRejectSubmit,
        handleOpenReject,
        handleOpenConfirm,
    }
}
