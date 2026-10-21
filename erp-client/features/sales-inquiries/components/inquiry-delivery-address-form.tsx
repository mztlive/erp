"use client"

import * as React from "react"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FieldError } from "@/components/ui/field"
import { Label } from "@/components/ui/label"
import {
    RegionCascadeSelect,
    regionFullName,
    useRegionIndex,
} from "@/features/regions"
import { getErrorMessage } from "@/lib/api/errors"
import { allocationFieldErrors } from "../lib/address-text-import"
import type { InquiryAllocation } from "../types"

/** Dialog 内独立编辑，确认前不修改询价需求或地址身份。 */
export function InquiryDeliveryAddressDialog({
    prefix,
    initial,
    title,
    quantityScale,
    frozen,
    occupied,
    disabled,
    onClose,
    onApply,
    returnFocusId,
}: {
    prefix: string
    initial: InquiryAllocation
    title: string
    quantityScale: number
    frozen: boolean
    occupied: boolean
    disabled: boolean
    onClose: () => void
    onApply: (value: InquiryAllocation) => void
    returnFocusId: string
}) {
    const regions = useRegionIndex()
    const [failure, setFailure] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: initial,
        validators: {
            onSubmit: ({ value }) => {
                const fields = allocationFieldErrors(value, quantityScale)
                return Object.keys(fields).length ? { fields } : undefined
            },
        },
        onSubmit: ({ value }) => {
            if (disabled) return
            setFailure(null)
            try {
                onApply({
                    ...value,
                    recipient: value.recipient.trim(),
                    phone: value.phone.trim(),
                    address: value.address.trim(),
                    quantity: value.quantity.trim(),
                })
                onClose()
            } catch (error) {
                setFailure(getErrorMessage(error))
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) onClose()
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-address-dialog-close`}
                className="max-h-[90vh] overflow-y-auto sm:max-w-xl"
                finalFocus={() =>
                    document.getElementById(returnFocusId) ??
                    document.getElementById(`${prefix}-add-address`)
                }
            >
                <DialogHeader>
                    <DialogTitle>{title}</DialogTitle>
                    <DialogDescription>
                        确认后加入当前需求，取消不会修改地址。需求仍需在页面保存。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    {failure && (
                        <Alert variant="destructive">
                            <AlertDescription>{failure}</AlertDescription>
                        </Alert>
                    )}
                    <fieldset disabled={disabled} className="space-y-4">
                        {frozen && (
                            <p className="rounded-md bg-muted/40 px-3 py-2 text-xs leading-5 text-muted-foreground">
                                {occupied
                                    ? "该地址已有销售占用，收件资料不可修改，数量不能少于已占用数量。"
                                    : "已提交的收件资料保留原记录。修改收件资料请取消后选择“替换地址”。"}
                            </p>
                        )}
                        <div className="grid gap-3 sm:grid-cols-2">
                            <form.AppField name="recipient">
                                {(field) => (
                                    <field.TextField
                                        id={`${prefix}-address-dialog-recipient`}
                                        label="收件人"
                                        disabled={frozen}
                                        required
                                        className="gap-1.5"
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="phone">
                                {(field) => (
                                    <field.TextField
                                        id={`${prefix}-address-dialog-phone`}
                                        label="联系电话"
                                        type="tel"
                                        disabled={frozen}
                                        required
                                        className="gap-1.5"
                                    />
                                )}
                            </form.AppField>
                            <form.Field name="region_code">
                                {(field) => (
                                    <div className="space-y-1.5 sm:col-span-2">
                                        <Label
                                            htmlFor={`${prefix}-address-dialog-region-province`}
                                        >
                                            省 / 市 / 区县
                                        </Label>
                                        <RegionCascadeSelect
                                            idPrefix={`${prefix}-address-dialog-region`}
                                            disabled={frozen}
                                            leafOnly
                                            value={field.state.value}
                                            invalid={
                                                field.state.meta.errors.length >
                                                0
                                            }
                                            ariaDescribedBy={`${prefix}-address-dialog-region-error`}
                                            onValueChange={(code) => {
                                                form.setFieldValue(
                                                    "region_name",
                                                    code && regions.data
                                                        ? regionFullName(
                                                              regions.data,
                                                              code,
                                                          )
                                                        : "",
                                                )
                                                field.handleChange(code)
                                            }}
                                        />
                                        <FieldError
                                            id={`${prefix}-address-dialog-region-error`}
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    </div>
                                )}
                            </form.Field>
                            <form.AppField name="address">
                                {(field) => (
                                    <field.TextField
                                        id={`${prefix}-address-dialog-detail`}
                                        label="详细地址"
                                        disabled={frozen}
                                        required
                                        className="gap-1.5 sm:col-span-2"
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name="quantity">
                                {(field) => (
                                    <field.TextField
                                        id={`${prefix}-address-dialog-quantity`}
                                        label="配送数量"
                                        inputMode="decimal"
                                        required
                                        className="gap-1.5"
                                    />
                                )}
                            </form.AppField>
                        </div>
                    </fieldset>
                    <DialogFooter>
                        <Button
                            id={`${prefix}-address-dialog-cancel`}
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <Button
                            id={`${prefix}-address-dialog-confirm`}
                            type="submit"
                            disabled={disabled}
                        >
                            确认地址
                        </Button>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
