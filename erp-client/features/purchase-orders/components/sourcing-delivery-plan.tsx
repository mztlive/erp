"use client"

import { QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed, compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import type {
    SourcingLineInput,
    SourcingProductLine,
} from "../lib/sourcing/types"
import type { DeliveryTerm, PurchaseDeliveryAllocation } from "../types"
import {
    addressDeliveryTerm,
    deliveryTermLabel,
} from "../lib/sourcing/delivery-terms"
import { findSourcingOption } from "../lib/sourcing/options"
import type { SourcingSupplierOption } from "../lib/sourcing/types"
import {
    defaultDeliveryCoversQuantity,
    deliveryRequirementRows,
    editableDeliveryAllocations,
    remainingDeliveryAllocations,
    supplierDeliveryRequired,
} from "../lib/sourcing/delivery"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import { fullDeliveryAddress } from "@/lib/delivery-address"

const NO_ADDRESS_NOTE = "未指定交付地址，履约前补齐。"

const TERM_BADGE: Record<DeliveryTerm, "neutral" | "warning" | "destructive"> =
    {
        FREIGHT_INCLUDED: "neutral",
        FREIGHT_EXTRA: "warning",
        NOT_DELIVERABLE: "destructive",
    }

function DeliveryAddress({
    row,
    option,
}: {
    row: PurchaseDeliveryAllocation
    option?: SourcingSupplierOption
}) {
    const term = addressDeliveryTerm(option, row.region_code)
    return (
        <span className="min-w-0 text-xs leading-5">
            <span className="flex flex-wrap items-center gap-1.5 font-medium">
                {row.recipient} · {row.phone}
                {option && term ? (
                    <Badge variant={TERM_BADGE[term]}>
                        {deliveryTermLabel(option, term)}
                    </Badge>
                ) : null}
            </span>
            <span className="block break-words text-muted-foreground">
                {fullDeliveryAddress(row)}
            </span>
        </span>
    )
}

function positive(quantity: string): boolean {
    try {
        return compareDecimal(quantity || "0", "0", 6) > 0
    } catch {
        return false
    }
}

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
                    {NO_ADDRESS_NOTE}
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
                                    value={compactFixed(row.quantity)}
                                    unit={product.unit}
                                />
                            </div>
                            <p className="mt-1 break-words text-muted-foreground">
                                {fullDeliveryAddress(row)}
                            </p>
                        </div>
                    ))}
                </div>
            )}
        </section>
    )
}

/** 没有来源需要供应商送达时，在商品末尾只读展示销售地址要求。 */
export function SourcingDeliveryRequirementSection({
    product,
    note,
}: {
    product: SourcingProductLine
    note?: string
}) {
    const rows = deliveryRequirementRows(product)
    return (
        <section
            className="space-y-2 border-t border-border px-4 py-4"
            aria-label="交付地址"
        >
            <h4 className="text-sm font-medium">交付地址</h4>
            {rows.length === 0 ? (
                <p className="text-xs text-muted-foreground">
                    {NO_ADDRESS_NOTE}
                </p>
            ) : (
                <>
                    {note ? (
                        <p className="text-xs text-muted-foreground">{note}</p>
                    ) : null}
                    <ul className="max-h-48 space-y-2 overflow-y-auto">
                        {rows.map((row, index) => (
                            <li
                                key={index}
                                className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3"
                            >
                                <DeliveryAddress row={row} />
                                <span className="text-right text-xs leading-5">
                                    <QuantityValue
                                        value={row.quantity}
                                        unit={product.unit}
                                    />
                                    {row.remaining !== row.quantity ? (
                                        <span className="block text-muted-foreground">
                                            剩余{" "}
                                            <QuantityValue
                                                value={row.remaining}
                                                unit={product.unit}
                                            />
                                        </span>
                                    ) : null}
                                </span>
                            </li>
                        ))}
                    </ul>
                </>
            )}
        </section>
    )
}

/**
 * 本来源全量承担剩余地址需求且未手动调整时收起为只读摘要；
 * 部分承担或手动调整后逐地址填写，地址数量不再跟随分配数量。
 */
export function SourcingDeliveryEditor({
    form,
    product,
    line,
    index,
    showNotApplicable = true,
}: {
    form: PurchaseOrderCreateFormApi
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
    showNotApplicable?: boolean
}) {
    if (!supplierDeliveryRequired(product, line))
        return showNotApplicable ? (
            <p className="text-xs text-muted-foreground">
                客户分地址交付由仓库发货时安排。
            </p>
        ) : null
    if (product.deliveryAllocations.length === 0)
        return showNotApplicable ? (
            <p className="text-xs text-muted-foreground">{NO_ADDRESS_NOTE}</p>
        ) : null
    const remaining = remainingDeliveryAllocations(product)
    const coversAll = defaultDeliveryCoversQuantity(product, line)
    const option = findSourcingOption(product, line.basisId)
    const id = `sourcing-delivery-${toAutomationIdSegment(line.rowKey)}`
    return (
        <form.AppField name={`lines[${index}].deliveryAllocations`}>
            {(field) => {
                const customized = field.state.value !== undefined
                const rows = editableDeliveryAllocations(product, {
                    ...line,
                    deliveryAllocations: field.state.value,
                })
                const expanded = customized || !coversAll
                let total = "—"
                try {
                    total = compactFixed(
                        sumFixed(
                            rows.map((row) => row.quantity || "0"),
                            { maxScale: 6, outputScale: 6 },
                        ),
                    )
                } catch {
                    /* 逐地址数量校验单独显示 */
                }
                return (
                    <section
                        className="space-y-2 border-t border-border pt-3"
                        aria-label="交付地址"
                    >
                        <div className="flex min-h-8 items-center justify-between gap-2">
                            <h4 className="text-sm font-medium">交付地址</h4>
                            {!expanded ? (
                                <Button
                                    id={`${id}-customize`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={() => field.handleChange(rows)}
                                >
                                    按地址调整
                                </Button>
                            ) : customized && coversAll ? (
                                <Button
                                    id={`${id}-reset`}
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    onClick={() =>
                                        field.handleChange(undefined)
                                    }
                                >
                                    恢复默认
                                </Button>
                            ) : null}
                        </div>
                        {expanded ? (
                            <>
                                <p className="text-xs text-muted-foreground">
                                    填写本来源送往各地址的数量，合计须等于本次分配数量。
                                </p>
                                <div className="max-h-80 space-y-3 overflow-y-auto">
                                    {rows.map((row, addressIndex) => (
                                        <div
                                            key={addressIndex}
                                            className="grid min-w-0 grid-cols-[minmax(0,1fr)_7rem] items-start gap-3"
                                        >
                                            <label
                                                htmlFor={`${id}-${addressIndex}`}
                                                className="min-w-0"
                                            >
                                                <DeliveryAddress
                                                    row={row}
                                                    option={option}
                                                />
                                                <span className="text-xs text-muted-foreground">
                                                    剩余需求{" "}
                                                    <QuantityValue
                                                        value={
                                                            remaining[
                                                                addressIndex
                                                            ]?.quantity ?? "0"
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
                                                disabled={
                                                    addressDeliveryTerm(
                                                        option,
                                                        row.region_code,
                                                    ) === "NOT_DELIVERABLE"
                                                }
                                                value={row.quantity}
                                                aria-label={`${row.recipient}，${fullDeliveryAddress(row)}，本次交付数量（${product.unit}）`}
                                                onBlur={() => {
                                                    // 只聚焦未修改时不固定地址数量，仍跟随本次分配数量。
                                                    if (customized)
                                                        field.handleChange(
                                                            rows.map(
                                                                (item) => ({
                                                                    ...item,
                                                                    quantity:
                                                                        compactFixed(
                                                                            item.quantity,
                                                                        ),
                                                                }),
                                                            ),
                                                        )
                                                    field.handleBlur()
                                                }}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        rows.map(
                                                            (
                                                                item,
                                                                itemIndex,
                                                            ) =>
                                                                itemIndex ===
                                                                addressIndex
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
                                            />
                                        </div>
                                    ))}
                                </div>
                                <p className="text-right text-xs text-muted-foreground">
                                    合计{" "}
                                    <QuantityValue
                                        value={total}
                                        unit={product.unit}
                                    />{" "}
                                    / 本次分配{" "}
                                    <QuantityValue
                                        value={line.quantity || "0"}
                                        unit={product.unit}
                                    />
                                </p>
                            </>
                        ) : (
                            <>
                                <p className="text-xs text-muted-foreground">
                                    按销售要求全部送达
                                </p>
                                <ul className="max-h-48 space-y-2 overflow-y-auto">
                                    {rows.map((row, addressIndex) =>
                                        positive(row.quantity) ? (
                                            <li
                                                key={addressIndex}
                                                className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-3"
                                            >
                                                <DeliveryAddress
                                                    row={row}
                                                    option={option}
                                                />
                                                <QuantityValue
                                                    className="text-xs leading-5"
                                                    value={row.quantity}
                                                    unit={product.unit}
                                                />
                                            </li>
                                        ) : null,
                                    )}
                                </ul>
                            </>
                        )}
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
