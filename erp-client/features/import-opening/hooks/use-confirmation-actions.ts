"use client"

import * as React from "react"

import { useImportConfirmationOperations } from "@/features/import-opening/hooks/queries"
import { commandIdempotencyKey } from "@/features/import-opening/lib/command-idempotency"
import type {
    ImportBatchView,
    ImportConfirmationView,
} from "@/features/import-opening/types"

/** 责任确认卡片组的本地交互状态与命令提交；写命令统一走 queries 的 mutation。 */
export function useConfirmationActions(batch: ImportBatchView) {
    const operations = useImportConfirmationOperations()
    const [confirming, setConfirming] = React.useState<ImportConfirmationView>()
    const [returning, setReturning] = React.useState<ImportConfirmationView>()
    const idempotencyKeys = React.useRef(new Map<string, string>())

    const complete = React.useCallback(
        async (
            confirmation: ImportConfirmationView,
            action: "CONFIRM_SCOPE" | "RETURN_FOR_FIX",
            reasonCode?: string,
            comment?: string,
        ) => {
            const task = confirmation.workItem
            if (!task) return
            operations.resetError()
            const payloadIdentity = [
                task.workItemId,
                action,
                task.taskVersion,
                batch.version,
                confirmation.trialVersion,
                reasonCode ?? "",
                comment ?? "",
            ].join(":")
            await operations.completeConfirmation({
                batchId: batch.batchId,
                batchVersion: batch.version,
                trialVersion: confirmation.trialVersion,
                confirmationScope: confirmation.scope,
                workItemId: task.workItemId,
                taskVersion: task.taskVersion,
                subjectVersion: task.subjectVersion,
                action,
                reasonCode,
                comment,
                idempotencyKey: commandIdempotencyKey(
                    idempotencyKeys.current,
                    payloadIdentity,
                ),
            })
        },
        [batch.batchId, batch.version, operations],
    )

    return {
        confirming,
        setConfirming,
        returning,
        setReturning,
        complete,
        isCompleting: operations.isCompleting,
        error: operations.error,
    }
}
