"use client"

import * as React from "react"

import { useHistoryBackfillCommandMutation } from "@/features/history-backfill/hooks/queries"
import { newRequestId } from "@/features/history-backfill/lib/presentation"
import type {
    HistoryBackfillCommandResult,
    HistoryBackfillJobCore,
    HistoryBackfillReportView,
} from "@/features/history-backfill/types"

export type BackfillCommandAction =
    | "VALIDATE_SOURCE"
    | "START"
    | "RESUME"
    | "REATTRIBUTE"
    | "CONFIRM_REPORT"

/**
 * 任务详情页的命令执行状态：组装 operationId / idempotencyKey、
 * 走 Command mutation 提交，并把最近一次结果保存在本地状态里。
 */
export function useBackfillCommand(
    job: HistoryBackfillJobCore,
    report?: HistoryBackfillReportView,
) {
    const commandMutation = useHistoryBackfillCommandMutation()
    const [actionResult, setActionResult] =
        React.useState<HistoryBackfillCommandResult | null>(null)

    const runCommand = React.useCallback(
        async (action: BackfillCommandAction, itemIds?: string[]) => {
            const operationId = newRequestId("op")
            const idempotencyKey =
                action === "RESUME"
                    ? `${job.idempotencyNamespace}:resume:${job.lockVersion}`
                    : newRequestId(`idem_${action.toLowerCase()}`)
            const result = await commandMutation.mutateAsync({
                action,
                jobId: job.id,
                expectedLockVersion: job.lockVersion,
                rangeStart: job.rangeStart,
                rangeEnd: job.rangeEnd,
                operationId,
                idempotencyKey,
                itemIds,
                reportVersion: report?.reportVersion,
            })
            setActionResult(result)
            return result
        },
        [commandMutation, job, report],
    )

    return {
        actionResult,
        isPending: commandMutation.isPending,
        runCommand,
    }
}
