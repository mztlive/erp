"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { createApprovalIdempotencyKey } from "../idempotency"
import {
    useEligibleReassigneesQuery,
    useReassignApproverMutation,
} from "../queries"
import { reassignFormSchema } from "../schema"
import type { ApprovalCommandView } from "../types"

/**
 * 改派当前审批人。必须通过服务端资格搜索选择具体用户，并提交非空原因。
 */
export function ReassignDialog({
    open,
    onOpenChange,
    instanceId,
    definitionAssigneeName,
    currentAssigneeName,
    expectedInstanceVersion,
    expectedExecutionVersion,
    expectedAssignmentVersion,
    expectedClosedTaskVersion,
    onApplied,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    instanceId: string
    definitionAssigneeName?: string
    currentAssigneeName?: string
    expectedInstanceVersion: string
    expectedExecutionVersion: string
    expectedAssignmentVersion: string
    expectedClosedTaskVersion?: string
    onApplied?: (view: ApprovalCommandView) => void
}) {
    const [search, setSearch] = React.useState("")
    const [appliedSearch, setAppliedSearch] = React.useState("")
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const candidatesQuery = useEligibleReassigneesQuery(
        { instanceId, search: appliedSearch },
        open,
    )
    const reassign = useReassignApproverMutation(instanceId)

    const form = useAppForm({
        defaultValues: {
            targetUserId: "",
            reason: "",
        },
        validators: {
            onChange: reassignFormSchema,
        },
        onSubmit: async ({ value }) => {
            try {
                const view = await reassign.mutateAsync({
                    target_user_id: value.targetUserId,
                    reason: value.reason,
                    expected_instance_version: expectedInstanceVersion,
                    expected_execution_version: expectedExecutionVersion,
                    expected_assignment_version: expectedAssignmentVersion,
                    expected_closed_task_version:
                        expectedClosedTaskVersion ?? null,
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
        },
    })

    React.useEffect(() => {
        if (!open) return
        form.reset({ targetUserId: "", reason: "" })
        setSearch("")
        setAppliedSearch("")
        setIdempotencyKey(createApprovalIdempotencyKey("reassign", instanceId))
        setConflictMessage(null)
    }, [form, instanceId, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>改派当前审批人</DialogTitle>
                    <DialogDescription>
                        定义审批人：{definitionAssigneeName ?? "—"}
                        <br />
                        当前审批人：{currentAssigneeName ?? "—"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="flex gap-2">
                        <Input
                            value={search}
                            onChange={(event) => setSearch(event.target.value)}
                            placeholder="按姓名或账号搜索"
                            aria-label="搜索改派对象"
                        />
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => setAppliedSearch(search.trim())}
                        >
                            搜索
                        </Button>
                    </div>
                    <form.AppField
                        name="targetUserId"
                        children={(field) => (
                            <fieldset className="space-y-2">
                                <legend className="text-sm font-medium">
                                    改派对象
                                </legend>
                                {(candidatesQuery.data ?? []).map(
                                    (candidate) => (
                                        <label
                                            key={candidate.userId}
                                            className="flex items-center gap-2 text-sm"
                                        >
                                            <input
                                                type="radio"
                                                name="targetUserId"
                                                value={candidate.userId}
                                                checked={
                                                    field.state.value ===
                                                    candidate.userId
                                                }
                                                onChange={() =>
                                                    field.handleChange(
                                                        candidate.userId,
                                                    )
                                                }
                                            />
                                            {candidate.name}
                                        </label>
                                    ),
                                )}
                                {candidatesQuery.isSuccess &&
                                (candidatesQuery.data ?? []).length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        没有符合资格的人员
                                    </p>
                                ) : null}
                            </fieldset>
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                label="改派原因"
                                disabled={reassign.isPending}
                            />
                        )}
                    />
                    {conflictMessage ? (
                        <p className="text-sm text-destructive">
                            {conflictMessage}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认改派"
                                disabled={reassign.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
