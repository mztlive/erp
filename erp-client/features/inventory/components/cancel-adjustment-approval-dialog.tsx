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
import {
    approvalConflictMessage,
    isApprovalConflict,
} from "@/features/approval-workflow/api"
import {
    slotForIntent,
    type IdempotencySlot,
} from "@/features/approval-workflow/idempotency"
import { reasonFormSchema } from "@/features/approval-workflow/schema"
import { useCancelStockAdjustmentApprovalMutation } from "@/features/inventory/hooks/queries"
import type { StockAdjustmentCancelCommand } from "@/features/inventory/types"

/**
 * 库存调整普通撤回入口。命令只使用详情投影下发的完整 CAS 令牌。
 */
export function CancelAdjustmentApprovalDialog({
    stockAdjustmentId,
    command,
    currentNodeName,
    id,
}: {
    stockAdjustmentId: string
    command: StockAdjustmentCancelCommand
    currentNodeName?: string
    id: string
}) {
    const [open, setOpen] = React.useState(false)
    const [slot, setSlot] = React.useState<IdempotencySlot | null>(null)
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const cancelApproval = useCancelStockAdjustmentApprovalMutation()

    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: reasonFormSchema,
        },
        onSubmit: async ({ value }) => {
            const nextSlot = slotForIntent(
                slot,
                "cancel",
                command.approvalProcessInstanceId,
                value.reason.trim(),
            )
            setSlot(nextSlot)
            try {
                await cancelApproval.mutateAsync({
                    stockAdjustmentId,
                    command,
                    reason: value.reason,
                    idempotencyKey: nextSlot.key,
                })
                setOpen(false)
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
        setSlot(null)
        setConflictMessage(null)
    }, [form, open])

    return (
        <>
            <Button
                id={`${id}-trigger`}
                type="button"
                variant="outline"
                onClick={() => setOpen(true)}
            >
                撤回审批
            </Button>
            <Dialog open={open} onOpenChange={setOpen}>
                <DialogContent closeButtonId={`${id}-close`}>
                    <DialogHeader>
                        <DialogTitle>撤回审批</DialogTitle>
                        <DialogDescription>
                            当前节点：{currentNodeName ?? "—"}
                            。撤回后库存调整单将回到草稿。
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
                                    id={`${id}-reason`}
                                    label="原因"
                                    required
                                    disabled={cancelApproval.isPending}
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
                                id={`${id}-cancel`}
                                type="button"
                                variant="outline"
                                disabled={cancelApproval.isPending}
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${id}-submit`}
                                    label="确认撤回"
                                    disabled={cancelApproval.isPending}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </>
    )
}
