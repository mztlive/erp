"use client"

import { z } from "zod"

import { MoneyValue } from "@/components/business"
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
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { ReceiptReversalApprovalArea } from "@/features/customer-receivables/components/receipt-reversal-approval-area"

const reversalReasonSchema = z.object({
    reason: z.string().trim().min(1, "请填写原因说明"),
})

/**
 * 回款冲正草稿登记。
 *
 * 原因走 `useAppForm + Zod`；创建后只读展示服务端绑定，不得选择流程或审批人。
 */
export function ReceiptReversalRequestDialog({
    open,
    pending,
    sourceLabel,
    amount,
    approval,
    onOpenChange,
    onSubmit,
}: {
    open: boolean
    pending: boolean
    sourceLabel?: string
    amount?: string
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onSubmit: (reason: string) => void | Promise<void>
}) {
    const form = useAppForm({
        defaultValues: {
            reason: "",
        },
        validators: {
            onChange: reversalReasonSchema,
        },
        onSubmit: async ({ value }) => {
            await onSubmit(value.reason.trim())
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent>
                <DialogHeader>
                    <DialogTitle>发起回款冲正</DialogTitle>
                    <DialogDescription>
                        不编辑、不删除已确认记录与分配；仅追加冲正记录。原单{" "}
                        {sourceLabel}。冲正表示撤销本次回款记录。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-3"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <p className="rounded-lg bg-muted/50 px-3 py-2 text-xs text-muted-foreground">
                        将按原单全额追加冲正
                        {amount ? (
                            <>
                                （
                                <MoneyValue value={amount} />）
                            </>
                        ) : null}
                        ，原记录保留。
                    </p>
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id="customer-receivables-reversal-reason"
                                label="原因说明"
                                placeholder="业务依据与说明"
                                disabled={pending}
                            />
                        )}
                    />
                    {approval ? (
                        <ReceiptReversalApprovalArea
                            phase="draft"
                            approval={approval}
                        />
                    ) : null}
                    <DialogFooter>
                        <Button
                            id="customer-receivables-reversal-request-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                            disabled={pending}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="customer-receivables-reversal-request-submit"
                                label="下一步"
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
