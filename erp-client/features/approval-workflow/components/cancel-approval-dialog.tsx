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
import { createApprovalIdempotencyKey } from "../idempotency"
import { useCancelApprovalMutation, useCancelBlockedMutation } from "../queries"
import { reasonFormSchema } from "../schema"
import type { ApprovalCommandView } from "../types"

/**
 * 撤回审批或取消受阻审批。
 *
 * 撤回走业务单据资源接口；受阻取消走专用端口。两者都不得调用通用任务关闭。
 */
export function CancelApprovalDialog({
    open,
    onOpenChange,
    mode,
    instanceId,
    documentType,
    documentId,
    currentNodeName,
    afterStatusLabel,
    expectedInstanceVersion,
    expectedExecutionVersion,
    expectedTaskVersion,
    emergency = false,
    onApplied,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    mode: "withdraw" | "cancel-blocked"
    instanceId: string
    documentType?: string
    documentId?: string
    currentNodeName?: string
    afterStatusLabel: string
    expectedInstanceVersion: string
    expectedExecutionVersion: string
    expectedTaskVersion?: string
    emergency?: boolean
    onApplied?: (view: ApprovalCommandView) => void
}) {
    const cancelApproval = useCancelApprovalMutation()
    const cancelBlocked = useCancelBlockedMutation(instanceId)
    const pending = cancelApproval.isPending || cancelBlocked.isPending
    const [idempotencyKey, setIdempotencyKey] = React.useState("")
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )

    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: reasonFormSchema,
        },
        onSubmit: async ({ value }) => {
            try {
                const view =
                    mode === "withdraw"
                        ? await cancelApproval.mutateAsync({
                              documentType: documentType ?? "",
                              documentId: documentId ?? "",
                              request: {
                                  reason: value.reason,
                                  expected_instance_version:
                                      expectedInstanceVersion,
                                  expected_execution_version:
                                      expectedExecutionVersion,
                                  expected_task_version:
                                      expectedTaskVersion ?? null,
                                  idempotency_key: idempotencyKey,
                              },
                          })
                        : await cancelBlocked.mutateAsync({
                              reason: value.reason,
                              expected_instance_version:
                                  expectedInstanceVersion,
                              expected_execution_version:
                                  expectedExecutionVersion,
                              expected_task_version:
                                  expectedTaskVersion ?? null,
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
        form.reset({ reason: "" })
        setIdempotencyKey(
            createApprovalIdempotencyKey(
                mode === "withdraw" ? "cancel" : "cancel-blocked",
                instanceId,
            ),
        )
        setConflictMessage(null)
    }, [form, instanceId, mode, open])

    const title =
        mode === "withdraw"
            ? emergency
                ? "应急撤回审批"
                : "撤回审批"
            : "取消受阻审批"

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>
                        当前节点：{currentNodeName ?? "—"}。撤回后单据将回到
                        {afterStatusLabel}。
                        {mode === "cancel-blocked"
                            ? "此操作不可恢复，不会改派或继续推进。"
                            : null}
                        {emergency
                            ? "你正在代原提交人撤回，系统会记录应急代办身份。"
                            : null}
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
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                label="原因"
                                disabled={pending}
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
                            disabled={pending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                label={
                                    mode === "withdraw"
                                        ? "确认撤回"
                                        : "确认取消"
                                }
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
