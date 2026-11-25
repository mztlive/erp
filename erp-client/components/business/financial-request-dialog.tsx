"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { CircleAlertIcon, TriangleAlertIcon } from "lucide-react"
import { z } from "zod"
import { toFieldErrors, useAppForm } from "@/components/form"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { MoneyValue } from "@/components/business/values"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogFooter,
} from "@/components/ui/dialog"
import { SubmissionResultUnknownError } from "@/lib/submission-result"
import { compareDecimal, parseDecimal } from "@/lib/fixed-decimal"
import { getErrorMessage } from "@/lib/api/errors"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    bpmReasonSchema,
} from "@/lib/validation/reason-length"

const schema = z.object({
    reason: bpmReasonSchema({ required: true, label: "原因" }),
})

function firstFieldErrorMessage(
    errors: unknown[] | undefined,
): string | undefined {
    return toFieldErrors(errors).find((item) => item?.message)?.message
}

/** Keep source, amount, reason and final submission in one modal; amount editing is opt-in. */
export function FinancialRequestDialog({
    open,
    pending,
    sourceLabel,
    amount,
    editableAmount = false,
    amountLabel = "本次退款金额",
    amountDescription = "填写本次实际退回的款项。退货金额不一定等于现金退款金额。",
    amountError = "请填写大于零且不超过可退金额的数值，最多两位小数",
    title,
    description,
    submitLabel,
    id,
    approvalContent,
    onOpenChange,
    onSubmit,
}: {
    open: boolean
    pending: boolean
    sourceLabel?: string
    amount?: string
    editableAmount?: boolean
    amountLabel?: string
    amountDescription?: string
    amountError?: string
    title: string
    description: string
    submitLabel: string
    id: string
    approvalContent?: React.ReactNode
    onOpenChange: (open: boolean) => void
    onSubmit: (reason: string, amount?: string) => void | Promise<void>
}) {
    const [error, setError] = React.useState<string | null>(null)
    const [unknown, setUnknown] = React.useState(false)
    const submitting = React.useRef(false)
    const form = useAppForm({
        defaultValues: { reason: "", refundAmount: amount ?? "" },
        validators: {
            onChange: schema.extend({
                refundAmount: z.string().refine((value) => {
                    if (!editableAmount) return true
                    try {
                        parseDecimal(value, { maxScale: 2 })
                        return (
                            compareDecimal(value, "0", 6) > 0 &&
                            !!amount &&
                            compareDecimal(value, amount, 6) <= 0
                        )
                    } catch {
                        return false
                    }
                }, amountError),
            }),
        },
        onSubmit: async ({ value }) => {
            if (submitting.current) return
            submitting.current = true
            setError(null)
            try {
                await onSubmit(
                    value.reason.trim(),
                    editableAmount ? value.refundAmount : undefined,
                )
                setUnknown(false)
            } catch (cause) {
                setUnknown(cause instanceof SubmissionResultUnknownError)
                setError(getErrorMessage(cause, "提交未完成，请重试。"))
            } finally {
                submitting.current = false
            }
        },
    })
    const wasOpenRef = React.useRef(false)
    const initialAmountRef = React.useRef(amount)
    React.useEffect(() => {
        initialAmountRef.current = amount
    }, [amount])
    React.useEffect(() => {
        const opening = open && !wasOpenRef.current
        wasOpenRef.current = open
        if (!opening) return
        form.reset({ reason: "", refundAmount: initialAmountRef.current ?? "" })
        setError(null)
        setUnknown(false)
    }, [open, form])
    const submissionAttempts = useStore(
        form.store,
        (state) => state.submissionAttempts,
    )
    const close = () => {
        if (!pending && !submitting.current && !unknown) onOpenChange(false)
    }
    const handleCtrlEnterSubmit = React.useCallback(() => {
        if (!pending && !unknown && !submitting.current) {
            void form.handleSubmit()
        }
    }, [form, pending, unknown])
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next) close()
            }}
        >
            <DialogContent
                size="md"
                scroll="body"
                closeButtonId={`${id}-close`}
                showCloseButton={!pending && !unknown}
            >
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>{description}</DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <DialogBody className="space-y-4">
                        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg bg-muted/50 p-3 text-body-compact">
                            <dt className="text-xs font-medium text-muted-foreground">
                                原单
                            </dt>
                            <dd className="num">{sourceLabel || "—"}</dd>
                            <dt className="text-xs font-medium text-muted-foreground">
                                {editableAmount ? "可退金额上限" : "全额金额"}
                            </dt>
                            <dd>
                                {amount ? (
                                    <MoneyValue value={amount} />
                                ) : (
                                    "待核对"
                                )}
                            </dd>
                        </dl>
                        {editableAmount ? (
                            <form.AppField
                                name="refundAmount"
                                children={(field) => (
                                    <field.TextField
                                        id={`${id}-amount`}
                                        money
                                        label={amountLabel}
                                        required
                                        inputMode="decimal"
                                        disabled={pending || unknown}
                                        description={amountDescription}
                                    />
                                )}
                            />
                        ) : null}
                        <form.Field
                            name="reason"
                            children={(field) => (
                                <ReasonTextareaField
                                    id={`${id}-reason`}
                                    name={field.name}
                                    mode="utf8-bytes"
                                    max={BPM_REASON_MAX_UTF8_BYTES}
                                    label="原因"
                                    required
                                    disabled={pending || unknown}
                                    placeholder="说明本次退款或纠错原因"
                                    value={field.state.value}
                                    onChange={field.handleChange}
                                    onBlur={field.handleBlur}
                                    error={
                                        field.state.meta.isTouched ||
                                        submissionAttempts > 0
                                            ? firstFieldErrorMessage(
                                                  field.state.meta.errors,
                                              )
                                            : undefined
                                    }
                                    onCtrlEnterSubmit={handleCtrlEnterSubmit}
                                />
                            )}
                        />
                        {approvalContent}
                        {error ? (
                            <Alert variant="destructive">
                                <TriangleAlertIcon aria-hidden="true" />
                                <AlertDescription>{error}</AlertDescription>
                            </Alert>
                        ) : null}
                        {unknown ? (
                            <Alert variant="warning">
                                <CircleAlertIcon aria-hidden="true" />
                                <AlertDescription>
                                    提交结果尚未确定，请保持原内容并核对结果。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                    </DialogBody>
                    <DialogFooter>
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={pending || unknown}
                            onClick={close}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id={`${id}-submit`}
                                label={unknown ? "核对提交结果" : submitLabel}
                                loading={pending}
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
