"use client"

import * as React from "react"

import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { useReviseSupplierOfferingMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    buildStatusRevisionInput,
    errorMessage,
    idempotencyKey,
    statusRevisionSchema,
} from "@/features/supplier-offerings/lib/offering-forms"
import type { OfferingStatusIntent } from "@/features/supplier-offerings/lib/offering-status"
import type { SupplierOfferingView } from "@/features/supplier-offerings/types"
import { OFFERING_STATUS_LABELS } from "@/features/supplier-offerings/types"

export function ChangeOfferingStatusDialog({
    offering,
    intent,
    onOpenChange,
}: {
    offering: SupplierOfferingView
    intent: OfferingStatusIntent
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useReviseSupplierOfferingMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const currentNo = offering.current_revision_no ?? "—"
    const nextNo =
        offering.current_revision_no != null
            ? offering.current_revision_no + 1
            : "—"
    const form = useAppForm({
        defaultValues: {
            changeReason: intent.defaultReason,
        },
        validators: { onSubmit: statusRevisionSchema },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            const input = buildStatusRevisionInput(
                offering,
                intent.nextStatus,
                value.changeReason,
                idempotencyKey("revise-supplier-offering-status"),
            )
            if (!input) {
                setSubmitError(
                    "当前条款不完整或看不到价格，无法保存新版本。请改用「修订条款」。",
                )
                return
            }
            try {
                await mutation.mutateAsync(input)
                onOpenChange(false)
            } catch (error) {
                setSubmitError(errorMessage(error, "供给条款保存失败"))
            }
        },
    })

    return (
        <Dialog open onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="supplier-offerings-dialog-status-close"
                className="sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>{intent.title}</DialogTitle>
                    <DialogDescription>
                        将追加一版商业条款。价格、起订量、区域和有效期保持当前内容，条款号从
                        v{currentNo} 变为 v{nextNo}，关系状态变为
                        {OFFERING_STATUS_LABELS[intent.nextStatus]}。
                    </DialogDescription>
                </DialogHeader>
                {submitError ? (
                    <Alert variant="destructive">
                        <AlertTitle>保存失败</AlertTitle>
                        <AlertDescription>{submitError}</AlertDescription>
                    </Alert>
                ) : null}
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="changeReason">
                        {(field) => (
                            <field.TextField
                                id="supplier-offerings-dialog-status-reason"
                                label="变更原因"
                                required
                            />
                        )}
                    </form.AppField>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    id="supplier-offerings-dialog-status-cancel"
                                    type="button"
                                    variant="outline"
                                    disabled={mutation.isPending}
                                />
                            }
                        >
                            取消
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                id="supplier-offerings-dialog-status-submit"
                                label={intent.submitLabel}
                                disabled={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
