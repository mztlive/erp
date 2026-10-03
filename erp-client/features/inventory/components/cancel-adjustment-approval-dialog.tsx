"use client"

import * as React from "react"

import { getErrorMessage, isApiError } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"
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
    reviseRejected = false,
    onCancelled,
}: {
    stockAdjustmentId: string
    command: StockAdjustmentCancelCommand
    currentNodeName?: string
    id: string
    reviseRejected?: boolean
    onCancelled?: (stockAdjustmentId: string) => void
}) {
    const [open, setOpen] = React.useState(false)
    const ledger = React.useRef(new FormalCommandKeyLedger())
    const [uncertain, setUncertain] = React.useState(false)
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
            if (cancelApproval.isPending) return
            const request = ledger.current.acquire(
                "cancel",
                `inventory:${stockAdjustmentId}:cancel`,
                {
                    stockAdjustmentId,
                    command,
                    reason: value.reason.trim(),
                },
            )
            try {
                await cancelApproval.mutateAsync({
                    ...request.payload,
                    idempotencyKey: request.idempotencyKey,
                })
                ledger.current.settle("cancel", "succeeded")
                setUncertain(false)
                setOpen(false)
                onCancelled?.(stockAdjustmentId)
            } catch (error) {
                const outcome =
                    isApiError(error) && error.code === "OUTCOME_UNKNOWN"
                        ? "unknown"
                        : classifyFormalCommandError(error)
                ledger.current.settle("cancel", outcome)
                setUncertain(outcome === "unknown")
                if (isApprovalConflict(error)) {
                    setConflictMessage(approvalConflictMessage(error))
                    return
                }
                setConflictMessage(
                    outcome === "unknown"
                        ? "撤回结果暂无法确认，原因已保留，请使用本次操作重试。"
                        : getErrorMessage(
                              error,
                              "撤回失败，请核对审批状态后重试",
                          ),
                )
            }
        },
    })

    React.useEffect(() => {
        if (!open || ledger.current.peek("cancel")) return
        form.reset({ reason: reviseRejected ? "修改驳回后的原单" : "" })
        setUncertain(false)
        setConflictMessage(null)
    }, [form, open, reviseRejected])

    return (
        <>
            <Button
                id={`${id}-trigger`}
                type="button"
                variant="outline"
                onClick={() => setOpen(true)}
            >
                {reviseRejected ? "修改原单" : "撤回审批"}
            </Button>
            <Dialog
                open={open}
                onOpenChange={(nextOpen) => {
                    if (!nextOpen && (cancelApproval.isPending || uncertain))
                        return
                    setOpen(nextOpen)
                }}
            >
                <DialogContent
                    closeButtonId={`${id}-close`}
                    showCloseButton={!cancelApproval.isPending && !uncertain}
                >
                    <DialogHeader>
                        <DialogTitle>
                            {reviseRejected ? "修改原单" : "撤回审批"}
                        </DialogTitle>
                        <DialogDescription>
                            当前节点：{currentNodeName ?? "—"}。
                            {reviseRejected
                                ? "撤回后打开原调整单草稿，修改后可重新提交。"
                                : "撤回后库存调整单将回到草稿。"}
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
                                    disabled={
                                        cancelApproval.isPending || uncertain
                                    }
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
                                disabled={cancelApproval.isPending || uncertain}
                                onClick={() => setOpen(false)}
                            >
                                取消
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id={`${id}-submit`}
                                    label={
                                        uncertain
                                            ? "使用本次操作重试"
                                            : reviseRejected
                                              ? "撤回并修改"
                                              : "确认撤回"
                                    }
                                    disabled={cancelApproval.isPending}
                                    loading={cancelApproval.isPending}
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
        </>
    )
}
