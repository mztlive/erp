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
    context,
    onApplied,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    workItemId: string
    expectedTaskVersion: string
    defaultDecision: ApprovalDecision
    allowedActions: readonly string[]
    context?: Readonly<{
        documentLabel?: string
        amountLabel?: string
        currentNodeLabel?: string
        impactSummary?: string
    }>
    onApplied?: (view: ApprovalCommandView) => void
}) {
    const submitDecision = useSubmitDecisionMutation()
    const [slot, setSlot] = React.useState<IdempotencySlot | null>(null)
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const appliedViewRef = React.useRef<ApprovalCommandView | null>(null)
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
                appliedViewRef.current = view
                onOpenChange(false)
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
        appliedViewRef.current = null
    }, [defaultDecision, form, open])

    return (
        <Dialog
            open={open}
            onOpenChange={onOpenChange}
            onOpenChangeComplete={(nextOpen) => {
                if (nextOpen || !appliedViewRef.current) return
                const view = appliedViewRef.current
                appliedViewRef.current = null
                onApplied?.(view)
            }}
        >
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>
                        {defaultDecision === "REJECT" ? "确认驳回" : "确认通过"}
                    </DialogTitle>
                    <DialogDescription>
                        {defaultDecision === "REJECT"
                            ? "请核对当前任务。确认后将驳回，并从第一节点开始下一轮审批。"
                            : "请核对单据、金额、当前节点和结果影响。只有确认后才会提交审批决定。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {context ? (
                        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 rounded-md border border-border/60 bg-muted/30 p-3 text-sm">
                            <dt className="text-muted-foreground">单据</dt>
                            <dd className="min-w-0 break-words font-medium">
                                {context.documentLabel?.trim() || "当前任务"}
                            </dd>
                            <dt className="text-muted-foreground">金额</dt>
                            <dd className="num font-medium">
                                {context.amountLabel?.trim() || "未提供金额"}
                            </dd>
                            <dt className="text-muted-foreground">当前节点</dt>
                            <dd className="min-w-0 break-words">
                                {context.currentNodeLabel?.trim() ||
                                    "当前节点待加载"}
                            </dd>
                            <dt className="text-muted-foreground">结果影响</dt>
                            <dd className="min-w-0 break-words">
                                {context.impactSummary?.trim() ||
                                    (defaultDecision === "REJECT"
                                        ? "驳回后从首节点进入下一轮审批。"
                                        : "通过后进入下一审批节点；如无后续节点，则完成本轮审批。")}
                            </dd>
                        </dl>
                    ) : null}
                    {defaultDecision === "REJECT" ? (
                        <form.AppField
                            name="decision"
                            children={(field) => (
                                <input
                                    type="hidden"
                                    value={field.state.value}
                                    readOnly
                                />
                            )}
                        />
                    ) : (
                        <form.AppField
                            name="decision"
                            children={(field) => (
                                <input
                                    type="hidden"
                                    value={field.state.value}
                                    readOnly
                                />
                            )}
                        />
                    )}
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
                        <p
                            className="text-sm text-destructive"
                            role="alert"
                            aria-live="assertive"
                        >
                            {conflictMessage}
                        </p>
                    ) : null}
                    <DialogFooter>
                        <Button
                            type="button"
                            variant="outline"
                            disabled={submitDecision.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                label={
                                    defaultDecision === "REJECT"
                                        ? "确认驳回"
                                        : "确认通过"
                                }
                                disabled={
                                    submitDecision.isPending ||
                                    (defaultDecision === "APPROVE" &&
                                        !canApprove) ||
                                    (defaultDecision === "REJECT" && !canReject)
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
