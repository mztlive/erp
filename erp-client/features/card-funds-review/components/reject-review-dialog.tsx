"use client"

import { z } from "zod"

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
import { Textarea } from "@/components/ui/textarea"
import { REJECT_REASON_LABEL } from "@/features/card-funds-review/types"
import type { RejectReasonCode } from "@/features/card-funds-review/types"

const rejectSchema = z.object({
    reasonCode: z.enum([
        "EVIDENCE_INSUFFICIENT",
        "FACTS_MISMATCH",
        "COUNTERPARTY_UNCLEAR",
        "OTHER",
    ]),
    comment: z.string().trim().min(5, "请填写至少 5 个字的驳回说明"),
})

export type RejectReviewValue = {
    reasonCode: RejectReasonCode
    comment: string
}

export function RejectReviewDialog({
    open,
    onOpenChange,
    pending,
    onSubmit,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    pending: boolean
    onSubmit: (value: RejectReviewValue) => Promise<void>
}) {
    const rejectForm = useAppForm({
        defaultValues: {
            reasonCode: "EVIDENCE_INSUFFICIENT" as RejectReasonCode,
            comment: "",
        },
        validators: { onChange: rejectSchema },
        onSubmit: async ({ value }) => {
            await onSubmit(value)
        },
    })

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>驳回复核</DialogTitle>
                    <DialogDescription>
                        本次驳回会完成当前任务，并按当前财务责任规则在同一事务创建后继待办；未决问题继续保留在工作台。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-3"
                    onSubmit={(e) => {
                        e.preventDefault()
                        void rejectForm.handleSubmit()
                    }}
                >
                    <rejectForm.AppField
                        name="reasonCode"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label>
                                    驳回原因
                                    <span className="text-destructive">*</span>
                                </Label>
                                <OptionCombobox
                                    id="card-contracts-funds-review-reject-reason"
                                    value={field.state.value}
                                    onValueChange={(v) =>
                                        field.handleChange(
                                            v as RejectReasonCode,
                                        )
                                    }
                                    options={(
                                        Object.keys(
                                            REJECT_REASON_LABEL,
                                        ) as RejectReasonCode[]
                                    ).map((code) => ({
                                        value: code,
                                        label: REJECT_REASON_LABEL[code],
                                    }))}
                                    className="w-full"
                                    allowClear={false}
                                />
                            </div>
                        )}
                    />
                    <rejectForm.AppField
                        name="comment"
                        children={(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor="reject-comment">
                                    补充说明
                                    <span className="text-destructive">*</span>
                                </Label>
                                <Textarea
                                    id="reject-comment"
                                    value={field.state.value}
                                    onChange={(e) =>
                                        field.handleChange(e.target.value)
                                    }
                                    onBlur={field.handleBlur}
                                    rows={3}
                                />
                                {field.state.meta.errors?.[0] ? (
                                    <p className="text-xs text-destructive">
                                        {String(field.state.meta.errors[0])}
                                    </p>
                                ) : null}
                            </div>
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            id="card-contracts-funds-review-reject-cancel"
                            render={
                                <Button
                                    id="card-contracts-funds-review-reject-cancel"
                                    type="button"
                                    variant="outline"
                                    disabled={pending}
                                />
                            }
                        >
                            取消
                        </DialogClose>
                        <rejectForm.AppForm>
                            <rejectForm.SubmitButton
                                id="card-contracts-funds-review-reject-confirm"
                                label="确认驳回"
                                pendingLabel="提交中…"
                                variant="destructive"
                                disabled={pending}
                            />
                        </rejectForm.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
