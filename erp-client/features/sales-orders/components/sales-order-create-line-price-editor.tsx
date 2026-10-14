"use client"

import { toFieldErrors } from "@/components/form"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 成交单价由销售填写；勾选赠品时单价固定为 0。 */
export function SalesOrderCreateLinePriceEditor({
    form,
    rowIndex,
}: {
    form: SalesOrderCreateFormApi
    rowIndex: number
}) {
    const line = form.getFieldValue("lineItems")[rowIndex]
    if (!line) return null
    const id = `sales-orders-create-line-${toAutomationIdSegment(line.rowKey)}-unit-price`

    return (
        <div className="min-w-32 space-y-1">
            <form.AppField name={`lineItems[${rowIndex}].pricingMode`}>
                {(modeField) => {
                    const gift = modeField.state.value === "GIFT"
                    return (
                        <>
                            <form.AppField
                                name={`lineItems[${rowIndex}].unitPriceGross`}
                            >
                                {(field) => {
                                    const invalid =
                                        field.state.meta.isTouched &&
                                        !field.state.meta.isValid
                                    return (
                                        <Field
                                            data-invalid={invalid || undefined}
                                        >
                                            <FieldLabel
                                                htmlFor={id}
                                                className="sr-only"
                                            >
                                                含税成交单价
                                            </FieldLabel>
                                            <Input
                                                id={id}
                                                name={field.name}
                                                type="number"
                                                step="any"
                                                value={field.state.value}
                                                disabled={gift}
                                                className="num min-w-24 text-right"
                                                aria-invalid={
                                                    invalid || undefined
                                                }
                                                placeholder="请填写"
                                                aria-describedby={
                                                    invalid
                                                        ? `${id}-error`
                                                        : undefined
                                                }
                                                onBlur={field.handleBlur}
                                                onChange={(event) => {
                                                    modeField.handleChange(
                                                        "MANUAL",
                                                    )
                                                    form.setFieldValue(
                                                        `lineItems[${rowIndex}].priceSource`,
                                                        undefined,
                                                    )
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }}
                                            />
                                            {invalid ? (
                                                <FieldError
                                                    id={`${id}-error`}
                                                    errors={toFieldErrors(
                                                        field.state.meta.errors,
                                                    )}
                                                />
                                            ) : null}
                                        </Field>
                                    )
                                }}
                            </form.AppField>
                            <Label
                                htmlFor={`${id}-gift`}
                                className="flex items-center gap-1.5 text-xs font-normal text-muted-foreground"
                            >
                                <Checkbox
                                    id={`${id}-gift`}
                                    checked={gift}
                                    onCheckedChange={(checked) => {
                                        modeField.handleChange(
                                            checked === true
                                                ? "GIFT"
                                                : "MANUAL",
                                        )
                                        form.setFieldValue(
                                            `lineItems[${rowIndex}].priceSource`,
                                            undefined,
                                        )
                                        form.setFieldValue(
                                            `lineItems[${rowIndex}].unitPriceGross`,
                                            checked === true ? "0" : "",
                                        )
                                    }}
                                />
                                赠品（单价为 0，不收款）
                            </Label>
                        </>
                    )
                }}
            </form.AppField>
        </div>
    )
}
