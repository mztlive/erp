"use client"
import { useRef, useState } from "react"
import { z } from "zod"
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
import { LoadingButton } from "@/components/ui/loading-button"
import { getErrorMessage } from "@/lib/api"
import type { PurchasePaymentArrangement } from "../api/payment-arrangement"
import { useSchedulePaymentFollowUp } from "../hooks/use-payment-arrangement"

export function PurchasePaymentFollowUp({
    open,
    onOpenChange,
    purchaseOrderId,
    arrangement,
    onSaved,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    purchaseOrderId: string
    arrangement: PurchasePaymentArrangement
    onSaved: () => void
}) {
    const [pending, setPending] = useState(false)
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && pending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="sm"
                closeButtonId="purchase-payment-follow-up-close"
                showCloseButton={!pending}
            >
                <DialogHeader>
                    <DialogTitle>安排下次跟进</DialogTitle>
                    <DialogDescription>
                        设置下次跟进采购付款安排的日期，到该日期重新提醒处理。
                    </DialogDescription>
                </DialogHeader>
                {open ? (
                    <PurchasePaymentFollowUpForm
                        key={`${arrangement.task_id}:${arrangement.task_version}`}
                        purchaseOrderId={purchaseOrderId}
                        arrangement={arrangement}
                        onPendingChange={setPending}
                        onCancel={() => onOpenChange(false)}
                        onSaved={onSaved}
                    />
                ) : null}
            </DialogContent>
        </Dialog>
    )
}

function PurchasePaymentFollowUpForm({
    purchaseOrderId,
    arrangement,
    onPendingChange,
    onCancel,
    onSaved,
}: {
    purchaseOrderId: string
    arrangement: PurchasePaymentArrangement
    onPendingChange: (pending: boolean) => void
    onCancel: () => void
    onSaved: () => void
}) {
    const mutation = useSchedulePaymentFollowUp(purchaseOrderId)
    const command = useRef<{ payload: string; key: string } | null>(null)
    const form = useAppForm({
        defaultValues: { date: "" },
        validators: {
            onSubmit: z.object({ date: z.string().min(1, "请选择跟进日期") }),
        },
        onSubmit: async ({ value }) => {
            if (!arrangement.task_id || arrangement.task_version == null) return
            const payload = JSON.stringify([
                arrangement.task_id,
                arrangement.task_version,
                value.date,
            ])
            if (command.current?.payload !== payload)
                command.current = { payload, key: crypto.randomUUID() }
            onPendingChange(true)
            try {
                await mutation.mutateAsync({
                    task_id: arrangement.task_id,
                    expected_version: arrangement.task_version,
                    follow_up_date: value.date,
                    idempotency_key: command.current.key,
                })
                onSaved()
            } catch {
                /* 留在表单中展示错误，重试沿用原操作号。 */
            } finally {
                onPendingChange(false)
            }
        },
    })
    return (
        <form
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <form.AppField name="date">
                {(field) => (
                    <field.DateField
                        id="purchase-payment-follow-up-date"
                        label="下次跟进日期"
                        description="到该日期重新跟进；此日期不代表已对账或已获准付款。"
                        required
                        disabled={mutation.isPending}
                    />
                )}
            </form.AppField>
            {mutation.isError && (
                <p role="alert" className="text-body-compact text-destructive">
                    {getErrorMessage(
                        mutation.error,
                        "跟进安排保存失败，请重试",
                    )}
                </p>
            )}
            <DialogFooter>
                <Button
                    id="purchase-payment-follow-up-cancel"
                    type="button"
                    variant="outline"
                    disabled={mutation.isPending}
                    onClick={onCancel}
                >
                    取消
                </Button>
                <LoadingButton
                    id="purchase-payment-follow-up-save"
                    type="submit"
                    loading={mutation.isPending}
                    disabled={mutation.isPending}
                >
                    {mutation.isPending ? "保存中…" : "保存跟进日期"}
                </LoadingButton>
            </DialogFooter>
        </form>
    )
}
