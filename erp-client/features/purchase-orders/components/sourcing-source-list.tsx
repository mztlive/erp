"use client"

import { Trash2Icon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingDeliveryAllocations } from "../lib/sourcing/delivery"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"

/**
 * 顶部横向来源条：单来源时紧凑一行不浪费弹窗宽度，多来源时横向并排对比数量、单价与异常。
 */
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
        <ul className="flex min-w-0 flex-wrap items-stretch gap-2">
            {row.allocations.map(({ line }, idx) => {
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
                            "group/source relative flex min-w-0 items-center rounded-lg border transition-colors",
                            canRemove ? "max-w-72" : "max-w-full",
                            active
                                ? hasError
                                    ? "border-destructive/60 bg-card shadow-xs ring-1 ring-destructive/20"
                                    : "border-primary/50 bg-card shadow-xs ring-1 ring-primary/15"
                                : hasError
                                  ? "border-destructive/40 bg-destructive-soft/25 hover:border-destructive/60 hover:bg-card"
                                  : "border-border/70 bg-card/70 hover:border-border hover:bg-card",
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
                            className={cn(
                                "flex min-w-0 cursor-pointer items-center gap-2 rounded-lg px-2.5 py-1.5 text-left outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                canRemove && "pr-8",
                            )}
                        >
                            {canRemove ? (
                                <span className="num shrink-0 rounded-sm bg-muted px-1.5 py-0.5 text-tiny font-semibold text-muted-foreground">
                                    源{idx + 1}
                                </span>
                            ) : null}
                            <div className="min-w-0">
                                <div className="flex items-center gap-1.5">
                                    <span
                                        className={cn(
                                            "truncate text-body-compact font-medium",
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
                                    <Badge
                                        variant={
                                            isStock
                                                ? "info"
                                                : option
                                                  ? "neutral"
                                                  : "outline"
                                        }
                                        className="h-4.5 shrink-0 px-1.5 text-tiny"
                                    >
                                        {route}
                                    </Badge>
                                    {!line.selected ? (
                                        <span className="shrink-0 text-tiny text-muted-foreground">
                                            暂不用
                                        </span>
                                    ) : null}
                                    {hasError ? (
                                        <StatusBadge
                                            tone="destructive"
                                            appearance="text"
                                            label="待调整"
                                            className="h-4.5 shrink-0 text-tiny"
                                        />
                                    ) : null}
                                </div>
                                <div className="mt-0.5 flex flex-wrap items-baseline gap-x-2 text-xs text-muted-foreground">
                                    <span>
                                        {line.quantity ? (
                                            <QuantityValue
                                                className="font-semibold text-foreground"
                                                value={line.quantity}
                                                unit={row.product.unit}
                                            />
                                        ) : (
                                            "待填写"
                                        )}
                                        {addressCount !== null
                                            ? ` · ${addressCount} 地址`
                                            : ""}
                                    </span>
                                    <span>·</span>
                                    <span>
                                        {isStock ? (
                                            "库存核算"
                                        ) : option ? (
                                            <MoneyValue
                                                display="exact"
                                                value={option.unitCostGross}
                                                className="text-xs font-medium text-foreground/85"
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </span>
                                </div>
                            </div>
                        </button>

                        {canRemove ? (
                            <Button
                                id={`procurement-orders-create-row-${toAutomationIdSegment(line.rowKey)}-split-remove`}
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                className="absolute top-1/2 right-1.5 -translate-y-1/2 text-muted-foreground/70 hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"
                                title="移除此来源"
                                aria-label={`移除 ${name} · ${route}`}
                                onClick={() => onRemove(line.rowKey)}
                            >
                                <Trash2Icon />
                            </Button>
                        ) : null}
                    </li>
                )
            })}
        </ul>
    )
}
