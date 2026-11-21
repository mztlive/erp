"use client"

import { useEffect } from "react"
import { Trash2Icon } from "lucide-react"
import { QuantityValue } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { SourcingAddressPicker } from "./sourcing-address-picker"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import {
    sourcingQuantityStep,
    sourcingUnitQuantityError,
} from "../lib/sourcing-quantity"
import { deliveryKey } from "../lib/sourcing/delivery"
import {
    availableDeliveryRows,
    deliveryQuantity,
    selectedDeliveryRows,
} from "../lib/sourcing/delivery-editor"
import {
    addressDeliverable,
    addressDeliveryTerm,
    deliveryTermLabel,
} from "../lib/sourcing/delivery-terms"
import { findSourcingOption } from "../lib/sourcing/options"
import type {
    SourcingLineInput,
    SourcingProductLine,
} from "../lib/sourcing/types"
import type { PurchaseDeliveryAllocation } from "../types"

/** 地址以收件资料作为稳定业务键，中文资料也保留在自动化 ID 中。 */
function addressId(row: PurchaseDeliveryAllocation): string {
    return toAutomationIdSegment(encodeURIComponent(deliveryKey(row)))
}

/** 仅编辑本来源承担的地址；地址与合计一次写入共享草稿。 */
export function SourcingAddressEditor({
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
    const id = `sourcing-delivery-${toAutomationIdSegment(line.rowKey)}`
    const option = findSourcingOption(product, line.basisId)

    // 打开来源或切换方案时冻结原默认分配，并兼容旧草稿中的零数量地址。
    useEffect(() => {
        const current = form.state.values.lines[index]
        if (!current || current.rowKey !== line.rowKey) return
        const rows = selectedDeliveryRows(product, current)
        const quantity = deliveryQuantity(rows)
        if (
            quantity !== current.quantity ||
            JSON.stringify(rows) !== JSON.stringify(current.deliveryAllocations)
        ) {
            form.setFieldValue(`lines[${index}]`, {
                ...current,
                deliveryAllocations: rows,
                quantity,
            })
        }
    }, [form, index, line.rowKey, line.basisId, product])

    const updateRows = (rows: PurchaseDeliveryAllocation[]) => {
        const current = form.state.values.lines[index]
        if (!current || current.rowKey !== line.rowKey) return
        form.setFieldValue(`lines[${index}]`, {
            ...current,
            deliveryAllocations: rows,
            quantity: deliveryQuantity(rows),
        })
    }

    return (
        <form.AppField name={`lines[${index}]`}>
            {(field) => {
                const rows: PurchaseDeliveryAllocation[] =
                    field.state.value.deliveryAllocations ??
                    selectedDeliveryRows(product, line)
                const available = availableDeliveryRows(
                    product,
                    line,
                    form.state.values.lines,
                )
                const capacities = new Map(
                    available.map((row) => [deliveryKey(row), row.quantity]),
                )
                const total = deliveryQuantity(rows)
                const invalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                return (
                    <section
                        className="space-y-2 border-t border-border pt-3"
                        aria-label="本来源交付地址"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-3">
                            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                                <h4 className="text-body-compact font-medium">
                                    交付地址 · {rows.length}
                                </h4>
                                <p
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    合计{" "}
                                    <QuantityValue
                                        value={total || "—"}
                                        unit={product.unit}
                                    />
                                </p>
                            </div>
                            <SourcingAddressPicker
                                key={line.basisId}
                                id={id}
                                unit={product.unit}
                                option={option}
                                available={available}
                                rows={rows}
                                onAdd={(added) => {
                                    updateRows([...rows, ...added])
                                    field.handleBlur()
                                }}
                            />
                        </div>
                        {rows.length === 0 ? (
                            <p className="border-y border-dashed border-border py-4 text-center text-xs text-muted-foreground">
                                尚未安排交付地址，请添加本来源负责的地址。
                            </p>
                        ) : (
                            <div>
                                <div className="divide-y divide-border/60 border-t border-border/60">
                                    {rows.map((row) => {
                                        const key = deliveryKey(row)
                                        const rowId = `${id}-${addressId(row)}`
                                        const cap = capacities.get(key) ?? "0"
                                        const quantity = compactFixed(
                                            row.quantity.trim(),
                                        )
                                        const term = addressDeliveryTerm(
                                            option,
                                            row.region_code,
                                        )
                                        let error = sourcingUnitQuantityError(
                                            quantity,
                                            product.quantityScale,
                                        )
                                        if (quantity === "0") error = undefined
                                        if (
                                            !error &&
                                            compareDecimal(quantity, cap, 6) > 0
                                        )
                                            error = `超过可分配数量 ${cap} ${product.unit}，请减少数量或调整其他来源`
                                        if (
                                            !addressDeliverable(
                                                option,
                                                row.region_code,
                                            )
                                        )
                                            error =
                                                "该来源不配送此地址，请移除地址或更换来源"
                                        return (
                                            <div
                                                key={key}
                                                className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-2 py-2.5 @min-[560px]/sourcing:grid-cols-[minmax(0,1fr)_7rem_4.5rem]"
                                            >
                                                <label
                                                    htmlFor={`${rowId}-quantity`}
                                                    className="col-span-2 min-w-0 space-y-1 @min-[560px]/sourcing:col-span-1"
                                                >
                                                    <span className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
                                                        <span className="text-body-compact font-medium">
                                                            {row.recipient} ·{" "}
                                                            {row.phone}
                                                        </span>
                                                        <span className="text-xs text-muted-foreground">
                                                            最多可分配{" "}
                                                            <QuantityValue
                                                                value={cap}
                                                                unit={
                                                                    product.unit
                                                                }
                                                            />
                                                            {option &&
                                                            term ===
                                                                "FREIGHT_EXTRA"
                                                                ? ` · ${deliveryTermLabel(option, term)}`
                                                                : ""}
                                                        </span>
                                                    </span>
                                                    <span className="block break-words text-body-compact text-muted-foreground">
                                                        {fullDeliveryAddress(
                                                            row,
                                                        )}
                                                    </span>
                                                </label>
                                                <div className="flex items-center gap-2">
                                                    <Input
                                                        id={`${rowId}-quantity`}
                                                        className="num h-8 w-20"
                                                        type="number"
                                                        min="0"
                                                        max={cap}
                                                        step={
                                                            sourcingQuantityStep(
                                                                product.quantityScale,
                                                            ) ?? "1"
                                                        }
                                                        inputMode="decimal"
                                                        aria-label={`${row.recipient}，${fullDeliveryAddress(row)}，本次交付数量（${product.unit}）`}
                                                        aria-invalid={Boolean(
                                                            error,
                                                        )}
                                                        aria-describedby={
                                                            error
                                                                ? `${rowId}-error`
                                                                : undefined
                                                        }
                                                        value={row.quantity}
                                                        onChange={(event) =>
                                                            updateRows(
                                                                rows.map(
                                                                    (item) =>
                                                                        deliveryKey(
                                                                            item,
                                                                        ) ===
                                                                        key
                                                                            ? {
                                                                                  ...item,
                                                                                  quantity:
                                                                                      event
                                                                                          .target
                                                                                          .value,
                                                                              }
                                                                            : item,
                                                                ),
                                                            )
                                                        }
                                                        onBlur={() => {
                                                            const current =
                                                                form.state
                                                                    .values
                                                                    .lines[
                                                                    index
                                                                ]
                                                                    ?.deliveryAllocations ??
                                                                []
                                                            const normalized =
                                                                current.flatMap(
                                                                    (item) => {
                                                                        if (
                                                                            deliveryKey(
                                                                                item,
                                                                            ) !==
                                                                            key
                                                                        )
                                                                            return [
                                                                                item,
                                                                            ]
                                                                        const value =
                                                                            compactFixed(
                                                                                item.quantity.trim(),
                                                                            )
                                                                        try {
                                                                            if (
                                                                                compareDecimal(
                                                                                    value,
                                                                                    "0",
                                                                                    6,
                                                                                ) ===
                                                                                0
                                                                            )
                                                                                return []
                                                                            return [
                                                                                {
                                                                                    ...item,
                                                                                    quantity:
                                                                                        value,
                                                                                },
                                                                            ]
                                                                        } catch {
                                                                            return [
                                                                                item,
                                                                            ]
                                                                        }
                                                                    },
                                                                )
                                                            updateRows(
                                                                normalized,
                                                            )
                                                            field.handleBlur()
                                                        }}
                                                    />
                                                    <span className="shrink-0 text-body-compact">
                                                        {product.unit}
                                                    </span>
                                                </div>
                                                <Button
                                                    id={`${rowId}-remove`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="sm"
                                                    className="justify-self-end text-muted-foreground"
                                                    aria-label={`移除 ${row.recipient} 的交付地址`}
                                                    onClick={() => {
                                                        updateRows(
                                                            rows.filter(
                                                                (item) =>
                                                                    deliveryKey(
                                                                        item,
                                                                    ) !== key,
                                                            ),
                                                        )
                                                        field.handleBlur()
                                                    }}
                                                >
                                                    <Trash2Icon />
                                                    移除
                                                </Button>
                                                {error ? (
                                                    <p
                                                        id={`${rowId}-error`}
                                                        role="alert"
                                                        className="col-span-full text-xs text-destructive"
                                                    >
                                                        {error}
                                                    </p>
                                                ) : null}
                                            </div>
                                        )
                                    })}
                                </div>
                            </div>
                        )}
                        {invalid ? (
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        ) : null}
                    </section>
                )
            }}
        </form.AppField>
    )
}
