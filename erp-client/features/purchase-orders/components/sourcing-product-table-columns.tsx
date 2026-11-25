"use client"

import {
    CalendarDaysIcon,
    ChevronDownIcon,
    ChevronRightIcon,
    InfoIcon,
    MapPinIcon,
    SlidersHorizontalIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import type { ColumnDef } from "@/components/business/data-table"
import { SalesLinePriceComparison } from "@/components/business/sales-line-price-comparison"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { summarizeRowPricing } from "../lib/sourcing/pricing"
import type { SourcingSalesOrder } from "../lib/sourcing/types"
import { SourcingInlineOptionPicker } from "./sourcing-inline-option-picker"
import { SourcingInlineWarehousePicker } from "./sourcing-inline-warehouse-picker"

/** 摘要优先展示实际参与的来源；暂停商品仍可核对保留的方案。 */
function sourceFacts(row: SourcingEditorRow) {
    const selected = row.allocations.filter(({ line }) => line.selected)
    return (selected.length ? selected : row.allocations).map(
        ({ line, index }) => ({
            line,
            index,
            option: findSourcingOption(row.product, line.basisId),
        }),
    )
}

function singleValue(values: string[], multiple: string) {
    const unique = [...new Set(values)]
    return unique.length === 1 ? unique[0] : unique.length ? multiple : "—"
}

function rowStatusTone(row: SourcingEditorRow): StatusTone {
    if (row.status === "已就绪") return "success"
    if (row.status === "缺目标仓" || row.status === "部分分配") return "warning"
    if (row.status === "待调整") return "destructive"
    return "neutral"
}

export function sourcingProductColumns({
    form,
    order,
    pageRowIds,
    checkedProductIds,
    onToggleChecked,
    expandedIds,
    onToggleExpanded,
    onEdit,
    onSyncWarehouseToOthers,
}: {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    pageRowIds: readonly string[]
    checkedProductIds: ReadonlySet<string>
    onToggleChecked: (ids: readonly string[], checked: boolean) => void
    expandedIds: ReadonlySet<string>
    onToggleExpanded: (id: string) => void
    onEdit: (id: string, section?: "compare" | "addresses") => void
    onSyncWarehouseToOthers: (
        warehouseId: string,
        warehouseName: string,
        excludeSalesOrderLineId: string,
    ) => void
}): ColumnDef<SourcingEditorRow, unknown>[] {
    const pageCheckedCount = pageRowIds.filter((id) =>
        checkedProductIds.has(id),
    ).length

    return [
        {
            id: "batchSelect",
            header: () => (
                <div className="flex items-center justify-center">
                    <NativeCheckbox
                        id="sourcing-batch-select-all"
                        aria-label="全选当前页用于批量操作"
                        checked={
                            pageRowIds.length > 0 &&
                            pageCheckedCount === pageRowIds.length
                        }
                        indeterminate={
                            pageCheckedCount > 0 &&
                            pageCheckedCount < pageRowIds.length
                        }
                        disabled={pageRowIds.length === 0}
                        onClick={(event) => event.stopPropagation()}
                        onCheckedChange={(checked) =>
                            onToggleChecked(pageRowIds, checked)
                        }
                    />
                </div>
            ),
            cell: ({ row: { original: row } }) => (
                <div className="flex items-center justify-center">
                    <NativeCheckbox
                        id={`sourcing-batch-${toAutomationIdSegment(row.product.salesOrderLineId)}-select`}
                        aria-label={`批量选择 ${row.product.itemName}`}
                        checked={checkedProductIds.has(
                            row.product.salesOrderLineId,
                        )}
                        onClick={(event) => event.stopPropagation()}
                        onCheckedChange={(checked) =>
                            onToggleChecked(
                                [row.product.salesOrderLineId],
                                checked,
                            )
                        }
                    />
                </div>
            ),
            enableHiding: false,
            enableSorting: false,
            enablePinning: false,
            enableResizing: false,
            meta: { label: "批量选择", role: "selection", align: "center" },
        },
        {
            id: "product",
            header: "商品 / 交付要求",
            enableHiding: false,
            cell: ({ row: { original: row } }) => {
                const included = row.selected || row.partiallySelected
                const id = toAutomationIdSegment(row.product.salesOrderLineId)
                const addressCount = row.product.deliveryAllocations.length
                return (
                    <div
                        className={cn(
                            "min-w-44 max-w-60 space-y-1 whitespace-normal",
                            !included && "opacity-60",
                        )}
                    >
                        <span
                            className="line-clamp-1 font-medium text-foreground"
                            title={row.product.itemName}
                        >
                            {row.product.itemName}
                        </span>
                        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                            {row.product.itemSku ? (
                                <span
                                    className="num max-w-32 truncate"
                                    title={row.product.itemSku}
                                >
                                    {row.product.itemSku}
                                </span>
                            ) : null}
                            {addressCount > 0 ? (
                                <button
                                    id={`sourcing-list-${id}-addresses`}
                                    type="button"
                                    onClick={(event) => {
                                        event.stopPropagation()
                                        onEdit(
                                            row.product.salesOrderLineId,
                                            "addresses",
                                        )
                                    }}
                                    className="inline-flex cursor-pointer items-center gap-0.5 rounded-sm text-xs text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                >
                                    <MapPinIcon
                                        aria-hidden="true"
                                        className="size-3 shrink-0"
                                    />
                                    <span className="num">
                                        {addressCount} 地址
                                    </span>
                                </button>
                            ) : null}
                            {row.product.deliveryDeadline ? (
                                <span className="num inline-flex items-center gap-0.5">
                                    <CalendarDaysIcon
                                        aria-hidden="true"
                                        className="size-3 shrink-0 opacity-70"
                                    />
                                    限 {row.product.deliveryDeadline}
                                </span>
                            ) : null}
                        </div>
                    </div>
                )
            },
            meta: { label: "商品 / 交付要求", width: "reference" },
        },
        {
            id: "source",
            header: "供给方案 / 目标仓",
            enableHiding: false,
            cell: ({ row: { original: row } }) => {
                const included = row.selected || row.partiallySelected
                const productId = row.product.salesOrderLineId
                const id = toAutomationIdSegment(productId)

                if (row.allocations.length > 1) {
                    const routes = singleValue(
                        sourceFacts(row).map(({ option }) =>
                            option ? sourcingRouteLabel(option) : "尚未选源",
                        ),
                        "多方式组合",
                    )
                    return (
                        <div
                            className={cn(
                                "min-w-52 space-y-1 whitespace-normal",
                                !included && "opacity-60",
                            )}
                        >
                            <div className="flex flex-wrap items-center gap-1.5">
                                <Button
                                    id={`sourcing-list-${id}-expand`}
                                    type="button"
                                    variant="secondary"
                                    size="xs"
                                    aria-expanded={expandedIds.has(productId)}
                                    aria-controls={`sourcing-list-${id}-sources`}
                                    onClick={() => onToggleExpanded(productId)}
                                >
                                    {expandedIds.has(productId) ? (
                                        <ChevronDownIcon />
                                    ) : (
                                        <ChevronRightIcon />
                                    )}
                                    已拆 {row.allocations.length} 个来源
                                </Button>
                                <Badge
                                    variant="outline"
                                    className="h-5 px-1.5 text-tiny"
                                >
                                    {routes}
                                </Badge>
                            </div>
                        </div>
                    )
                }

                const entry = row.allocations[0]
                if (!entry) return "—"
                const { line, index } = entry
                const option = findSourcingOption(row.product, line.basisId)
                const needsWarehouse =
                    option?.sourceType === "PURCHASE" &&
                    option.fulfillmentResponsibility === "WAREHOUSE"
                const missingWarehouse =
                    included && needsWarehouse && !line.targetWarehouseId
                const inlineId = `sourcing-inline-${toAutomationIdSegment(line.rowKey)}`

                // 统计除本商品外，其他已纳入且需要入仓但未选该目标仓的行数
                const otherWarehouseTargets =
                    needsWarehouse && line.targetWarehouseId
                        ? form.state.values.lines.filter((candidate) => {
                              if (
                                  !candidate.selected ||
                                  candidate.salesOrderLineId === productId ||
                                  candidate.targetWarehouseId ===
                                      line.targetWarehouseId
                              ) {
                                  return false
                              }
                              const p = order.lines.find(
                                  (item) =>
                                      item.salesOrderLineId ===
                                      candidate.salesOrderLineId,
                              )
                              const opt = findSourcingOption(
                                  p,
                                  candidate.basisId,
                              )
                              return (
                                  opt?.sourceType === "PURCHASE" &&
                                  opt.fulfillmentResponsibility === "WAREHOUSE"
                              )
                          }).length
                        : 0

                return (
                    <div
                        className={cn(
                            "min-w-56 max-w-80 space-y-1 whitespace-normal",
                            !included && "opacity-60",
                        )}
                    >
                        <SourcingInlineOptionPicker
                            form={form}
                            order={order}
                            product={row.product}
                            line={line}
                            index={index}
                            onOpenDetail={() => onEdit(productId, "compare")}
                            warehouseSlot={
                                needsWarehouse ? (
                                    <SourcingInlineWarehousePicker
                                        id={`${inlineId}-warehouse`}
                                        value={
                                            line.targetWarehouseId || undefined
                                        }
                                        option={option}
                                        productName={row.product.itemName}
                                        missing={missingWarehouse}
                                        otherWarehouseTargets={
                                            otherWarehouseTargets
                                        }
                                        onSelect={(warehouse) => {
                                            form.setFieldValue(
                                                `lines[${index}].targetWarehouseId`,
                                                warehouse.warehouseId,
                                            )
                                            form.setFieldValue(
                                                `lines[${index}].targetWarehouseName`,
                                                warehouse.warehouseCode
                                                    ? `${warehouse.warehouseCode} · ${warehouse.warehouseName}`
                                                    : warehouse.warehouseName,
                                            )
                                        }}
                                        onSyncToOthers={(resolvedName) =>
                                            onSyncWarehouseToOthers(
                                                line.targetWarehouseId ?? "",
                                                resolvedName ||
                                                    line.targetWarehouseName ||
                                                    "",
                                                productId,
                                            )
                                        }
                                    />
                                ) : undefined
                            }
                        />
                        {needsWarehouse &&
                        otherWarehouseTargets > 0 &&
                        line.targetWarehouseId ? (
                            <div className="px-1.5">
                                <button
                                    id={`${inlineId}-sync-warehouse`}
                                    type="button"
                                    className="cursor-pointer text-tiny text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                    onClick={(event) => {
                                        event.stopPropagation()
                                        onSyncWarehouseToOthers(
                                            line.targetWarehouseId ?? "",
                                            line.targetWarehouseName ?? "",
                                            productId,
                                        )
                                    }}
                                >
                                    同步目标仓到其余 {otherWarehouseTargets}{" "}
                                    个入仓项
                                </button>
                            </div>
                        ) : null}
                    </div>
                )
            },
            meta: { label: "供给方案 / 目标仓", width: "flex" },
        },
        {
            id: "quantity",
            header: "本次 / 待分配",
            cell: ({ row: { original: row } }) => {
                const included = row.selected || row.partiallySelected
                const single =
                    row.allocations.length === 1 ? row.allocations[0] : null
                const addressDriven = single
                    ? usesAddressQuantity(row.product, single.line)
                    : false
                const canInlineQty = Boolean(single && !addressDriven)
                const isPartial = included && row.status === "部分分配"

                return (
                    <div
                        className={cn(
                            "inline-flex items-center justify-end gap-1 whitespace-nowrap",
                            !included && "opacity-60",
                        )}
                    >
                        {canInlineQty && single ? (
                            <>
                                <Input
                                    id={`sourcing-inline-${toAutomationIdSegment(single.line.rowKey)}-quantity`}
                                    size="sm"
                                    type="number"
                                    inputMode="decimal"
                                    min="0"
                                    step={
                                        sourcingQuantityStep(
                                            row.product.quantityScale,
                                        ) ?? "1"
                                    }
                                    aria-label={`行内修改 ${row.product.itemName} 分配数量（${row.product.unitName ?? ""}）`}
                                    className="num w-16 shrink-0 bg-surface-sunken text-right hover:bg-card focus-visible:bg-card"
                                    value={single.line.quantity}
                                    onChange={(event) =>
                                        form.setFieldValue(
                                            `lines[${single.index}].quantity`,
                                            event.target.value,
                                        )
                                    }
                                />
                                <span className="text-xs text-muted-foreground">
                                    /
                                </span>
                                <QuantityValue
                                    value={row.product.remainingQuantity}
                                    unit={row.product.unitName ?? ""}
                                    tone={isPartial ? "warning" : "neutral"}
                                    className="text-xs [&_.num]:text-muted-foreground"
                                />
                            </>
                        ) : (
                            <>
                                <span
                                    className={cn(
                                        "num text-body-compact font-semibold",
                                        isPartial
                                            ? "text-warning-soft-foreground"
                                            : "text-foreground",
                                    )}
                                >
                                    {included ? row.quantity : "—"}
                                </span>
                                <span className="text-xs text-muted-foreground">
                                    /
                                </span>
                                <QuantityValue
                                    value={row.product.remainingQuantity}
                                    unit={row.product.unitName ?? ""}
                                    className="text-xs [&_.num]:text-muted-foreground"
                                />
                            </>
                        )}
                    </div>
                )
            },
            meta: { label: "本次 / 待分配", align: "end", width: "default" },
        },
        {
            id: "pricing",
            header: "采购价 vs 售价（含税）",
            cell: ({ row: { original: row } }) => {
                const included = row.selected || row.partiallySelected
                const pricing = summarizeRowPricing(row)
                const hasAnyPriceNotice =
                    pricing.hasInvertedSource ||
                    pricing.salesWarnings.length > 0

                return (
                    <div
                        className={cn(
                            "flex flex-col items-end gap-0.5 whitespace-normal text-right",
                            !included && "opacity-60",
                        )}
                    >
                        <div className="flex flex-wrap items-center justify-end gap-1.5">
                            {pricing.kind === "purchase" &&
                            pricing.unitCostGross ? (
                                <span
                                    className={
                                        pricing.hasInvertedSource
                                            ? "text-destructive"
                                            : undefined
                                    }
                                >
                                    <MoneyValue
                                        display="exact"
                                        value={pricing.unitCostGross}
                                        inheritColor={pricing.hasInvertedSource}
                                    />
                                </span>
                            ) : pricing.kind === "stock" ? (
                                <span className="text-xs font-medium text-info-soft-foreground">
                                    库存核算
                                </span>
                            ) : pricing.kind === "multiple" ? (
                                <span className="text-xs font-medium text-foreground">
                                    多个价格
                                </span>
                            ) : (
                                <span className="text-xs text-muted-foreground">
                                    —
                                </span>
                            )}

                            {pricing.hasInvertedSource ? (
                                <StatusBadge
                                    tone="destructive"
                                    label={
                                        pricing.margin?.marginRateText
                                            ? `倒挂 ${pricing.margin.marginRateText}`
                                            : "倒挂"
                                    }
                                    className="h-4.5 px-1.5 text-tiny"
                                />
                            ) : pricing.margin?.marginRateText ? (
                                <span className="num text-tiny font-medium text-success-soft-foreground">
                                    {pricing.margin.marginRateText}
                                </span>
                            ) : null}
                        </div>

                        <div className="flex flex-wrap items-center justify-end gap-1 text-xs text-muted-foreground">
                            {pricing.salesUnitPriceGross ? (
                                <span>
                                    售{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={pricing.salesUnitPriceGross}
                                        inheritColor
                                    />
                                </span>
                            ) : null}
                            {row.product.salesPrices ? (
                                <Tooltip>
                                    <TooltipTrigger
                                        render={
                                            <button
                                                id={`sourcing-list-${toAutomationIdSegment(row.product.salesOrderLineId)}-price-detail`}
                                                type="button"
                                                onClick={(event) =>
                                                    event.stopPropagation()
                                                }
                                                aria-label={`查看 ${row.product.itemName} 完整价格对比`}
                                                className={cn(
                                                    "inline-flex cursor-pointer items-center gap-0.5 rounded-sm px-1 py-0.5 text-tiny transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                                    hasAnyPriceNotice
                                                        ? "bg-warning-soft/80 text-warning-soft-foreground hover:bg-warning-soft"
                                                        : "text-muted-foreground hover:bg-muted hover:text-foreground",
                                                )}
                                            />
                                        }
                                    >
                                        {hasAnyPriceNotice ? (
                                            <TriangleAlertIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                        ) : (
                                            <InfoIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                        )}
                                        <span>
                                            {hasAnyPriceNotice
                                                ? "价差提醒"
                                                : "详情"}
                                        </span>
                                    </TooltipTrigger>
                                    <TooltipContent className="w-72 p-3 text-left">
                                        <div className="space-y-2">
                                            {pricing.kind === "purchase" &&
                                            pricing.unitCostGross ? (
                                                <div className="flex items-baseline justify-between gap-3 border-b border-border/60 pb-1.5 text-xs">
                                                    <span className="font-medium">
                                                        本次选用采购价
                                                    </span>
                                                    <span
                                                        className={
                                                            pricing.hasInvertedSource
                                                                ? "text-destructive"
                                                                : undefined
                                                        }
                                                    >
                                                        <MoneyValue
                                                            display="exact"
                                                            value={
                                                                pricing.unitCostGross
                                                            }
                                                            inheritColor={
                                                                pricing.hasInvertedSource
                                                            }
                                                        />
                                                    </span>
                                                </div>
                                            ) : null}
                                            <SalesLinePriceComparison
                                                prices={row.product.salesPrices}
                                                unit={
                                                    row.product.unitName ?? ""
                                                }
                                            />
                                        </div>
                                    </TooltipContent>
                                </Tooltip>
                            ) : null}
                        </div>
                    </div>
                )
            },
            meta: {
                label: "采购价 vs 售价（含税）",
                align: "end",
                width: "reference",
            },
        },
        {
            id: "delivery",
            header: "预计交期",
            cell: ({ row: { original: row } }) => {
                const included = row.selected || row.partiallySelected
                const single =
                    row.allocations.length === 1 ? row.allocations[0] : null
                if (single) {
                    const option = findSourcingOption(
                        row.product,
                        single.line.basisId,
                    )
                    if (option?.sourceType === "EXISTING_STOCK") {
                        return (
                            <span
                                className={cn(
                                    "text-xs text-muted-foreground",
                                    !included && "opacity-60",
                                )}
                            >
                                库存出库
                            </span>
                        )
                    }
                    const deadline = row.product.deliveryDeadline
                    const validDeadline =
                        deadline && /^\d{4}-\d{2}-\d{2}$/.test(deadline)
                    const isLate = Boolean(
                        validDeadline &&
                        single.line.expectedDeliveryDate &&
                        single.line.expectedDeliveryDate > deadline,
                    )
                    return (
                        <div className={cn(!included && "opacity-60")}>
                            <DatePicker
                                id={`sourcing-inline-${toAutomationIdSegment(single.line.rowKey)}-delivery-date`}
                                size="sm"
                                clearable={false}
                                value={
                                    single.line.expectedDeliveryDate ||
                                    undefined
                                }
                                placeholder="选交期"
                                aria-invalid={
                                    (included &&
                                        (!single.line.expectedDeliveryDate ||
                                            isLate)) ||
                                    undefined
                                }
                                disabledDates={
                                    validDeadline
                                        ? {
                                              after: new Date(
                                                  `${deadline}T23:59:59`,
                                              ),
                                          }
                                        : undefined
                                }
                                onValueChange={(next) =>
                                    form.setFieldValue(
                                        `lines[${single.index}].expectedDeliveryDate`,
                                        next ?? "",
                                    )
                                }
                                className="w-full min-w-32"
                            />
                        </div>
                    )
                }

                return (
                    <span
                        className={cn(
                            "num text-xs text-muted-foreground",
                            !included && "opacity-60",
                        )}
                    >
                        {singleValue(
                            sourceFacts(row).map(({ line, option }) =>
                                option?.sourceType === "EXISTING_STOCK"
                                    ? "库存出库"
                                    : line.expectedDeliveryDate || "待填写",
                            ),
                            "多个日期",
                        )}
                    </span>
                )
            },
            meta: { label: "预计交期", numeric: true, width: "default" },
        },
        {
            id: "sales-prices",
            header: "销售与供货参考价",
            cell: ({ row: { original: row } }) =>
                row.product.salesPrices ? (
                    <SalesLinePriceComparison
                        prices={row.product.salesPrices}
                        compact
                        unit={row.product.unitName ?? ""}
                    />
                ) : (
                    "—"
                ),
            meta: { label: "销售与供货参考价", width: "reference" },
        },
        {
            id: "tax",
            header: "进项税率",
            cell: ({ row: { original: row } }) => {
                const value = singleValue(
                    sourceFacts(row).map(({ option }) =>
                        option?.sourceType === "PURCHASE"
                            ? option.inputTaxRate
                            : "—",
                    ),
                    "多个税率",
                )
                return ["—", "多个税率"].includes(value) ? (
                    value
                ) : (
                    <RateValue
                        value={multiplyFixed(value, "100", {
                            leftMaxScale: 6,
                            rightMaxScale: 0,
                            outputScale: 2,
                        })}
                        precision={2}
                    />
                )
            },
            meta: { label: "进项税率", align: "end", width: "rate" },
        },
        {
            id: "payment",
            header: "付款条件",
            cell: ({ row: { original: row } }) =>
                singleValue(
                    sourceFacts(row).map(({ option }) =>
                        option?.sourceType === "PURCHASE"
                            ? option.paymentTermLabel
                            : "—",
                    ),
                    "多种条件",
                ),
            meta: { label: "付款条件", width: "status" },
        },
        {
            id: "statusActions",
            header: "状态 / 调整",
            enableHiding: false,
            cell: ({ row: { original: row } }) => {
                const id = toAutomationIdSegment(row.product.salesOrderLineId)
                const issueText = row.issues[0]
                return (
                    <div className="flex flex-wrap items-center justify-end gap-2">
                        <div className="flex items-center justify-end gap-1.5">
                            <StatusBadge
                                label={row.status}
                                tone={rowStatusTone(row)}
                                appearance={
                                    row.status === "已就绪" ? "quiet" : "badge"
                                }
                            />
                            {issueText ? (
                                <Tooltip>
                                    <TooltipTrigger
                                        render={
                                            <button
                                                id={`sourcing-list-${id}-issue`}
                                                type="button"
                                                aria-label={`查看 ${row.product.itemName} 报错详情`}
                                                className="inline-flex cursor-pointer items-center text-destructive outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                            />
                                        }
                                    >
                                        <TriangleAlertIcon
                                            aria-hidden="true"
                                            className="size-3.5 shrink-0"
                                        />
                                    </TooltipTrigger>
                                    <TooltipContent className="max-w-72">
                                        {row.issues.join("；")}
                                    </TooltipContent>
                                </Tooltip>
                            ) : null}
                        </div>

                        <Button
                            id={`sourcing-list-${id}-edit`}
                            type="button"
                            variant="outline"
                            size="xs"
                            aria-label={`调整 ${row.product.itemName}`}
                            onClick={() => onEdit(row.product.salesOrderLineId)}
                        >
                            <SlidersHorizontalIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            调整
                        </Button>
                    </div>
                )
            },
            meta: { label: "状态 / 调整", width: "reference", align: "end" },
        },
    ]
}

export function isRowQuantityPartial(row: SourcingEditorRow): boolean {
    try {
        return (
            compareDecimal(row.quantity, row.product.remainingQuantity, 6) < 0
        )
    } catch {
        return false
    }
}
