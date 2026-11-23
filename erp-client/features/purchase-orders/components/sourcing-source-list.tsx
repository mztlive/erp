"use client"

import { Trash2Icon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { sourcingDeliveryAllocations } from "../lib/sourcing/delivery"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"

/** 来源列表展示身份、路线、数量与单价摘要，便于多来源横向比较。 */
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
    const canRemove = row.allocations.length > 1
    return (
        <ul className="space-y-2 p-2.5">
            {row.allocations.map(({ line }) => {
                const option = findSourcingOption(row.product, line.basisId)
                const isStock = option?.sourceType === "EXISTING_STOCK"
                const name = isStock
                    ? option.warehouseName || option.supplierName
                    : option?.supplierName || "请选择来源"
                const route = option ? sourcingRouteLabel(option) : "尚未选源"
                const active = line.rowKey === activeRowKey
                const id = `sourcing-source-${toAutomationIdSegment(line.rowKey)}`
                const hasError = Boolean(errors.get(line.rowKey)?.length)
                const addressCount = usesAddressQuantity(row.product, line)
                    ? sourcingDeliveryAllocations(row.product, line).length
                    : null

                return (
                    <li
                        key={line.rowKey}
                        className={cn(
                            "group/source relative rounded-lg border transition-colors",
                            active
                                ? hasError
                                    ? "border-destructive/55 bg-card shadow-xs ring-1 ring-destructive/20"
                                    : "border-primary/45 bg-card shadow-xs ring-1 ring-primary/15"
                                : hasError
                                  ? "border-destructive/40 bg-destructive-soft/25 hover:border-destructive/60 hover:bg-card"
                                  : "border-border/60 bg-card/70 hover:border-border hover:bg-card",
                            !line.selected && "border-dashed opacity-70",
                        )}
                    >
                        <button
                            id={`${id}-edit`}
                            type="button"
                            aria-pressed={active}
                            aria-controls="sourcing-product-adjust-detail"
                            aria-label={`编辑 ${name} · ${route}，分配数量 ${line.quantity || "待填写"}${row.product.unit}${hasError ? "，待调整" : ""}`}
                            onClick={() => onSelect(line.rowKey)}
                            className="block w-full cursor-pointer rounded-lg px-3 py-2.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            <div className="pr-6">
                                <span
                                    className={cn(
                                        "block truncate text-body-compact font-medium",
                                        active
                                            ? "text-foreground"
                                            : "text-foreground/90",
                                        !line.selected &&
                                            "text-muted-foreground",
                                    )}
                                    title={name}
                                >
                                    {name}
                                </span>
                                <div className="mt-1 flex flex-wrap items-center gap-1.5">
                                    <Badge
                                        variant={
                                            isStock
                                                ? "info"
                                                : option
                                                  ? "neutral"
                                                  : "outline"
                                        }
                                        className="h-5 px-1.5"
                                    >
                                        {route}
                                    </Badge>
                                    {!line.selected ? (
                                        <span className="text-xs text-muted-foreground">
                                            暂不分配
                                        </span>
                                    ) : null}
                                    {hasError ? (
                                        <StatusBadge
                                            tone="destructive"
                                            appearance="text"
                                            label="待调整"
                                            className="h-5"
                                        />
                                    ) : null}
                                </div>
                            </div>

                            <div className="mt-2.5 flex items-baseline justify-between gap-2 border-t border-border/50 pt-2">
                                <span className="min-w-0 text-body-compact">
                                    {line.quantity ? (
                                        <QuantityValue
                                            className="font-semibold"
                                            value={line.quantity}
                                            unit={row.product.unit}
                                        />
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            待填写
                                        </span>
                                    )}
                                    {addressCount !== null ? (
                                        <span className="text-xs text-muted-foreground">
                                            {" "}
                                            · {addressCount} 个地址
                                        </span>
                                    ) : null}
                                </span>
                                <span className="shrink-0 text-right text-xs text-muted-foreground">
                                    {isStock ? (
                                        "库存核算"
                                    ) : option ? (
                                        <MoneyValue
                                            display="exact"
                                            value={option.unitCostGross}
                                            className="text-xs font-medium"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </span>
                            </div>
                        </button>

                        <Button
                            id={`procurement-orders-create-row-${toAutomationIdSegment(line.rowKey)}-split-remove`}
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            className={cn(
                                "absolute top-2 right-2 text-muted-foreground/70 hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground",
                                !canRemove && "opacity-30",
                            )}
                            disabled={!canRemove}
                            title={
                                !canRemove ? "至少保留一个来源" : "移除此来源"
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
    )
}
