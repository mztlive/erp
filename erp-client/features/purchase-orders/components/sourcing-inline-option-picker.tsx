"use client"

import { useMemo, useState, type ReactNode } from "react"
import {
    ArrowRightIcon,
    CheckIcon,
    ChevronDownIcon,
    PackageCheckIcon,
    SearchIcon,
    SlidersHorizontalIcon,
    SparklesIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import { bulkMinimumOptionState } from "../lib/sourcing/bulk-minimum"
import {
    deliverySummary,
    directOptionHasFreightExtra,
    directOptionUndeliverable,
    sourcingRouteLabel,
} from "../lib/sourcing/delivery-terms"
import { findSourcingOption } from "../lib/sourcing/options"
import {
    applySourcingOptionToLine,
    comparePurchaseCostToSalesPrice,
} from "../lib/sourcing/pricing"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
} from "../lib/sourcing/types"

/**
 * 主表行内富信息选源浮层：单源行在主表直接比价换源，无需为了改供应商弹出 2xl 大窗。
 * DOM id 统一采用 `sourcing-inline-*` 前缀，与精调弹窗内正式表单控件严格隔离。
 */
export function SourcingInlineOptionPicker({
    form,
    order,
    product,
    line,
    index,
    onOpenDetail,
    warehouseSlot,
}: {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
    onOpenDetail: () => void
    warehouseSlot?: ReactNode
}) {
    const [open, setOpen] = useState(false)
    const [query, setQuery] = useState("")

    const currentOption = findSourcingOption(product, line.basisId)
    const isStock = currentOption?.sourceType === "EXISTING_STOCK"
    const currentName = isStock
        ? currentOption.warehouseName || currentOption.supplierName
        : currentOption?.supplierName || "选择供给来源"
    const currentRoute = currentOption
        ? sourcingRouteLabel(currentOption)
        : undefined
    const currentDelivery = currentOption
        ? deliverySummary(currentOption, product)
        : ""
    const hasDeliveryAlert =
        Boolean(currentOption) &&
        !isStock &&
        Boolean(currentDelivery) &&
        !currentDelivery.startsWith("全部地址")

    const salesPrice = product.salesPrices?.unit_price_gross?.trim()
    const idPrefix = `sourcing-inline-${toAutomationIdSegment(line.rowKey)}`

    const filteredOptions = useMemo(() => {
        const keyword = query.trim().toLocaleLowerCase()
        if (!keyword) return product.options
        return product.options.filter((option) =>
            `${option.supplierName} ${option.warehouseName ?? ""} ${sourcingRouteLabel(option)} ${option.paymentTermLabel}`
                .toLocaleLowerCase()
                .includes(keyword),
        )
    }, [product.options, query])

    const lineOne = (
        <span className="flex w-full min-w-0 items-center justify-between gap-1.5">
            <span
                className={cn(
                    "min-w-0 truncate text-body-compact font-medium",
                    currentOption ? "text-foreground" : "text-destructive",
                )}
                title={currentName}
            >
                {currentName}
            </span>
            <span className="flex shrink-0 items-center gap-1">
                {product.options.length > 1 ? (
                    <span className="num text-tiny text-muted-foreground">
                        {product.options.length}源
                    </span>
                ) : null}
                <ChevronDownIcon
                    aria-hidden="true"
                    className="size-3.5 shrink-0 text-muted-foreground/70 transition-transform group-hover/opt:text-foreground group-data-[popup-open]/opt:rotate-180"
                />
            </span>
        </span>
    )

    const routeAndMeta = (
        <>
            {currentRoute ? (
                <Badge
                    variant={isStock ? "info" : "neutral"}
                    className="h-5 px-1.5 text-tiny"
                >
                    {isStock ? <PackageCheckIcon aria-hidden="true" /> : null}
                    {currentRoute}
                </Badge>
            ) : null}
            {currentOption?.inquirySource ? (
                <Badge variant="violet" className="h-5 px-1.5 text-tiny">
                    询价
                </Badge>
            ) : null}
        </>
    )

    const popover = (
        <Popover
            open={open}
            onOpenChange={(next) => {
                setOpen(next)
                if (!next) setQuery("")
            }}
        >
            <PopoverTrigger
                render={
                    <button
                        id={`${idPrefix}-option-trigger`}
                        type="button"
                        onClick={(event) => event.stopPropagation()}
                        aria-label={`切换 ${product.itemName} 供给来源，当前 ${currentName}`}
                        className={cn(
                            "group/opt flex w-full min-w-0 cursor-pointer flex-col items-start rounded-md border border-transparent px-1.5 text-left transition-colors outline-none hover:border-border/80 hover:bg-surface-sunken focus-visible:border-primary focus-visible:ring-2 focus-visible:ring-ring data-[popup-open]:border-primary/45 data-[popup-open]:bg-surface-sunken",
                            warehouseSlot ? "py-0.5" : "gap-1 py-1",
                            !currentOption &&
                                "border-dashed border-destructive-border bg-destructive-soft/30",
                        )}
                    />
                }
            >
                {lineOne}
                {warehouseSlot ? null : (
                    <span className="flex w-full flex-wrap items-center gap-1 text-xs">
                        {routeAndMeta}
                        {hasDeliveryAlert ? (
                            <span
                                className="inline-flex max-w-full items-center gap-0.5 truncate text-tiny text-warning-soft-foreground"
                                title={currentDelivery}
                            >
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="size-3 shrink-0"
                                />
                                <span className="truncate">
                                    {currentDelivery}
                                </span>
                            </span>
                        ) : null}
                    </span>
                )}
            </PopoverTrigger>

            <PopoverContent
                align="start"
                className="w-[min(34rem,calc(100vw-2rem))] gap-0 overflow-hidden rounded-lg p-0"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="flex items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-3.5 py-2.5">
                    <div className="min-w-0">
                        <p className="truncate text-body-compact font-semibold text-foreground">
                            选择供给方案 · {product.itemName}
                        </p>
                        {salesPrice ? (
                            <p className="text-xs text-muted-foreground">
                                含税售价{" "}
                                <MoneyValue
                                    display="exact"
                                    value={salesPrice}
                                    className="font-medium text-foreground"
                                />
                                {" · "}待分配{" "}
                                <QuantityValue
                                    value={product.remainingQuantity}
                                    unit={product.unitName ?? ""}
                                />
                            </p>
                        ) : null}
                    </div>
                    <Button
                        id={`${idPrefix}-option-open-detail`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="shrink-0 text-primary hover:text-primary"
                        onClick={() => {
                            setOpen(false)
                            onOpenDetail()
                        }}
                    >
                        <SlidersHorizontalIcon aria-hidden="true" />
                        拆分 / 精调
                    </Button>
                </div>

                {product.options.length >= 5 ? (
                    <div className="border-b border-border/60 px-3 py-2">
                        <div className="relative">
                            <SearchIcon
                                aria-hidden="true"
                                className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                            />
                            <Input
                                id={`${idPrefix}-option-search`}
                                aria-label="搜索供应商或仓库"
                                placeholder="搜索供应商、仓库或履约方式"
                                className="h-control-sm pr-7 pl-8 text-xs"
                                value={query}
                                onChange={(event) =>
                                    setQuery(event.target.value)
                                }
                            />
                            {query ? (
                                <Button
                                    id={`${idPrefix}-option-search-clear`}
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label="清除搜索"
                                    className="absolute top-1/2 right-0.5 -translate-y-1/2"
                                    onClick={() => setQuery("")}
                                >
                                    <XIcon />
                                </Button>
                            ) : null}
                        </div>
                    </div>
                ) : null}

                <div className="max-h-72 overflow-y-auto overscroll-contain">
                    {filteredOptions.length === 0 ? (
                        <p className="px-4 py-6 text-center text-xs text-muted-foreground">
                            没有匹配的供给方案。
                        </p>
                    ) : (
                        <ul className="divide-y divide-grid">
                            {filteredOptions.map((option) => {
                                const selected = option.basisId === line.basisId
                                const stock =
                                    option.sourceType === "EXISTING_STOCK"
                                const name = stock
                                    ? option.warehouseName ||
                                      option.supplierName
                                    : option.supplierName
                                const route = sourcingRouteLabel(option)
                                const minimum = bulkMinimumOptionState(
                                    order,
                                    option,
                                )
                                const delivery = deliverySummary(
                                    option,
                                    product,
                                )
                                const undeliverable = directOptionUndeliverable(
                                    option,
                                    product,
                                )
                                const hasExtra = directOptionHasFreightExtra(
                                    option,
                                    product,
                                )
                                const margin = !stock
                                    ? comparePurchaseCostToSalesPrice(
                                          option.unitCostGross,
                                          salesPrice,
                                      )
                                    : null
                                const maxQty = stock
                                    ? (option.sourceAvailableQuantity ??
                                      option.maxCreateQuantity)
                                    : option.maxCreateQuantity

                                return (
                                    <li key={option.basisId}>
                                        <button
                                            id={`${idPrefix}-option-pick-${toAutomationIdSegment(option.basisId)}`}
                                            type="button"
                                            disabled={minimum.disabled}
                                            onClick={() => {
                                                applySourcingOptionToLine(
                                                    form,
                                                    product,
                                                    index,
                                                    option,
                                                )
                                                setOpen(false)
                                            }}
                                            className={cn(
                                                "flex w-full cursor-pointer items-start justify-between gap-3 px-3.5 py-2.5 text-left transition-colors outline-none hover:bg-row-hover focus-visible:bg-row-hover",
                                                selected &&
                                                    "bg-sidebar-accent/70",
                                                minimum.disabled &&
                                                    "cursor-not-allowed opacity-55 hover:bg-transparent",
                                            )}
                                        >
                                            <div className="min-w-0 flex-1 space-y-1">
                                                <div className="flex flex-wrap items-center gap-1.5">
                                                    {selected ? (
                                                        <CheckIcon
                                                            aria-hidden="true"
                                                            className="size-3.5 shrink-0 text-primary"
                                                        />
                                                    ) : null}
                                                    <span className="truncate text-body-compact font-medium text-foreground">
                                                        {name}
                                                    </span>
                                                    <Badge
                                                        variant={
                                                            stock
                                                                ? "info"
                                                                : "neutral"
                                                        }
                                                        className="h-4.5 px-1.5 text-tiny"
                                                    >
                                                        {route}
                                                    </Badge>
                                                    {option.inquirySource ? (
                                                        <Badge
                                                            variant="violet"
                                                            className="h-4.5 px-1.5 text-tiny"
                                                        >
                                                            <SparklesIcon aria-hidden="true" />
                                                            询价
                                                        </Badge>
                                                    ) : null}
                                                </div>
                                                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-xs text-muted-foreground">
                                                    <span>
                                                        可供{" "}
                                                        <QuantityValue
                                                            value={maxQty}
                                                            unit={
                                                                product.unitName ??
                                                                ""
                                                            }
                                                        />
                                                    </span>
                                                    {!stock &&
                                                    option.paymentTermLabel ? (
                                                        <span>
                                                            {
                                                                option.paymentTermLabel
                                                            }
                                                        </span>
                                                    ) : null}
                                                    {!stock &&
                                                    option.expectedDeliveryDate ? (
                                                        <span className="num">
                                                            交期{" "}
                                                            {
                                                                option.expectedDeliveryDate
                                                            }
                                                        </span>
                                                    ) : null}
                                                    {delivery ? (
                                                        <span
                                                            className={cn(
                                                                undeliverable
                                                                    ? "text-destructive"
                                                                    : hasExtra ||
                                                                        !delivery.startsWith(
                                                                            "全部地址",
                                                                        )
                                                                      ? "text-warning-soft-foreground"
                                                                      : undefined,
                                                            )}
                                                        >
                                                            {delivery}
                                                        </span>
                                                    ) : null}
                                                    {minimum.label ? (
                                                        <span
                                                            className={
                                                                minimum.disabled
                                                                    ? "text-destructive"
                                                                    : undefined
                                                            }
                                                        >
                                                            {minimum.label}
                                                        </span>
                                                    ) : null}
                                                </div>
                                            </div>

                                            <div className="shrink-0 text-right">
                                                {stock ? (
                                                    <span className="text-xs text-muted-foreground">
                                                        库存核算
                                                    </span>
                                                ) : (
                                                    <>
                                                        <div className="text-body-compact font-semibold text-foreground">
                                                            <MoneyValue
                                                                display="exact"
                                                                value={
                                                                    option.unitCostGross
                                                                }
                                                            />
                                                        </div>
                                                        {margin ? (
                                                            <span
                                                                className={cn(
                                                                    "num block text-tiny font-medium",
                                                                    margin.inverted
                                                                        ? "text-destructive"
                                                                        : "text-success-soft-foreground",
                                                                )}
                                                            >
                                                                {margin.inverted
                                                                    ? `倒挂 +¥${margin.absSpread}`
                                                                    : margin.marginRateText
                                                                      ? `毛利 ${margin.marginRateText}`
                                                                      : "平价"}
                                                            </span>
                                                        ) : null}
                                                    </>
                                                )}
                                            </div>
                                        </button>
                                    </li>
                                )
                            })}
                        </ul>
                    )}
                </div>
            </PopoverContent>
        </Popover>
    )

    if (!warehouseSlot) return popover

    return (
        <div className="min-w-0 space-y-1">
            {popover}
            <div className="flex min-w-0 items-center gap-1.5 px-1.5">
                {routeAndMeta}
                <ArrowRightIcon
                    aria-hidden="true"
                    className="size-3 shrink-0 text-muted-foreground/55"
                />
                <div className="flex min-w-0 flex-1 items-center">
                    {warehouseSlot}
                </div>
            </div>
        </div>
    )
}
