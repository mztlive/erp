"use client"

import * as React from "react"
import { useMutation, useQueries, useQueryClient } from "@tanstack/react-query"

import {
    DEFAULT_WORK_ITEM_CLOSE_REASON_OPTIONS,
    WorkItemResponsibilityDialog as WorkItemResponsibilityDialogPrimitive,
    intersectWorkItemReassignCandidates,
    type WorkItemCloseReasonOption,
    type WorkItemResponsibilityBatchCommand,
    type WorkItemResponsibilityExecutionOutcome,
    type WorkItemResponsibilitySummary,
    type WorkItemResponsibilityTarget,
} from "@/components/business/work-item-responsibility-dialog"
import { getErrorMessage } from "@/lib/api/errors"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    getWorkItemReassignCandidates,
    submitWorkItemResponsibility,
} from "../api"
import { synchronizeWorkItemConflict, workItemKeys } from "../queries"

export {
    DEFAULT_WORK_ITEM_CLOSE_REASON_OPTIONS,
    type WorkItemCloseReasonOption,
    type WorkItemResponsibilitySummary,
    type WorkItemResponsibilityTarget,
}

const BATCH_CONCURRENCY = 3

export type WorkItemResponsibilityDialogProps = Readonly<{
    mode: "reassign" | "close"
    targets: readonly WorkItemResponsibilityTarget[]
    /** `mode="close"` 时传入后端允许的关闭原因码选项；缺省使用 W29 允许的 `MISROUTED` / `DUPLICATE`。 */
    closeReasonOptions?: readonly WorkItemCloseReasonOption[]
    open: boolean
    onOpenChange: (open: boolean) => void
    onCompleted: (summary: WorkItemResponsibilitySummary) => void
    /** 自定义原生自动化 ID 前缀。 */
    idPrefix?: string
    /** 自定义弹窗标题。 */
    title?: string
    /** 自定义弹窗说明。 */
    description?: string
}>

async function runWithConcurrency<T, R>(
    items: readonly T[],
    concurrency: number,
    worker: (item: T) => Promise<R>,
): Promise<R[]> {
    if (items.length === 0) return []
    const results: (R | undefined)[] = Array.from({ length: items.length })
    let cursor = 0
    const workerCount = Math.max(1, Math.min(concurrency, items.length))
    await Promise.all(
        Array.from({ length: workerCount }, async () => {
            while (cursor < items.length) {
                const index = cursor
                cursor += 1
                results[index] = await worker(items[index])
            }
        }),
    )
    return results as R[]
}

/**
 * 绑定 W29 责任管理查询/变更与冲突同步的任务责任弹窗：
 * - `reassign`：并发拉取每个任务的合格候选人并求交集；
 * - `close` / `reassign` 提交：按并发度 3 执行并在 409 冲突时同步最新任务投影、刷新工作台缓存。
 */
export function WorkItemResponsibilityDialog({
    mode,
    targets,
    closeReasonOptions = DEFAULT_WORK_ITEM_CLOSE_REASON_OPTIONS,
    open,
    onOpenChange,
    onCompleted,
    idPrefix,
    title,
    description,
}: WorkItemResponsibilityDialogProps) {
    const queryClient = useQueryClient()

    const candidateQueries = useQueries({
        queries: targets.map((target) => ({
            queryKey: workItemKeys.reassignCandidates(target.workItemId),
            queryFn: () => getWorkItemReassignCandidates(target.workItemId),
            enabled:
                open &&
                mode === "reassign" &&
                target.workItemId.trim().length > 0,
        })),
    })

    const candidatesPending =
        mode === "reassign" && candidateQueries.some((query) => query.isPending)
    const candidatesFetching =
        mode === "reassign" &&
        candidateQueries.some((query) => query.isFetching)
    const candidateErrorQuery =
        mode === "reassign"
            ? candidateQueries.find((query) => query.isError)
            : undefined
    const candidatesReady =
        mode === "reassign" &&
        targets.length > 0 &&
        candidateQueries.every((query) => query.isSuccess)

    const eligibleCandidates = React.useMemo(() => {
        if (!candidatesReady) return []
        return intersectWorkItemReassignCandidates(
            candidateQueries.map((query) => query.data ?? []),
        )
    }, [candidateQueries, candidatesReady])

    const refetchCandidates = React.useCallback(async () => {
        const results = await Promise.all(
            candidateQueries.map((query) => query.refetch()),
        )
        const failed = results.find((result) => result.error)
        if (failed?.error) throw failed.error
    }, [candidateQueries])

    const batchMutation = useMutation({
        meta: { affectsDataScope: true },
        mutationFn: async (
            commands: readonly WorkItemResponsibilityBatchCommand[],
        ): Promise<WorkItemResponsibilityExecutionOutcome[]> => {
            const outcomes = await runWithConcurrency(
                commands,
                BATCH_CONCURRENCY,
                async ({
                    target,
                    command,
                }): Promise<WorkItemResponsibilityExecutionOutcome> => {
                    try {
                        await submitWorkItemResponsibility(command)
                        return { ok: true, id: target.workItemId }
                    } catch (cause) {
                        await synchronizeWorkItemConflict(
                            queryClient,
                            command,
                            cause,
                        )
                        return {
                            ok: false,
                            id: target.workItemId,
                            label: target.label,
                            message: getErrorMessage(
                                cause,
                                mode === "reassign"
                                    ? "任务责任未转交，请刷新后重试"
                                    : "任务未关闭，请刷新后重试",
                            ),
                        }
                    }
                },
            )
            if (outcomes.some((outcome) => outcome.ok)) {
                await Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: workItemKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: queryKeyRoots.workspaceHome,
                    }),
                ])
            }
            return outcomes
        },
    })

    return (
        <WorkItemResponsibilityDialogPrimitive
            mode={mode}
            targets={targets}
            closeReasonOptions={closeReasonOptions}
            open={open}
            onOpenChange={onOpenChange}
            onCompleted={onCompleted}
            idPrefix={idPrefix}
            title={title}
            description={description}
            candidates={eligibleCandidates}
            candidatesPending={candidatesPending}
            candidatesFetching={candidatesFetching}
            candidatesReady={candidatesReady}
            candidatesError={candidateErrorQuery?.error}
            onRetryCandidates={refetchCandidates}
            submitting={batchMutation.isPending}
            onSubmitCommands={(commands) => batchMutation.mutateAsync(commands)}
        />
    )
}
