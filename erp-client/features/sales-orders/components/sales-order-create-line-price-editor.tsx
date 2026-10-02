"use client"

import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { selectSalesReferencePrice } from "@/features/sales-orders/lib/sales-line-pricing"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 成交单价可手动修改；恢复按数量取价后才随数量切换销售参考档位。 */
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
    const isGoods = form.getFieldValue("nature") === "physical_service"
    const reference = selectSalesReferencePrice(
        line.quantity,
        line.referencePrices,
    )
    const automatic = line.pricingMode === "AUTO"

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
                                aria-describedby={
                                    invalid ? `${id}-error` : undefined
                                }
                                onBlur={field.handleBlur}
                                onChange={(event) => {
                                    form.setFieldValue(
                                        `lineItems[${rowIndex}].pricingMode`,
                                        "MANUAL",
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
            {isGoods ? (
                <div className="flex flex-wrap items-center justify-end gap-x-2 text-xs text-muted-foreground">
                    <span>
                        {automatic
                            ? reference?.tier === "bulk"
                                ? "按集采价"
                                : reference?.tier === "dropship"
                                  ? "按一件代发价"
                                  : "自动按数量"
                            : "手动成交价"}
                    </span>
                    {!automatic ? (
                        <Button
                            id={`${id}-restore-auto`}
                            type="button"
                            variant="link"
                            size="sm"
                            className="h-auto px-0 py-0 text-xs"
                            disabled={!reference}
                            title={
                                !reference
                                    ? "数量有效且已选商品后可按数量取价"
                                    : undefined
                            }
                            onClick={() => {
                                if (!reference) return
                                form.setFieldValue(
                                    `lineItems[${rowIndex}].pricingMode`,
                                    "AUTO",
                                )
                                form.setFieldValue(
                                    `lineItems[${rowIndex}].unitPriceGross`,
                                    reference.unitPriceGross,
                                )
                            }}
                        >
                            按数量取价
                        </Button>
                    ) : null}
                </div>
            ) : null}
            {isGoods && line.sku.trim() && !line.referencePrices ? (
                <p className="text-xs text-destructive" role="status">
                    参考价暂缺，请重新选择商品。
                </p>
            ) : null}
        </div>
    )
}
