"use client"
import { useRef } from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { getErrorMessage } from "@/lib/api"
import type { PurchasePaymentArrangement } from "../api/payment-arrangement"
import { useSchedulePaymentFollowUp } from "../hooks/use-payment-arrangement"

export function PurchasePaymentFollowUp({
    purchaseOrderId,
    arrangement,
    onSaved,
}: {
    purchaseOrderId: string
    arrangement: PurchasePaymentArrangement
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
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(
                        mutation.error,
                        "跟进安排保存失败，请重试",
                    )}
                </p>
            )}
            <div className="flex justify-end gap-2">
                <Button
                    id="purchase-payment-follow-up-cancel"
                    type="button"
                    variant="ghost"
                    disabled={mutation.isPending}
                    onClick={onSaved}
                >
                    取消
                </Button>
                <Button
                    id="purchase-payment-follow-up-save"
                    type="submit"
                    disabled={mutation.isPending}
                >
                    {mutation.isPending ? "保存中…" : "保存跟进日期"}
                </Button>
            </div>
        </form>
    )
}
