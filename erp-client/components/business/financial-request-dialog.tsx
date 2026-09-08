"use client"

import * as React from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business/values"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
    DialogFooter,
} from "@/components/ui/dialog"
import { SubmissionResultUnknownError } from "@/lib/submission-result"
import { getErrorMessage } from "@/lib/api/errors"

const schema = z.object({ reason: z.string().trim().min(1, "请填写原因") })

/** Keep source, full amount, reason and final submission in one modal. */
export function FinancialRequestDialog({
    open,
    pending,
    sourceLabel,
    amount,
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
    title: string
    description: string
    submitLabel: string
    id: string
    approvalContent?: React.ReactNode
    onOpenChange: (open: boolean) => void
    onSubmit: (reason: string) => void | Promise<void>
}) {
    const [error, setError] = React.useState<string | null>(null)
    const [unknown, setUnknown] = React.useState(false)
    const submitting = React.useRef(false)
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            if (submitting.current) return
            submitting.current = true
            setError(null)
            try {
                await onSubmit(value.reason.trim())
                setUnknown(false)
            } catch (cause) {
                setUnknown(cause instanceof SubmissionResultUnknownError)
                setError(getErrorMessage(cause, "提交未完成，请重试。"))
            } finally {
                submitting.current = false
            }
        },
    })
    React.useEffect(() => {
        if (!open) return
        form.reset({ reason: "" })
        setError(null)
        setUnknown(false)
    }, [open, sourceLabel, form])
    const close = () => {
        if (!pending && !submitting.current && !unknown) onOpenChange(false)
    }
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next) close()
            }}
        >
            <DialogContent
                className="max-h-[85dvh] w-[calc(100vw-2rem)] overflow-y-auto sm:max-w-lg"
                closeButtonId={`${id}-close`}
                showCloseButton={!pending && !unknown}
            >
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>{description}</DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 rounded-lg bg-muted/50 p-3 text-sm">
                        <dt className="text-muted-foreground">原单</dt>
                        <dd>{sourceLabel || "—"}</dd>
                        <dt className="text-muted-foreground">全额金额</dt>
                        <dd>
                            {amount ? <MoneyValue value={amount} /> : "待核对"}
                        </dd>
                    </dl>
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <field.TextareaField
                                id={`${id}-reason`}
                                label="原因"
                                required
                                disabled={pending || unknown}
                                placeholder="说明本次退款或纠错原因"
                            />
                        )}
                    />
                    {approvalContent}
                    {error ? (
                        <p role="alert" className="text-sm text-destructive">
                            {error}
                        </p>
                    ) : null}
                    {unknown ? (
                        <p
                            role="status"
                            className="text-sm text-muted-foreground"
                        >
                            提交结果尚未确定，请保持原内容并核对结果。
                        </p>
                    ) : null}
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
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
