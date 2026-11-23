"use client"

import { useEffect, useState } from "react"
import {
    CircleAlertIcon,
    MapPinIcon,
    SearchIcon,
    Trash2Icon,
    XIcon,
} from "lucide-react"
import { QuantityValue } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { FieldError } from "@/components/ui/field"
import { Input } from "@/components/ui/input"
import { SourcingAddressPicker } from "./sourcing-address-picker"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import { compactFixed, compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
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
    const [query, setQuery] = useState("")
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
                const assignedKeys = new Set(rows.map(deliveryKey))
                const unassignedAddableCount = available.filter(
                    (row) =>
                        !assignedKeys.has(deliveryKey(row)) &&
                        addressDeliverable(option, row.region_code) &&
                        compareDecimal(row.quantity, "0", 6) > 0,
                ).length
                const total = deliveryQuantity(rows)
                const invalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                const keyword = query.trim().toLocaleLowerCase()
                const visibleRows = keyword
                    ? rows.filter((row) =>
                          `${row.recipient} ${row.phone} ${fullDeliveryAddress(row)}`
                              .toLocaleLowerCase()
                              .includes(keyword),
                      )
                    : rows
                const showSearch = rows.length >= 6 || keyword.length > 0

                return (
                    <section
                        className="border-t border-border/60"
                        aria-label="本来源交付地址"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2.5 px-4 pt-3.5 pb-2.5 @min-[560px]/sourcing:px-6">
                            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                                <h4 className="text-body-compact font-semibold text-foreground">
                                    交付地址
                                </h4>
                                <span className="num rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-medium text-muted-foreground">
                                    {rows.length}
                                    {product.deliveryAllocations.length >
                                    rows.length
                                        ? ` / ${product.deliveryAllocations.length}`
                                        : ""}
                                </span>
                                <span
                                    aria-hidden="true"
                                    className="text-muted-foreground/40"
                                >
                                    ·
                                </span>
                                <p
                                    className="text-xs text-muted-foreground"
                                    aria-live="polite"
                                >
                                    合计{" "}
                                    <QuantityValue
                                        className="font-semibold text-foreground"
                                        value={total || "—"}
                                        unit={product.unit}
                                    />
                                </p>
                                {unassignedAddableCount > 0 ? (
                                    <span className="text-xs text-warning-soft-foreground">
                                        · 另有 {unassignedAddableCount}{" "}
                                        个地址待分配
                                    </span>
                                ) : null}
                            </div>

                            <div className="flex flex-wrap items-center gap-2">
                                {showSearch ? (
                                    <div className="relative w-44">
                                        <SearchIcon
                                            aria-hidden="true"
                                            className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                                        />
                                        <Input
                                            id={`${id}-search`}
                                            aria-label="筛选交付地址"
                                            placeholder="筛选收件人或地址"
                                            className="h-control-sm bg-surface-sunken pr-7 pl-8 text-xs focus-visible:bg-card"
                                            value={query}
                                            onChange={(event) =>
                                                setQuery(event.target.value)
                                            }
                                        />
                                        {query ? (
                                            <Button
                                                id={`${id}-search-clear`}
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                aria-label="清除地址筛选"
                                                className="absolute top-1/2 right-0.5 -translate-y-1/2 text-muted-foreground"
                                                onClick={() => setQuery("")}
                                            >
                                                <XIcon />
                                            </Button>
                                        ) : null}
                                    </div>
                                ) : null}
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
                        </div>

                        {rows.length === 0 ? (
                            <div className="px-4 pb-5 @min-[560px]/sourcing:px-6">
                                <div className="flex flex-col items-center justify-center gap-1.5 rounded-lg border border-dashed border-border bg-surface-sunken/60 px-4 py-7 text-center">
                                    <MapPinIcon
                                        aria-hidden="true"
                                        className="size-4 text-muted-foreground/70"
                                    />
                                    <p className="text-body-compact font-medium text-foreground">
                                        尚未安排交付地址
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        请点击右上角「添加交付地址」，为当前来源分配收货地址与数量。
                                    </p>
                                </div>
                            </div>
                        ) : (
                            <div>
                                <div className="sticky top-0 z-10 hidden grid-cols-[minmax(0,1fr)_5.5rem_7.5rem_2rem] items-center gap-3 border-y border-border/60 bg-table-header px-4 py-1.5 text-xs font-medium text-muted-foreground @min-[520px]/sourcing:grid @min-[560px]/sourcing:px-6">
                                    <span>收件人 / 交付地址</span>
                                    <span className="text-right">可分配</span>
                                    <span className="text-right">本次分配</span>
                                    <span className="sr-only">操作</span>
                                </div>

                                {visibleRows.length === 0 ? (
                                    <div className="flex items-center justify-center gap-2 border-b border-border/60 px-6 py-7 text-xs text-muted-foreground">
                                        <span>未找到匹配的交付地址</span>
                                        <Button
                                            id={`${id}-search-reset`}
                                            type="button"
                                            variant="link"
                                            size="xs"
                                            onClick={() => setQuery("")}
                                        >
                                            清除筛选
                                        </Button>
                                    </div>
                                ) : (
                                    <div className="divide-y divide-grid border-t border-border/60 border-b @min-[520px]/sourcing:border-t-0">
                                        {visibleRows.map((row) => {
                                            const key = deliveryKey(row)
                                            const rowId = `${id}-${addressId(row)}`
                                            const cap =
                                                capacities.get(key) ?? "0"
                                            const quantity = compactFixed(
                                                row.quantity.trim(),
                                            )
                                            const term = addressDeliveryTerm(
                                                option,
                                                row.region_code,
                                            )
                                            const fullAddress =
                                                fullDeliveryAddress(row)
                                            let error =
                                                sourcingUnitQuantityError(
                                                    quantity,
                                                    product.quantityScale,
                                                )
                                            if (quantity === "0")
                                                error = undefined
                                            if (
                                                !error &&
                                                compareDecimal(
                                                    quantity,
                                                    cap,
                                                    6,
                                                ) > 0
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
                                                    className={cn(
                                                        "grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1.5 px-4 py-2 transition-colors hover:bg-row-hover @min-[520px]/sourcing:grid-cols-[minmax(0,1fr)_5.5rem_7.5rem_2rem] @min-[560px]/sourcing:px-6",
                                                        error &&
                                                            "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                                                    )}
                                                >
                                                    <label
                                                        htmlFor={`${rowId}-quantity`}
                                                        className="col-span-2 min-w-0 cursor-pointer @min-[520px]/sourcing:col-span-1"
                                                    >
                                                        <span className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                                            <span className="text-body-compact font-medium text-foreground">
                                                                {row.recipient}
                                                            </span>
                                                            <span className="num text-xs text-muted-foreground">
                                                                {row.phone}
                                                            </span>
                                                            {option &&
                                                            term ===
                                                                "FREIGHT_EXTRA" ? (
                                                                <Badge variant="warning">
                                                                    {deliveryTermLabel(
                                                                        option,
                                                                        term,
                                                                    )}
                                                                </Badge>
                                                            ) : null}
                                                            {option &&
                                                            term ===
                                                                "NOT_DELIVERABLE" ? (
                                                                <Badge variant="destructive">
                                                                    {deliveryTermLabel(
                                                                        option,
                                                                        term,
                                                                    )}
                                                                </Badge>
                                                            ) : null}
                                                        </span>
                                                        <span className="mt-0.5 block break-words text-xs text-muted-foreground">
                                                            {fullAddress}
                                                        </span>
                                                    </label>

                                                    <div className="flex items-baseline gap-1 text-xs text-muted-foreground @min-[520px]/sourcing:justify-end">
                                                        <span className="@min-[520px]/sourcing:hidden">
                                                            可分配
                                                        </span>
                                                        <QuantityValue
                                                            value={cap}
                                                            unit={product.unit}
                                                            className="text-xs [&_.num]:text-muted-foreground"
                                                        />
                                                    </div>

                                                    <div className="flex items-center justify-end gap-1.5">
                                                        <Input
                                                            id={`${rowId}-quantity`}
                                                            className="num h-control-sm w-20 bg-surface-sunken text-right hover:bg-card focus-visible:bg-card"
                                                            type="number"
                                                            min="0"
                                                            max={cap}
                                                            step={
                                                                sourcingQuantityStep(
                                                                    product.quantityScale,
                                                                ) ?? "1"
                                                            }
                                                            inputMode="decimal"
                                                            aria-label={`${row.recipient}，${fullAddress}，本次交付数量（${product.unit}）`}
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
                                                                        (
                                                                            item,
                                                                        ) =>
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
                                                                        (
                                                                            item,
                                                                        ) => {
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
                                                        <span className="min-w-4 shrink-0 text-xs text-muted-foreground">
                                                            {product.unit}
                                                        </span>
                                                    </div>

                                                    <Button
                                                        id={`${rowId}-remove`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-xs"
                                                        className="justify-self-end text-muted-foreground/70 hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"
                                                        title="移除该地址"
                                                        aria-label={`移除 ${row.recipient} 的交付地址`}
                                                        onClick={() => {
                                                            updateRows(
                                                                rows.filter(
                                                                    (item) =>
                                                                        deliveryKey(
                                                                            item,
                                                                        ) !==
                                                                        key,
                                                                ),
                                                            )
                                                            field.handleBlur()
                                                        }}
                                                    >
                                                        <Trash2Icon />
                                                    </Button>

                                                    {error ? (
                                                        <p
                                                            id={`${rowId}-error`}
                                                            role="alert"
                                                            className="col-span-full flex items-center gap-1.5 pt-0.5 text-xs text-destructive"
                                                        >
                                                            <CircleAlertIcon
                                                                aria-hidden="true"
                                                                className="size-3.5 shrink-0"
                                                            />
                                                            <span>{error}</span>
                                                        </p>
                                                    ) : null}
                                                </div>
                                            )
                                        })}
                                    </div>
                                )}
                            </div>
                        )}

                        {invalid ? (
                            <div className="px-4 py-2.5 @min-[560px]/sourcing:px-6">
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </div>
                        ) : null}
                    </section>
                )
            }}
        </form.AppField>
    )
}
