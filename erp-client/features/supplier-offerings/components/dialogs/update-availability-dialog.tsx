"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
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
import { Label } from "@/components/ui/label"
import { useUpdateOfferingAvailabilityMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    availabilitySchema,
    errorMessage,
    idempotencyKey,
} from "@/features/supplier-offerings/lib/offering-forms"
import type {
    AvailabilityStatus,
    SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import { AVAILABILITY_STATUS_LABELS } from "@/features/supplier-offerings/types"

export function UpdateAvailabilityDialog({
    offering,
    onOpenChange,
}: {
    offering: SupplierOfferingView
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useUpdateOfferingAvailabilityMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: {
            availabilityStatus: offering.availability_status ?? "UNAVAILABLE",
            availableQuantity: offering.available_quantity ?? "",
            changeReason: "更新当前可供情况",
        },
        validators: { onSubmit: availabilitySchema },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            try {
                await mutation.mutateAsync({
                    offeringId: offering.id,
                    expected_version:
                        offering.availability_version ?? undefined,
                    availability_status: value.availabilityStatus,
                    available_quantity: value.availableQuantity.trim() || null,
                    change_reason: value.changeReason.trim(),
                    idempotency_key: idempotencyKey(
                        "update-offering-availability",
                    ),
                })
                onOpenChange(false)
            } catch (error) {
                setSubmitError(errorMessage(error, "可供情况保存失败"))
            }
        },
    })

    return (
        <Dialog open onOpenChange={onOpenChange}>
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>更新当前可供情况</DialogTitle>
                    <DialogDescription>
                        该信息独立于商业条款版本，可由人工或供应商接口高频更新。
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
                    <form.AppField name="availabilityStatus">
                        {(field) => (
                            <div className="space-y-1.5">
                                <Label>可供状态 *</Label>
                                <OptionCombobox
                                    value={field.state.value}
                                    onValueChange={(value) =>
                                        field.handleChange(
                                            (value ??
                                                "UNAVAILABLE") as AvailabilityStatus,
                                        )
                                    }
                                    options={Object.entries(
                                        AVAILABILITY_STATUS_LABELS,
                                    ).map(([value, label]) => ({
                                        value,
                                        label,
                                    }))}
                                    className="w-full"
                                />
                            </div>
                        )}
                    </form.AppField>
                    <form.AppField name="availableQuantity">
                        {(field) => (
                            <field.TextField
                                label="当前可供数量"
                                description="留空表示供应商未提供数量上限"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="changeReason">
                        {(field) => <field.TextField label="变更原因 *" />}
                    </form.AppField>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
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
                                label="保存可供情况"
                                disabled={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
