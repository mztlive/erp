"use client"

import { Input } from "@/components/ui/input"
import type { PurchaseDeliveryLineOption } from "@/features/purchase-orders/api/purchase-delivery-options"
import {
    deliveryRowExceedsAvailable,
    deliveryRowsMatchQuantity,
    deliveryRowsTotal,
} from "@/features/purchase-orders/lib/delivery-adjustment"
import type { PurchaseDeliveryAllocation } from "@/features/purchase-orders/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { fullDeliveryAddress } from "@/lib/delivery-address"

/**
 * 按销售当前收件地址调整本采购各地址的交付数量（第一期 §6.3 第 5 条）。
 * 地址只能取自销售当前收件资料，数量不超过扣除其他采购后的剩余量，合计等于采购数量。
 */
export function PurchaseDeliveryAllocationEditor({
    id,
    option,
    rows,
    removed,
    quantity,
    unit,
    disabled,
    onChange,
}: {
    id: string
    option: PurchaseDeliveryLineOption
    rows: readonly PurchaseDeliveryAllocation[]
    /** 本采购原分配中已不在销售收件资料里的地址；保存后移除。 */
    removed: readonly PurchaseDeliveryAllocation[]
    quantity: string
    unit?: string
    disabled?: boolean
    onChange: (rows: PurchaseDeliveryAllocation[]) => void
}) {
    const total = deliveryRowsTotal(rows)
    const matches = deliveryRowsMatchQuantity(rows, quantity)

    return (
        <section
            id={id}
            aria-labelledby={`${id}-title`}
            className="space-y-2 rounded-lg border border-border/70 p-3"
        >
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h4 id={`${id}-title`} className="text-sm font-medium">
                    第 {option.lineNo} 行 {option.itemName} · 收件分配
                </h4>
                <p
                    className={cn(
                        "num text-xs",
                        matches ? "text-muted-foreground" : "text-destructive",
                    )}
                >
                    合计 {total ?? "—"} / 采购数量 {quantity || "—"}
                    {unit ? ` ${unit}` : ""}
                </p>
            </div>
            <ul className="space-y-2">
                {rows.map((row) => {
                    const key = toAutomationIdSegment(
                        `${row.recipient}-${row.phone}-${row.region_code}-${row.address}`,
                    )
                    const available = option.addresses.find(
                        (address) =>
                            address.recipient === row.recipient &&
                            address.phone === row.phone &&
                            address.region_code === row.region_code &&
                            address.address === row.address,
                    )?.availableQuantity
                    const exceeds = deliveryRowExceedsAvailable(row, option)
                    return (
                        <li
                            key={key}
                            className="flex flex-wrap items-center justify-between gap-2 text-xs"
                        >
                            <span className="min-w-0 flex-1 break-words">
                                {row.recipient} · {row.phone} ·{" "}
                                {fullDeliveryAddress(row)}
                            </span>
                            <span className="flex items-center gap-2">
                                <Input
                                    id={`${id}-${key}-quantity`}
                                    className="num w-24 text-right"
                                    inputMode="decimal"
                                    value={row.quantity}
                                    disabled={disabled}
                                    aria-label={`${fullDeliveryAddress(row)} 交付数量`}
                                    aria-invalid={exceeds || undefined}
                                    onChange={(event) =>
                                        onChange(
                                            rows.map((item) =>
                                                item === row
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
                                <span
                                    className={cn(
                                        "num w-24",
                                        exceeds
                                            ? "text-destructive"
                                            : "text-muted-foreground",
                                    )}
                                >
                                    最多{" "}
                                    {available ? compactFixed(available) : "0"}
                                </span>
                            </span>
                        </li>
                    )
                })}
            </ul>
            {removed.length ? (
                <p className="text-xs text-muted-foreground">
                    以下原收件地址已不在销售收件资料中，保存后移除：
                    {removed
                        .map(
                            (row) =>
                                `${row.recipient} · ${fullDeliveryAddress(row)}`,
                        )
                        .join("；")}
                </p>
            ) : null}
            {!matches ? (
                <p className="text-xs text-destructive">
                    各地址交付数量合计须等于采购数量。
                </p>
            ) : null}
        </section>
    )
}
