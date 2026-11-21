"use client"

import { QuantityValue } from "@/components/business"
import { Trash2Icon } from "lucide-react"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { sourcingDeliveryAllocations } from "../lib/sourcing/delivery"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"

/** 来源列表只显示身份与分配摘要；参与范围在当前来源编辑区调整。 */
export function SourcingSourceList({
    row,
    activeRowKey,
    onSelect,
    onRemove,
    errors,
}: {
    row: SourcingEditorRow
    activeRowKey?: string
    onSelect: (rowKey: string) => void
    onRemove: (rowKey: string) => void
    errors: ReadonlyMap<string, string[]>
}) {
    return (
        <div className="min-w-0">
            <ul className="divide-y divide-border/60">
                {row.allocations.map(({ line }) => {
                    const option = findSourcingOption(row.product, line.basisId)
                    const name =
                        option?.sourceType === "EXISTING_STOCK"
                            ? option.warehouseName || option.supplierName
                            : option?.supplierName || "请选择来源"
                    const route = option
                        ? sourcingRouteLabel(option)
                        : "尚未选源"
                    const active = line.rowKey === activeRowKey
                    const id = `sourcing-source-${toAutomationIdSegment(line.rowKey)}`
                    const hasError = Boolean(errors.get(line.rowKey)?.length)
                    return (
                        <li
                            key={line.rowKey}
                            className={cn(
                                "flex items-center gap-1 border-l-2 border-l-transparent pr-2 pl-3",
                                active && "border-l-foreground bg-muted/50",
                            )}
                        >
                            <button
                                id={`${id}-edit`}
                                type="button"
                                aria-pressed={active}
                                aria-controls="sourcing-product-adjust-detail"
                                aria-label={`编辑 ${name} · ${route}，分配数量 ${line.quantity || "待填写"}${row.product.unit}${hasError ? "，待调整" : ""}`}
                                onClick={() => onSelect(line.rowKey)}
                                className={cn(
                                    "flex min-w-0 flex-1 cursor-pointer flex-wrap items-center justify-between gap-x-3 gap-y-2 rounded-sm px-2 py-4 text-left text-body-compact outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring",
                                    !line.selected && "text-muted-foreground",
                                )}
                            >
                                <span className="min-w-0 flex-1 basis-44">
                                    <span
                                        className="block truncate font-medium"
                                        title={name}
                                    >
                                        {name}
                                    </span>
                                    <span className="mt-1 block text-xs text-muted-foreground">
                                        {route}
                                        {!line.selected ? " · 暂不分配" : ""}
                                    </span>
                                    {hasError ? (
                                        <span className="block text-xs text-destructive">
                                            待调整
                                        </span>
                                    ) : null}
                                </span>
                                <span className="shrink-0 text-right text-body-compact">
                                    {line.quantity ? (
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={row.product.unit}
                                        />
                                    ) : (
                                        "待填写"
                                    )}
                                    {usesAddressQuantity(row.product, line) ? (
                                        <span className="text-xs text-muted-foreground">
                                            {" "}
                                            ·{" "}
                                            {
                                                sourcingDeliveryAllocations(
                                                    row.product,
                                                    line,
                                                ).length
                                            }{" "}
                                            个地址
                                        </span>
                                    ) : null}
                                </span>
                            </button>
                            <Button
                                id={`procurement-orders-create-row-${toAutomationIdSegment(line.rowKey)}-split-remove`}
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                className="shrink-0 text-muted-foreground hover:text-destructive"
                                disabled={row.allocations.length <= 1}
                                title={
                                    row.allocations.length <= 1
                                        ? "至少保留一个来源"
                                        : undefined
                                }
                                aria-label={`移除 ${name} · ${route}`}
                                onClick={() => onRemove(line.rowKey)}
                            >
                                <Trash2Icon />
                            </Button>
                        </li>
                    )
                })}
            </ul>
        </div>
    )
}
