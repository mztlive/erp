"use client"

import { OptionCombobox } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Label } from "@/components/ui/label"
import { rejectSchema } from "@/features/procurement-confirmation/lib/validation"
import {
    NEXT_SALES_RESOLUTION_COPY,
    REJECT_REASON_LABEL,
    type RejectReasonCode,
} from "@/features/procurement-confirmation/types"

export type RejectConfirmationDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    onSubmit: (value: {
        reasonCode: RejectReasonCode
        comment: string
    }) => Promise<void>
}

/** 驳回采购二次确认弹窗：形成驳回结论并结束当前任务。 */
export function RejectConfirmationDialog({
    open,
    onOpenChange,
    onSubmit,
}: RejectConfirmationDialogProps) {
    const rejectForm = useAppForm({
        defaultValues: {
            reasonCode: "",
            comment: "",
        },
        validators: { onChange: rejectSchema, onMount: rejectSchema },
        onSubmit: async ({ value }) => {
            await onSubmit({
                reasonCode: value.reasonCode as RejectReasonCode,
                comment: value.comment.trim(),
            })
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>驳回采购二次确认</DialogTitle>
                    <DialogDescription>
                        将形成本次确认的驳回结论并结束当前任务；不创建采购单、变更单或后继任务。销售可在销售单选择三条固定出路。
                    </DialogDescription>
                </DialogHeader>
                <form
                    onSubmit={(event) => {
                        event.preventDefault()
                        void rejectForm.handleSubmit()
                    }}
                    className="space-y-4"
                >
                    <div className="space-y-2">
                        <Label htmlFor="reject-reason-code">驳回原因</Label>
                        <rejectForm.AppField name="reasonCode">
                            {(field) => (
                                <OptionCombobox
                                    id="reject-reason-code"
                                    value={field.state.value}
                                    onValueChange={(value) => {
                                        if (value)
                                            field.handleChange(
                                                value as RejectReasonCode,
                                            )
                                    }}
                                    options={(
                                        Object.keys(
                                            REJECT_REASON_LABEL,
                                        ) as RejectReasonCode[]
                                    ).map((code) => ({
                                        value: code,
                                        label: REJECT_REASON_LABEL[code],
                                    }))}
                                    allowClear={false}
                                    aria-label="驳回原因"
                                    placeholder="请选择驳回原因"
                                />
                            )}
                        </rejectForm.AppField>
                    </div>
                    <rejectForm.AppField name="comment">
                        {(field) => (
                            <field.TextareaField
                                label="补充说明"
                                placeholder="请说明无法履约、成本、交期或资质等问题"
                                rows={4}
                            />
                        )}
                    </rejectForm.AppField>
                    <div className="rounded-lg border border-border bg-muted/40 p-3 text-xs text-muted-foreground">
                        <p className="mb-2 font-medium text-foreground">
                            销售后续三条固定出路（驳回后只读展示）
                        </p>
                        <ol className="list-decimal space-y-1 pl-4">
                            {NEXT_SALES_RESOLUTION_COPY.map((item) => (
                                <li key={item.code}>{item.title}</li>
                            ))}
                        </ol>
                    </div>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button type="button" variant="outline" />
                            }
                        >
                            取消
                        </DialogClose>
                        <rejectForm.AppForm>
                            <rejectForm.SubmitButton
                                label="确认驳回并完成任务"
                                pendingLabel="正在驳回"
                                variant="destructive"
                            />
                        </rejectForm.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
