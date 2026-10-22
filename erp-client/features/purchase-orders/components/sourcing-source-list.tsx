"use client"

import { MoneyValue, QuantityValue } from "@/components/business"
import { Checkbox } from "@/components/ui/checkbox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import type { SourcingEditorProps } from "./create-sourcing/types"

const columns =
    "grid grid-cols-[minmax(0,1fr)_4.5rem_4rem_5.5rem] items-center gap-2"

/** 勾选决定提交范围；整行按钮只切换编辑对象，所有来源摘要持续可比较。 */
export function SourcingSourceList({
    form,
    row,
    activeRowKey,
    onSelect,
    errors,
}: Pick<SourcingEditorProps, "form"> & {
    row: SourcingEditorRow
    activeRowKey?: string
    onSelect: (rowKey: string) => void
    errors: ReadonlyMap<string, string[]>
}) {
    return (
        <div className="min-w-0">
            <div className="sticky top-0 z-10 flex items-center gap-2 border-b bg-background px-3 py-2 text-xs text-muted-foreground">
                <span className="w-8 shrink-0 text-center">使用</span>
                <div className={cn(columns, "flex-1 px-2")}>
                    <span>供给来源</span>
                    <span className="text-right">含税单价</span>
                    <span className="text-right">分配数量</span>
                    <span className="text-right">预计交付</span>
                </div>
            </div>
            <ul className="divide-y divide-border/60">
                {row.allocations.map(({ line, index }) => {
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
                                "flex items-center gap-2 border-l-2 border-l-transparent px-3",
                                active && "border-l-foreground bg-muted/50",
                            )}
                        >
                            <div className="flex w-8 shrink-0 justify-center">
                                <form.AppField
                                    name={`lines[${index}].selected`}
                                >
                                    {(field) => (
                                        <Checkbox
                                            id={`procurement-orders-create-row-${toAutomationIdSegment(line.rowKey)}-select`}
                                            checked={field.state.value === true}
                                            onCheckedChange={(checked) =>
                                                field.handleChange(
                                                    checked === true,
                                                )
                                            }
                                            aria-label={`本次使用 ${name} · ${route}`}
                                            data-testid={`purchase-sourcing-selected-${line.rowKey}`}
                                        />
                                    )}
                                </form.AppField>
                            </div>
                            <button
                                id={`${id}-edit`}
                                type="button"
                                aria-pressed={active}
                                aria-controls="sourcing-product-adjust-detail"
                                aria-label={`编辑 ${name} · ${route}，分配数量 ${line.quantity || "待填写"}${row.product.unit}${hasError ? "，待调整" : ""}`}
                                onClick={() => onSelect(line.rowKey)}
                                className={cn(
                                    columns,
                                    "min-w-0 flex-1 cursor-pointer rounded-sm px-2 py-3 text-left text-sm outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring",
                                    !line.selected && "text-muted-foreground",
                                )}
                            >
                                <span className="min-w-0">
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
                                <span className="text-right text-xs">
                                    {option?.sourceType === "EXISTING_STOCK" ? (
                                        "库存核算"
                                    ) : option ? (
                                        <MoneyValue
                                            value={option.unitCostGross}
                                            display="exact"
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </span>
                                <span className="text-right">
                                    {line.quantity ? (
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={null}
                                        />
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            待填写
                                        </span>
                                    )}
                                </span>
                                <span className="num text-right text-xs">
                                    {option?.sourceType === "EXISTING_STOCK"
                                        ? "—"
                                        : line.expectedDeliveryDate || "待填写"}
                                </span>
                            </button>
                        </li>
                    )
                })}
            </ul>
        </div>
    )
}
