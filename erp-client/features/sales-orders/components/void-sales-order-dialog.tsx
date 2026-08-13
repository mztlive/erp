"use client"

import * as React from "react"
import { z } from "zod"

import { FormalActionConfirmDialog } from "@/components/business"
import { useAppForm } from "@/components/form"
import {
    Dialog,
    DialogContent,
    DialogFooter,
} from "@/components/ui/dialog"
import { Button } from "@/components/ui/button"

const voidSchema = z.object({
    reason: z.string().trim().min(4, "请填写作废原因"),
})

type VoidSalesOrderDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    pending?: boolean
    onConfirm: (reason: string) => Promise<void>
}

/**
 * 作废销售单：先填原因，再走正式动作确认。
 */
export function VoidSalesOrderDialog({
    open,
    onOpenChange,
    pending = false,
    onConfirm,
}: VoidSalesOrderDialogProps) {
    const [reason, setReason] = React.useState<string | null>(null)

    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: { onChange: voidSchema },
        onSubmit: async ({ value }) => {
            setReason(value.reason.trim())
        },
    })

    React.useEffect(() => {
        if (!open) {
            setReason(null)
            form.reset()
        }
    }, [form, open])

    return (
        <>
            <Dialog
                open={open && reason == null}
                onOpenChange={(next) => {
                    if (!next) onOpenChange(false)
                }}
            >
                <DialogContent>
                    <form
                        className="space-y-3"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="reason">
                            {(field) => (
                                <field.TextareaField
                                    label="作废原因"
                                    rows={4}
                                    placeholder="客户取消 / 谈不拢条件…"
                                />
                            )}
                        </form.AppField>
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
                                    label="下一步"
                                    pendingLabel="校验中"
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>

            <FormalActionConfirmDialog
                open={reason != null}
                onOpenChange={(next) => {
                    if (!next) setReason(null)
                }}
                title="确认作废本单"
                actionLabel="作废"
                confirmLabel="确认作废"
                fromStatus={{ label: "采购未通过", tone: "warning" }}
                toStatus={{ label: "已作废", tone: "void" }}
                lockedFields={["销售单号", "历史提交与驳回记录"]}
                effects={[
                    "本单作废，不能再继续履约或改价重提",
                    `作废原因：${reason ?? ""}`,
                    "采购驳回与历史记录会保留备查",
                ]}
                irreversibleEffects={["作废后不能恢复"]}
                pending={pending}
                onConfirm={async () => {
                    if (!reason) return
                    await onConfirm(reason)
                    onOpenChange(false)
                }}
            />
        </>
    )
}
