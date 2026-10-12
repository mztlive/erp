"use client"

import { QuantityValue } from "@/components/business"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import type {
    SourcingLineInput,
    SourcingProductLine,
} from "../lib/sourcing/types"
import {
    editableDeliveryAllocations,
    remainingDeliveryAllocations,
    supplierDeliveryRequired,
} from "../lib/sourcing/delivery"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"

export function SalesDeliveryRequirements({
    product,
}: {
    product: SourcingProductLine
}) {
    return (
        <section
            className="space-y-2 border-b border-border px-4 py-4"
            aria-label="销售交付要求"
        >
            <h3 className="text-sm font-medium">销售交付要求</h3>
            {product.deliveryAllocations.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    未指定交付地址，履约前补齐。
                </p>
            ) : (
                <div className="max-h-64 space-y-2 overflow-y-auto">
                    {product.deliveryAllocations.map((row, index) => (
                        <div key={index} className="text-xs">
                            <div className="flex flex-wrap justify-between gap-2">
                                <span>
                                    {row.recipient} · {row.phone}
                                </span>
                                <QuantityValue
                                    value={row.quantity}
                                    unit={product.unit}
                                />
                            </div>
                            <p className="mt-1 break-words text-muted-foreground">
                                {row.address}
                            </p>
                        </div>
                    ))}
                </div>
            )}
        </section>
    )
}

export function SourcingDeliveryEditor({
    form,
    product,
    line,
    index,
}: {
    form: PurchaseOrderCreateFormApi
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
}) {
    if (!supplierDeliveryRequired(product, line))
        return (
            <p className="text-xs text-muted-foreground">
                客户分地址交付由仓库发货时安排。
            </p>
        )
    if (product.deliveryAllocations.length === 0)
        return (
            <p className="text-xs text-muted-foreground">
                未指定交付地址，履约前补齐。
            </p>
        )
    const remaining = remainingDeliveryAllocations(product)
    const id = `sourcing-delivery-${toAutomationIdSegment(line.rowKey)}`
    return (
        <form.AppField name={`lines[${index}].deliveryAllocations`}>
            {(field) => {
                const rows = editableDeliveryAllocations(product, {
                    ...line,
                    deliveryAllocations: field.state.value,
                })
                return (
                    <section
                        className="space-y-2 border-t border-border pt-3"
                        aria-label="本次交付安排"
                    >
                        <h4 className="text-sm font-medium">本次交付安排</h4>
                        <p className="text-xs text-muted-foreground">
                            填写本来源送往各地址的数量；不承担的地址填
                            0，合计须等于本次分配数量。
                        </p>
                        <div className="max-h-80 space-y-3 overflow-y-auto">
                            {rows.map((row, addressIndex) => (
                                <div
                                    key={addressIndex}
                                    className="grid min-w-0 grid-cols-[minmax(0,1fr)_7rem] items-start gap-3"
                                >
                                    <label
                                        htmlFor={`${id}-${addressIndex}`}
                                        className="min-w-0 text-xs leading-5"
                                    >
                                        <span className="font-medium">
                                            {row.recipient} · {row.phone}
                                        </span>
                                        <span className="block break-words text-muted-foreground">
                                            {row.address}
                                        </span>
                                        <span className="text-muted-foreground">
                                            地址剩余需求{" "}
                                            <QuantityValue
                                                value={
                                                    remaining[addressIndex]
                                                        ?.quantity ?? "0"
                                                }
                                                unit={product.unit}
                                            />
                                        </span>
                                    </label>
                                    <Input
                                        id={`${id}-${addressIndex}`}
                                        type="number"
                                        min="0"
                                        step={
                                            sourcingQuantityStep(
                                                product.quantityScale,
                                            ) ?? "1"
                                        }
                                        inputMode="decimal"
                                        className="num"
                                        value={row.quantity}
                                        aria-label={`${row.recipient}，${row.address}，本次交付数量（${product.unit}）`}
                                        onBlur={field.handleBlur}
                                        onChange={(event) =>
                                            field.handleChange(
                                                rows.map((item, itemIndex) =>
                                                    itemIndex === addressIndex
                                                        ? {
                                                              ...item,
                                                              quantity:
                                                                  event.target
                                                                      .value,
                                                          }
                                                        : item,
                                                ),
                                            )
                                        }
                                    />
                                </div>
                            ))}
                        </div>
                        {field.state.meta.errors.length > 0 ? (
                            <p
                                role="alert"
                                className="text-xs text-destructive"
                            >
                                {field.state.meta.errors.map(String).join("；")}
                            </p>
                        ) : null}
                    </section>
                )
            }}
        </form.AppField>
    )
}
