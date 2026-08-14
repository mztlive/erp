"use client"

import * as React from "react"

import { getErrorMessage } from "@/lib/api/errors"
import type {
    FormalOutcome,
    ProcurementConfirmationTask,
} from "@/features/procurement-confirmation/types"
import type { ProcurementQueueRefetchResult } from "./use-procurement-confirmation-actions"

type ResultState = import("@/components/business/feedback").ResultState<FormalOutcome>
type ResponsibilityMutation = ReturnType<
    typeof import("@/features/work-items").useWorkItemResponsibilityMutation
>

export type ProcurementResponsibilityActionsOptions = {
    task: ProcurementConfirmationTask | undefined
    dirty: boolean
    handleSave: () => Promise<boolean>
    responsibilityMutation: ResponsibilityMutation
    queueRefetch: () => Promise<ProcurementQueueRefetchResult>
    replaceUrl: (patch: Record<string, string | null | undefined>) => void
    /** 前进 delta 位返回相邻 workItemId；无相邻返回 undefined */
    neighborId: (delta: number) => string | undefined
    goToWorkItem: (workItemId: string | undefined | null) => void
    assertAllowed: (action: string) => void
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
}

/**
 * 责任类动作编排：退回团队与开始处理。
 * 依赖 SAVE 通过 handleSave 复用统一保存编排。
 */
export function useProcurementResponsibilityActions({
    task,
    dirty,
    handleSave,
    responsibilityMutation,
    queueRefetch,
    replaceUrl,
    neighborId,
    goToWorkItem,
    assertAllowed,
    setActionError,
    setLastResult,
}: ProcurementResponsibilityActionsOptions) {
    const handleReleaseToTeam = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            let currentTask = task
            if (dirty) {
                const saved = await handleSave()
                if (!saved) {
                    setActionError(
                        "有未保存的确认分行修改且保存失败；请重试保存后再跳过",
                    )
                    return
                }
                const refreshed = await queueRefetch()
                if (refreshed.isError) {
                    throw refreshed.error
                }
                const refreshedTask = refreshed.data?.tasks.find(
                    (candidate) => candidate.workItemId === task.workItemId,
                )
                if (!refreshedTask) {
                    throw new Error(
                        "保存后未取得当前任务的新版本，已禁止使用旧版本退回团队",
                    )
                }
                currentTask = refreshedTask
            }
            if (!currentTask.allowedActions.includes("RELEASE_TO_TEAM")) {
                throw new Error("当前责任已变化，请刷新后再退回团队")
            }
            const nextId = neighborId(1)
            const response = await responsibilityMutation.mutateAsync({
                kind: "RELEASE_TO_TEAM",
                workItemId: currentTask.workItemId,
                expectedTaskVersion: currentTask.taskVersion,
                reason: "当前确认数据已保存，退回团队继续安排",
                idempotencyKey: `w07:${currentTask.workItemId}:${currentTask.taskVersion}:release`,
            })
            if (response.status !== "OPEN") {
                throw new Error("退回团队后任务未保持开放，请刷新核对")
            }
            const released = {
                kind: "RELEASED_TO_TEAM" as const,
                workItemId: response.id,
                workItemStatus: response.status,
                taskVersion: String(response.task_version),
                reference: response.id,
            }
            setLastResult({
                status: "blocked",
                title: "当前项已退回团队",
                description:
                    "原任务保持待处理，未形成通过或驳回结论；个人责任已退回团队。",
                reference: released.reference,
                outcome: released,
            })
            if (nextId) goToWorkItem(nextId)
        } catch (error) {
            setActionError(getErrorMessage(error, "退回团队失败"))
        }
    }, [
        dirty,
        goToWorkItem,
        handleSave,
        neighborId,
        queueRefetch,
        responsibilityMutation,
        setActionError,
        setLastResult,
        task,
    ])

    const handleStartProcessing = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            assertAllowed("START_PROCESSING")
            await responsibilityMutation.mutateAsync({
                kind: "START_PROCESSING",
                workItemId: task.workItemId,
                expectedTaskVersion: task.taskVersion,
                idempotencyKey: `w07:${task.workItemId}:${task.taskVersion}:start`,
            })
            replaceUrl({
                scope: null,
                queueContextId: null,
                currentWorkItemId: task.workItemId,
            })
            await queueRefetch()
        } catch (error) {
            setActionError(getErrorMessage(error, "开始处理失败"))
        }
    }, [
        assertAllowed,
        queueRefetch,
        replaceUrl,
        responsibilityMutation,
        setActionError,
        task,
    ])

    return {
        handleReleaseToTeam,
        handleStartProcessing,
    }
}
