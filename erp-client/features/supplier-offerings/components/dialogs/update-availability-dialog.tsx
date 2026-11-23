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
import { RequiredMark } from "@/components/ui/required-mark"
import { toast } from "@/components/ui/toast"
import { useUpdateOfferingAvailabilityMutation } from "@/features/supplier-offerings/hooks/queries"
import {
    availabilitySchema,
    errorMessage,
    idempotencyKey,
} from "@/features/supplier-offerings/lib/offering-forms"
import type {
    AvailabilityStatus,
    SupplierOfferingView,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"
import { AVAILABILITY_STATUS_LABELS } from "@/features/supplier-offerings/types"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"

type AvailabilityPayload = Omit<
    UpdateOfferingAvailabilityInput,
    "idempotency_key"
>

export function UpdateAvailabilityDialog({
    offering,
    onOpenChange,
}: {
    offering: SupplierOfferingView
    onOpenChange: (open: boolean) => void
}) {
    const mutation = useUpdateOfferingAvailabilityMutation()
    const [submitError, setSubmitError] = React.useState<string | null>(null)
    const ledgerRef = React.useRef<FormalCommandKeyLedger | null>(null)
    if (ledgerRef.current === null) {
        ledgerRef.current = new FormalCommandKeyLedger((prefix) =>
            idempotencyKey(prefix),
        )
    }

    const form = useAppForm({
        defaultValues: {
            availabilityStatus: offering.availability_status ?? "UNAVAILABLE",
            availableQuantity: offering.available_quantity ?? "",
            changeReason: "",
        },
        validators: { onSubmit: availabilitySchema },
        onSubmit: async ({ value }) => {
            setSubmitError(null)
            const ledger = ledgerRef.current!
            const slot = `availability:${offering.id}`
            const payload: AvailabilityPayload = {
                offeringId: offering.id,
                expected_version: offering.availability_version ?? undefined,
                availability_status: value.availabilityStatus,
                available_quantity: value.availableQuantity.trim() || null,
                change_reason: value.changeReason.trim(),
            }
            const command = ledger.acquire(
                slot,
                "update-offering-availability",
                payload,
            )
            try {
                await mutation.mutateAsync({
                    ...command.payload,
                    idempotency_key: command.idempotencyKey,
                })
                ledger.settle(slot, "succeeded")
                toast.add({
                    title: "当前可供情况已更新",
                    type: "success",
                })
                onOpenChange(false)
            } catch (error) {
                ledger.settle(slot, classifyFormalCommandError(error))
                setSubmitError(errorMessage(error, "可供情况保存失败"))
            }
        },
    })

    return (
        <Dialog
            open
            onOpenChange={(next) => {
                if (!next && mutation.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                size="md"
                closeButtonId="supplier-offerings-dialog-availability-close"
                showCloseButton={!mutation.isPending}
                onKeyDownCapture={(event) => {
                    if (event.key !== "Escape" || mutation.isPending) return
                    const target = event.target
                    if (
                        target instanceof Element &&
                        target.closest('[aria-expanded="true"]')
                    ) {
                        return
                    }
                    event.preventDefault()
                    event.stopPropagation()
                    onOpenChange(false)
                }}
            >
                <DialogHeader>
                    <DialogTitle>更新当前可供情况</DialogTitle>
                    <DialogDescription>
                        不修改供给价格和条款。
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
                                <Label htmlFor="supplier-offerings-dialog-availability-status">
                                    可供状态
                                    <RequiredMark />
                                </Label>
                                <OptionCombobox
                                    id="supplier-offerings-dialog-availability-status"
                                    value={field.state.value}
                                    allowClear={false}
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
                                id="supplier-offerings-dialog-availability-quantity"
                                inputMode="decimal"
                                label="当前可供数量"
                                description="留空表示供应商未提供数量上限"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="changeReason">
                        {(field) => (
                            <field.TextField
                                id="supplier-offerings-dialog-availability-reason"
                                label="变更原因"
                                placeholder="例如：供应商反馈当前可售库存变化"
                                required
                            />
                        )}
                    </form.AppField>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    id="supplier-offerings-dialog-availability-cancel"
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
                                id="supplier-offerings-dialog-availability-submit"
                                label="保存可供情况"
                                loading={mutation.isPending}
                                disabled={mutation.isPending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
