"use client"

import * as React from "react"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { useImportExecutionOperations } from "@/features/import-opening/hooks/queries"
import { commandIdempotencyKey } from "@/features/import-opening/lib/command-idempotency"
import type {
    BatchSection,
    ImportBatchView,
    ImportExecutionAction,
} from "@/features/import-opening/types"
import { hasPermission } from "@/lib/permissions"

/** 导入执行卡片的权限判定、本地交互状态与执行命令提交。 */
export function useExecutionActions(
    batch: ImportBatchView,
    onGoSection: (section: BatchSection) => void,
) {
    const operations = useImportExecutionOperations()
    const profileQuery = useAccountProfileQuery()
    const [confirming, setConfirming] = React.useState<
        "START_APPLY" | "RETRY_FAILED"
    >()
    const [cancelling, setCancelling] = React.useState(false)
    const idempotencyKeys = React.useRef(new Map<string, string>())
    const canExecute = hasPermission(
        profileQuery.data?.permissions,
        "legacy_import_batch:execute",
    )
    const canStart = canExecute && batch.allowedActions.includes("START_APPLY")
    const canCancel =
        canExecute && batch.allowedActions.includes("CANCEL_PENDING")
    const canRetry = canExecute && batch.allowedActions.includes("RETRY_FAILED")
    const visible = canStart || canCancel || canRetry

    const execute = React.useCallback(
        async (
            action: ImportExecutionAction,
            reasonCode?: string,
            comment?: string,
        ) => {
            operations.resetError()
            const identity = [
                batch.batchId,
                action,
                batch.version,
                batch.trialVersion,
                reasonCode ?? "",
                comment ?? "",
            ].join(":")
            const result = await operations.execute({
                batchId: batch.batchId,
                expectedBatchVersion: batch.version,
                expectedTrialVersion:
                    batch.trialVersion === "0" ? undefined : batch.trialVersion,
                action,
                reasonCode,
                comment: comment?.trim() || undefined,
                requestId: commandIdempotencyKey(
                    idempotencyKeys.current,
                    identity,
                ),
            })
            setConfirming(undefined)
            setCancelling(false)
            if (result.nextStep === "MONITOR_PROGRESS") {
                onGoSection("progress")
            } else if (result.nextStep === "REVIEW_RESULT") {
                onGoSection("result")
            } else {
                onGoSection("confirm")
            }
        },
        [batch, onGoSection, operations],
    )

    return {
        confirming,
        setConfirming,
        cancelling,
        setCancelling,
        canStart,
        canCancel,
        canRetry,
        visible,
        execute,
        isExecuting: operations.isExecuting,
        error: operations.error,
    }
}
