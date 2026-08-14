"use client"

import * as React from "react"

import type { UseQueryResult } from "@tanstack/react-query"

import { getErrorMessage } from "@/lib/api/errors"
import { type ResultState as SharedResultState } from "@/components/business/feedback"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import type {
    CardFundsReviewItemView,
    CardFundsReviewQueueView,
    ConfirmMode,
    FormalOutcome,
    WorkItemAction,
} from "@/features/card-funds-review/types"

type ResultState = SharedResultState<FormalOutcome>

type ReplaceUrlFn = (patch: Record<string, string | null | undefined>) => void

/**
 * 责任动作：退回团队（RELEASE_TO_TEAM）与开始处理（START_PROCESSING）。
 * 开始处理成功后切回个人范围并刷新队列。
 */
export function useCardFundsResponsibilityActions(args: {
    task: CardFundsReviewItemView | undefined
    comment: string
    assertAllowed: (action: WorkItemAction) => void
    responsibilityMutation: ReturnType<
        typeof useWorkItemResponsibilityMutation
    >
    replaceUrl: ReplaceUrlFn
    queueQuery: Pick<UseQueryResult<CardFundsReviewQueueView>, "refetch">
    setConfirmMode: React.Dispatch<React.SetStateAction<ConfirmMode>>
    setLastResult: React.Dispatch<React.SetStateAction<ResultState>>
    setActionError: React.Dispatch<React.SetStateAction<string | null>>
}): {
    handleReleaseToTeam: () => Promise<void>
    startProcessing: () => Promise<void>
} {
    const {
        task,
        comment,
        assertAllowed,
        responsibilityMutation,
        replaceUrl,
        queueQuery,
        setConfirmMode,
        setLastResult,
        setActionError,
    } = args

    const handleReleaseToTeam = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            assertAllowed("RELEASE_TO_TEAM")
            const response = await responsibilityMutation.mutateAsync({
                kind: "RELEASE_TO_TEAM",
                workItemId: task.workItem.workItemId,
                expectedTaskVersion: task.workItem.taskVersion,
                reason: comment.trim() || "待团队补充票款证据",
                idempotencyKey: `w13:${task.workItem.workItemId}:${task.workItem.taskVersion}:release`,
            })
            setConfirmMode(null)
            if (response.status !== "OPEN") {
                throw new Error("退回团队后任务未保持开放")
            }
            const released: FormalOutcome = {
                kind: "RELEASED_TO_TEAM",
                workItemId: response.id,
                workItemStatus: "OPEN",
                taskVersion: String(response.task_version),
                reference: response.id,
            }
            setLastResult({
                status: "blocked",
                title: "当前项已退回团队",
                description:
                    "原任务保持待处理，未形成复核记录；个人责任已退回团队。",
                reference: released.reference,
                outcome: released,
            })
        } catch (error) {
            setActionError(getErrorMessage(error, "退回团队失败"))
        }
    }, [
        assertAllowed,
        comment,
        responsibilityMutation,
        setActionError,
        setConfirmMode,
        setLastResult,
        task,
    ])

    const startProcessing = React.useCallback(async () => {
        if (!task) return
        setActionError(null)
        try {
            assertAllowed("START_PROCESSING")
            await responsibilityMutation.mutateAsync({
                kind: "START_PROCESSING",
                workItemId: task.workItem.workItemId,
                expectedTaskVersion: task.workItem.taskVersion,
                idempotencyKey: `w13:${task.workItem.workItemId}:${task.workItem.taskVersion}:start`,
            })
            replaceUrl({
                scope: null,
                queueContextId: null,
                currentWorkItemId: task.workItem.workItemId,
            })
            await queueQuery.refetch()
        } catch (error) {
            setActionError(getErrorMessage(error, "开始处理失败"))
        }
    }, [
        assertAllowed,
        queueQuery,
        replaceUrl,
        responsibilityMutation,
        setActionError,
        task,
    ])

    return { handleReleaseToTeam, startProcessing }
}
