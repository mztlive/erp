"use client"

import { toFieldErrors } from "@/components/form"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 成交单价完全由销售填写。 */
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
            <form.AppField name={`lineItems[${rowIndex}].unitPriceGross`}>
                {(field) => {
                    const invalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                    return (
                        <Field data-invalid={invalid || undefined}>
                            <FieldLabel htmlFor={id} className="sr-only">
                                含税成交单价
                            </FieldLabel>
                            <Input
                                id={id}
                                name={field.name}
                                type="number"
                                step="any"
                                value={field.state.value}
                                className="num min-w-24 text-right"
                                aria-invalid={invalid || undefined}
                                placeholder="请填写"
                                aria-describedby={
                                    invalid ? `${id}-error` : undefined
                                }
                                onBlur={field.handleBlur}
                                onChange={(event) => {
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].pricingMode`,
                                        "MANUAL",
                                    )
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].priceSource`,
                                        undefined,
                                    )
                                    field.handleChange(event.target.value)
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
        </div>
    )
}
