"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { createApprovalIdempotencyKey } from "../idempotency"
import { useResumeApproverMutation } from "../queries"
import type { ApprovalCommandView } from "../types"

/**
 * 恢复当前审批人。不得选择用户，只提交版本与幂等键。
 */
export function ResumeApproverDialog({
    open,
    onOpenChange,
    instanceId,
    expectedInstanceVersion,
    expectedExecutionVersion,
    expectedAssignmentVersion,
    expectedClosedTaskVersion,
    onApplied,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    instanceId: string
    expectedInstanceVersion: string
    expectedExecutionVersion: string
    expectedAssignmentVersion: string
    expectedClosedTaskVersion?: string
    onApplied?: (view: ApprovalCommandView) => void
}) {
    const resume = useResumeApproverMutation(instanceId)
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )

    React.useEffect(() => {
        if (!open) return
        setIdempotencyKey(createApprovalIdempotencyKey("resume", instanceId))
        setConflictMessage(null)
    }, [instanceId, open])

    const submit = async () => {
        try {
            const view = await resume.mutateAsync({
                expected_instance_version: expectedInstanceVersion,
                expected_execution_version: expectedExecutionVersion,
                expected_assignment_version: expectedAssignmentVersion,
                expected_closed_task_version: expectedClosedTaskVersion ?? null,
                idempotency_key: idempotencyKey,
            })
            onOpenChange(false)
            onApplied?.(view)
        } catch (error) {
            if (isApprovalConflict(error)) {
                setConflictMessage(approvalConflictMessage(error))
                return
            }
            throw error
        }
    }

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>恢复当前审批人</DialogTitle>
                    <DialogDescription>
                        将为原审批人创建新的办理记录和新的待办，不会重开旧任务或重放原决定。
                    </DialogDescription>
                </DialogHeader>
                {conflictMessage ? (
                    <p className="text-sm text-destructive">
                        {conflictMessage}
                    </p>
                ) : null}
                <DialogFooter>
                    <Button
                        type="button"
                        variant="outline"
                        disabled={resume.isPending}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <Button
                        type="button"
                        disabled={resume.isPending}
                        onClick={() => void submit()}
                    >
                        {resume.isPending ? "提交中" : "确认恢复"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
