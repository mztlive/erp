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

import { approvalConflictMessage, isApprovalConflict } from "../api"
import {
    decisionIntentFingerprint,
    slotForIntent,
    type IdempotencySlot,
} from "../idempotency"
import { useSubmitDecisionMutation } from "../queries"
import { decisionFormSchema } from "../schema"
import {
    buildDecisionRequest,
    type ApprovalCommandView,
    type ApprovalDecision,
} from "../types"

/**
 * 审批决定弹窗。
 *
 * 只提交 work_item_id、APPROVE|REJECT、原因、expected_task_version 和幂等键。
 * 请求进行中禁用重复提交；409 刷新事实且不自动重放。
 */
export function DecisionDialog({
    open,
    onOpenChange,
    workItemId,
    expectedTaskVersion,
    defaultDecision,
    allowedActions,
    onApplied,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    workItemId: string
    expectedTaskVersion: string
    defaultDecision: ApprovalDecision
    allowedActions: readonly string[]
    onApplied?: (view: ApprovalCommandView) => void
}) {
    const submitDecision = useSubmitDecisionMutation()
    const [slot, setSlot] = React.useState<IdempotencySlot | null>(null)
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const canApprove = allowedActions.includes("APPROVE")
    const canReject = allowedActions.includes("REJECT")

    const form = useAppForm({
        defaultValues: {
            decision: defaultDecision,
            reason: "",
        },
        validators: {
            onChange: decisionFormSchema,
        },
        onSubmit: async ({ value }) => {
            const fingerprint = decisionIntentFingerprint(
                value.decision,
                value.reason,
            )
            const nextSlot = slotForIntent(
                slot,
                "decision",
                workItemId,
                fingerprint,
            )
            setSlot(nextSlot)
            try {
                const view = await submitDecision.mutateAsync(
                    buildDecisionRequest({
                        workItemId,
                        decision: value.decision,
                        reason: value.reason,
                        expectedTaskVersion,
                        idempotencyKey: nextSlot.key,
                    }),
                )
                setConflictMessage(null)
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
        form.reset({ decision: defaultDecision, reason: "" })
        setSlot(null)
        setConflictMessage(null)
    }, [defaultDecision, form, open])

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>
                        {defaultDecision === "REJECT" ? "驳回" : "通过"}
                    </DialogTitle>
                    <DialogDescription>
                        将按当前任务提交决定。驳回后将从第一节点开始下一轮审批。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="decision"
                        children={(field) => (
                            <fieldset className="space-y-2">
                                <legend className="text-sm font-medium">
                                    决定
                                </legend>
                                <div className="flex gap-2">
                                    <Button
                                        type="button"
                                        variant={
                                            field.state.value === "APPROVE"
                                                ? "default"
                                                : "outline"
                                        }
                                        disabled={
                                            !canApprove ||
                                            submitDecision.isPending
                                        }
                                        onClick={() =>
                                            field.handleChange("APPROVE")
                                        }
                                    >
                                        通过
                                    </Button>
                                    <Button
                                        type="button"
                                        variant={
                                            field.state.value === "REJECT"
                                                ? "destructive"
                                                : "outline"
                                        }
                                        disabled={
                                            !canReject ||
                                            submitDecision.isPending
                                        }
                                        onClick={() =>
                                            field.handleChange("REJECT")
                                        }
                                    >
                                        驳回
                                    </Button>
                                </div>
                            </fieldset>
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                label={
                                    form.state.values.decision === "REJECT"
                                        ? "驳回原因"
                                        : "原因（可选）"
                                }
                                disabled={submitDecision.isPending}
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
                                label="提交决定"
                                disabled={submitDecision.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
