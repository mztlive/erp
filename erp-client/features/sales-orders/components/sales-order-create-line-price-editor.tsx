"use client"

import { toFieldErrors } from "@/components/form"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 成交单价由销售填写；赠品行单价固定为 0，不可编辑。 */
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
        <div className="min-w-32">
            <form.AppField name={`lineItems[${rowIndex}].pricingMode`}>
                {(modeField) => (
                    <form.AppField
                        name={`lineItems[${rowIndex}].unitPriceGross`}
                    >
                        {(field) => {
                            const invalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <Field data-invalid={invalid || undefined}>
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
                                        disabled={
                                            modeField.state.value === "GIFT"
                                        }
                                        className="num min-w-24 text-right"
                                        aria-invalid={invalid || undefined}
                                        placeholder="请填写"
                                        aria-describedby={
                                            invalid ? `${id}-error` : undefined
                                        }
                                        onBlur={field.handleBlur}
                                        onChange={(event) => {
                                            modeField.handleChange("MANUAL")
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
                )}
            </form.AppField>
        </div>
    )
}

/** 赠品单独成列：勾选后成交单价置 0 且不收款，取消后需重新填写单价。 */
export function SalesOrderCreateLineGiftToggle({
    form,
    rowIndex,
}: {
    form: SalesOrderCreateFormApi
    rowIndex: number
}) {
    const line = form.getFieldValue("lineItems")[rowIndex]
    if (!line) return null
    const id = `sales-orders-create-line-${toAutomationIdSegment(line.rowKey)}-gift`

    return (
        <form.AppField name={`lineItems[${rowIndex}].pricingMode`}>
            {(modeField) => (
                <div className="flex min-h-control items-center justify-center">
                    <NativeCheckbox
                        id={id}
                        aria-label={`${line.name || "该商品"}为赠品（单价为 0，不收款）`}
                        title="赠品：单价为 0，不收款"
                        checked={modeField.state.value === "GIFT"}
                        onCheckedChange={(checked) => {
                            modeField.handleChange(checked ? "GIFT" : "MANUAL")
                            form.setFieldValue(
                                `lineItems[${rowIndex}].priceSource`,
                                undefined,
                            )
                            form.setFieldValue(
                                `lineItems[${rowIndex}].unitPriceGross`,
                                checked ? "0" : "",
                            )
                        }}
                    />
                </div>
            )}
        </form.AppField>
    )
}
