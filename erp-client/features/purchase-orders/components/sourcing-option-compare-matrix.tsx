"use client"

import { useMemo, useState } from "react"
import {
    ArrowUpDownIcon,
    CheckCircle2Icon,
    CircleIcon,
    PackageCheckIcon,
    PlusCircleIcon,
    SparklesIcon,
    TriangleAlertIcon,
} from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { bulkMinimumOptionState } from "../lib/sourcing/bulk-minimum"
import {
    deliverySummary,
    directOptionHasFreightExtra,
    directOptionUndeliverable,
    sourcingRouteLabel,
} from "../lib/sourcing/delivery-terms"
import { comparePurchaseCostToSalesPrice } from "../lib/sourcing/pricing"
import {
    sourceRouteBadgeVariant,
    sourceRouteCategory,
} from "../lib/sourcing/source-palette"
import type {
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "../lib/sourcing/types"

type FilterKind =
    | "all"
    | "stock"
    | "warehouse"
    | "direct_dropship"
    | "direct_bulk"
    | "deliverable"
    | "inquiry"
type SortKind = "default" | "price" | "delivery"

/**
 * 可视化比价选源矩阵：支持「替换当前来源」与「添加新来源」两种模式，
 * 按履约方式（现有库存 / 集采入仓 / 代发直发 / 集采直发）直观筛选，
 * 并排展示价格、毛利/倒挂、税率、账期、配送覆盖与可供量。
 */
export function SourcingOptionCompareMatrix({
    idPrefix,
    order,
    product,
    selectedBasisId,
    usedBasisIds,
    recommendedBasisId,
    mode = "replace",
    onSelect,
}: {
    idPrefix: string
    order: SourcingSalesOrder
    product: SourcingProductLine
    selectedBasisId: string
    /** 同一商品其他（或全部，在 add 模式下）拆分来源已占用的 basisId，防止重复选择同一履约方案。 */
    usedBasisIds: ReadonlySet<string>
    recommendedBasisId?: string
    mode?: "replace" | "add"
    onSelect: (option: SourcingSupplierOption) => void
}) {
    const [filter, setFilter] = useState<FilterKind>("all")
    const [sort, setSort] = useState<SortKind>("default")

    const salesPrice = product.salesPrices?.unit_price_gross?.trim()
    const hasAddresses = product.deliveryAllocations.length > 0

    const counts = useMemo(() => {
        let stock = 0
        let warehouse = 0
        let directDropship = 0
        let directBulk = 0
        let inquiry = 0
        let deliverable = 0
        for (const o of product.options) {
            const cat = sourceRouteCategory(o)
            if (cat === "stock") stock += 1
            else if (cat === "warehouse") warehouse += 1
            else if (cat === "direct_dropship") directDropship += 1
            else if (cat === "direct_bulk") directBulk += 1
            if (o.inquirySource) inquiry += 1
            if (hasAddresses) {
                if (
                    o.sourceType === "EXISTING_STOCK" ||
                    o.fulfillmentResponsibility !== "SUPPLIER_DIRECT" ||
                    deliverySummary(o, product).startsWith("全部地址")
                ) {
                    deliverable += 1
                }
            }
        }
        return {
            stock,
            warehouse,
            directDropship,
            directBulk,
            inquiry,
            deliverable,
        }
    }, [hasAddresses, product])

    const visibleOptions = useMemo(() => {
        const filtered = product.options.filter((option) => {
            if (filter === "inquiry") return Boolean(option.inquirySource)
            if (
                filter === "stock" ||
                filter === "warehouse" ||
                filter === "direct_dropship" ||
                filter === "direct_bulk"
            ) {
                return sourceRouteCategory(option) === filter
            }
            if (filter === "deliverable") {
                if (option.sourceType === "EXISTING_STOCK") return true
                if (option.fulfillmentResponsibility !== "SUPPLIER_DIRECT")
                    return true
                return deliverySummary(option, product).startsWith("全部地址")
            }
            return true
        })

        if (sort === "default") {
            if (mode === "add") {
                // 添加模式把尚未添加的可用方案排在前面
                return [...filtered].sort((left, right) => {
                    const leftUsed = usedBasisIds.has(left.basisId)
                    const rightUsed = usedBasisIds.has(right.basisId)
                    if (leftUsed !== rightUsed) return leftUsed ? 1 : -1
                    return 0
                })
            }
            return filtered
        }
        return [...filtered].sort((left, right) => {
            if (sort === "price") {
                if (left.sourceType !== right.sourceType) {
                    return left.sourceType === "EXISTING_STOCK" ? -1 : 1
                }
                try {
                    return compareDecimal(
                        left.unitCostGross,
                        right.unitCostGross,
                        4,
                    )
                } catch {
                    return 0
                }
            }
            const leftDate =
                left.sourceType === "EXISTING_STOCK"
                    ? "0000-00-00"
                    : left.expectedDeliveryDate || "9999-99-99"
            const rightDate =
                right.sourceType === "EXISTING_STOCK"
                    ? "0000-00-00"
                    : right.expectedDeliveryDate || "9999-99-99"
            return leftDate.localeCompare(rightDate)
        })
    }, [filter, mode, product, sort, usedBasisIds])

    if (product.options.length === 0) {
        return (
            <p className="rounded-lg border border-dashed border-border bg-surface-sunken px-4 py-5 text-center text-xs text-muted-foreground">
                当前商品暂无可用的库存或合格采购供给方案。
            </p>
        )
    }

    const filterButtons: Array<{ value: FilterKind; label: string }> = [
        { value: "all", label: `全部 ${product.options.length}` },
        ...(counts.stock > 0
            ? [{ value: "stock" as const, label: `现有库存 ${counts.stock}` }]
            : []),
        ...(counts.warehouse > 0
            ? [
                  {
                      value: "warehouse" as const,
                      label: `集采入仓 ${counts.warehouse}`,
                  },
              ]
            : []),
        ...(counts.directDropship > 0
            ? [
                  {
                      value: "direct_dropship" as const,
                      label: `代发直发 ${counts.directDropship}`,
                  },
              ]
            : []),
        ...(counts.directBulk > 0
            ? [
                  {
                      value: "direct_bulk" as const,
                      label: `集采直发 ${counts.directBulk}`,
                  },
              ]
            : []),
        ...(hasAddresses &&
        counts.deliverable > 0 &&
        counts.deliverable < product.options.length
            ? [
                  {
                      value: "deliverable" as const,
                      label: `全地址可配 ${counts.deliverable}`,
                  },
              ]
            : []),
        ...(counts.inquiry > 0 && counts.inquiry < product.options.length
            ? [
                  {
                      value: "inquiry" as const,
                      label: `询价报价 ${counts.inquiry}`,
                  },
              ]
            : []),
    ]

    return (
        <div className="overflow-hidden rounded-lg border border-border/80 bg-card">
            <div className="flex flex-wrap items-center justify-between gap-2 border-b border-border/60 bg-surface-sunken px-3 py-1.5">
                <div
                    role="group"
                    aria-label="筛选候选供给方案"
                    className="flex flex-wrap items-center gap-1"
                >
                    {filterButtons.map((btn) => (
                        <Button
                            key={btn.value}
                            id={`${idPrefix}-compare-filter-${btn.value}`}
                            type="button"
                            size="xs"
                            variant={
                                filter === btn.value ? "secondary" : "ghost"
                            }
                            aria-pressed={filter === btn.value}
                            onClick={() => setFilter(btn.value)}
                        >
                            {btn.label}
                        </Button>
                    ))}
                </div>

                <div className="flex items-center gap-1">
                    <ArrowUpDownIcon
                        aria-hidden="true"
                        className="size-3.5 text-muted-foreground"
                    />
                    {(
                        [
                            ["default", "默认"],
                            ["price", "低价优先"],
                            ["delivery", "交期优先"],
                        ] as const
                    ).map(([value, label]) => (
                        <Button
                            key={value}
                            id={`${idPrefix}-compare-sort-${value}`}
                            type="button"
                            size="xs"
                            variant={sort === value ? "secondary" : "ghost"}
                            aria-pressed={sort === value}
                            onClick={() => setSort(value)}
                        >
                            {label}
                        </Button>
                    ))}
                </div>
            </div>

            <div className="max-h-64 overflow-y-auto overscroll-contain">
                <div className="hidden grid-cols-[1.5rem_minmax(0,1.6fr)_6rem_8.5rem_6.5rem_minmax(0,1.2fr)_5.5rem_5.5rem] items-center gap-2 border-b border-border/60 bg-table-header px-3 py-1.5 text-xs font-medium text-muted-foreground @min-[700px]/sourcing:grid">
                    <span className="sr-only">选择</span>
                    <span>供给方 / 仓库</span>
                    <span>履约方式</span>
                    <span className="text-right">含税单价 / 毛利</span>
                    <span>税率 / 付款</span>
                    <span>配送与起订</span>
                    <span className="text-right">可供量</span>
                    <span className="text-right">预计交期</span>
                </div>

                {visibleOptions.length === 0 ? (
                    <p className="px-4 py-6 text-center text-xs text-muted-foreground">
                        当前筛选条件下无匹配方案。
                    </p>
                ) : (
                    <ul className="divide-y divide-grid">
                        {visibleOptions.map((option) => {
                            const isSelected =
                                mode === "replace" &&
                                option.basisId === selectedBasisId
                            const isStock =
                                option.sourceType === "EXISTING_STOCK"
                            const isUsedElsewhere =
                                !isSelected && usedBasisIds.has(option.basisId)
                            const minimum = bulkMinimumOptionState(
                                order,
                                option,
                            )
                            const undeliverableAll = directOptionUndeliverable(
                                option,
                                product,
                            )
                            const disabled = isUsedElsewhere || minimum.disabled
                            const name = isStock
                                ? option.warehouseName || option.supplierName
                                : option.supplierName
                            const route = sourcingRouteLabel(option)
                            const routeVariant = sourceRouteBadgeVariant(option)
                            const deliveryText = deliverySummary(
                                option,
                                product,
                            )
                            const hasDeliveryWarning =
                                !isStock &&
                                (undeliverableAll ||
                                    directOptionHasFreightExtra(
                                        option,
                                        product,
                                    ) ||
                                    (Boolean(deliveryText) &&
                                        !deliveryText.startsWith("全部地址")))
                            const margin = !isStock
                                ? comparePurchaseCostToSalesPrice(
                                      option.unitCostGross,
                                      salesPrice,
                                  )
                                : null
                            const taxPercent = !isStock
                                ? multiplyFixed(option.inputTaxRate, "100", {
                                      leftMaxScale: 6,
                                      rightMaxScale: 0,
                                      outputScale: 2,
                                  })
                                : null
                            const availableQty = isStock
                                ? (option.sourceAvailableQuantity ??
                                  option.maxCreateQuantity)
                                : option.maxCreateQuantity
                            const isRecommended =
                                recommendedBasisId === option.basisId

                            return (
                                <li key={option.basisId}>
                                    <button
                                        id={`${idPrefix}-compare-option-${toAutomationIdSegment(option.basisId)}`}
                                        type="button"
                                        disabled={disabled}
                                        aria-pressed={isSelected}
                                        onClick={() => onSelect(option)}
                                        className={cn(
                                            "grid w-full cursor-pointer grid-cols-1 items-center gap-x-2 gap-y-1.5 px-3 py-2 text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring @min-[700px]/sourcing:grid-cols-[1.5rem_minmax(0,1.6fr)_6rem_8.5rem_6.5rem_minmax(0,1.2fr)_5.5rem_5.5rem]",
                                            isSelected
                                                ? "bg-sidebar-accent/70"
                                                : "hover:bg-row-hover",
                                            disabled &&
                                                "cursor-not-allowed opacity-55 hover:bg-transparent",
                                        )}
                                    >
                                        <span className="hidden items-center justify-center @min-[700px]/sourcing:flex">
                                            {isSelected ? (
                                                <CheckCircle2Icon
                                                    aria-hidden="true"
                                                    className="size-4 text-primary"
                                                />
                                            ) : mode === "add" && !disabled ? (
                                                <PlusCircleIcon
                                                    aria-hidden="true"
                                                    className="size-4 text-primary"
                                                />
                                            ) : (
                                                <CircleIcon
                                                    aria-hidden="true"
                                                    className="size-4 text-muted-foreground/50"
                                                />
                                            )}
                                        </span>

                                        <div className="min-w-0">
                                            <div className="flex flex-wrap items-center gap-1.5">
                                                <span className="truncate text-body-compact font-medium text-foreground">
                                                    {name}
                                                </span>
                                                {isRecommended ? (
                                                    <Badge
                                                        variant="info"
                                                        className="h-4.5 px-1.5 text-tiny"
                                                    >
                                                        <SparklesIcon aria-hidden="true" />
                                                        推荐
                                                    </Badge>
                                                ) : null}
                                                {option.inquirySource ? (
                                                    <Badge
                                                        variant="violet"
                                                        className="h-4.5 px-1.5 text-tiny"
                                                    >
                                                        询价
                                                    </Badge>
                                                ) : null}
                                                {isUsedElsewhere ? (
                                                    <Badge
                                                        variant="neutral"
                                                        className="h-4.5 px-1.5 text-tiny"
                                                    >
                                                        {mode === "add"
                                                            ? "已添加"
                                                            : "他源已用"}
                                                    </Badge>
                                                ) : null}
                                            </div>
                                            {option.inquirySource ? (
                                                <p className="mt-0.5 text-tiny text-muted-foreground">
                                                    第{" "}
                                                    {
                                                        option.inquirySource
                                                            .feedback_round
                                                    }{" "}
                                                    轮 · 报价 v
                                                    {
                                                        option.inquirySource
                                                            .quote_revision_no
                                                    }
                                                </p>
                                            ) : null}
                                        </div>

                                        <div>
                                            <Badge
                                                variant={routeVariant}
                                                className="h-5 px-1.5"
                                            >
                                                {isStock ? (
                                                    <PackageCheckIcon aria-hidden="true" />
                                                ) : null}
                                                {route}
                                            </Badge>
                                        </div>

                                        <div className="text-left @min-[700px]/sourcing:text-right">
                                            {isStock ? (
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
                                                        <p
                                                            className={cn(
                                                                "num text-tiny font-medium",
                                                                margin.inverted
                                                                    ? "text-destructive"
                                                                    : "text-success-soft-foreground",
                                                            )}
                                                        >
                                                            {margin.inverted
                                                                ? `高于售价 ¥${margin.absSpread}`
                                                                : margin.marginRateText
                                                                  ? `毛利 ${margin.marginRateText}`
                                                                  : "平价"}
                                                        </p>
                                                    ) : null}
                                                </>
                                            )}
                                        </div>

                                        <div className="text-xs text-muted-foreground">
                                            {isStock ? (
                                                "—"
                                            ) : (
                                                <>
                                                    {taxPercent ? (
                                                        <span className="block">
                                                            税率{" "}
                                                            <RateValue
                                                                value={
                                                                    taxPercent
                                                                }
                                                                precision={2}
                                                            />
                                                        </span>
                                                    ) : null}
                                                    <span className="block truncate">
                                                        {option.paymentTermLabel ||
                                                            "—"}
                                                    </span>
                                                </>
                                            )}
                                        </div>

                                        <div className="min-w-0 text-xs">
                                            {isStock ? (
                                                <span className="text-muted-foreground">
                                                    仓库统一安排发货
                                                </span>
                                            ) : option.fulfillmentResponsibility ===
                                              "WAREHOUSE" ? (
                                                <span className="text-muted-foreground">
                                                    采购入仓（需选目标仓）
                                                </span>
                                            ) : (
                                                <div className="space-y-0.5">
                                                    {deliveryText ? (
                                                        <span
                                                            className={cn(
                                                                "flex items-center gap-1",
                                                                undeliverableAll
                                                                    ? "text-destructive"
                                                                    : hasDeliveryWarning
                                                                      ? "text-warning-soft-foreground"
                                                                      : "text-muted-foreground",
                                                            )}
                                                        >
                                                            {hasDeliveryWarning ? (
                                                                <TriangleAlertIcon
                                                                    aria-hidden="true"
                                                                    className="size-3 shrink-0"
                                                                />
                                                            ) : null}
                                                            <span className="truncate">
                                                                {deliveryText}
                                                            </span>
                                                        </span>
                                                    ) : (
                                                        <span className="text-muted-foreground">
                                                            按订单履约
                                                        </span>
                                                    )}
                                                    {minimum.label ? (
                                                        <span
                                                            className={cn(
                                                                "block text-tiny",
                                                                minimum.disabled
                                                                    ? "text-destructive"
                                                                    : "text-muted-foreground",
                                                            )}
                                                        >
                                                            {minimum.label}
                                                        </span>
                                                    ) : null}
                                                </div>
                                            )}
                                        </div>

                                        <div className="text-left text-xs @min-[700px]/sourcing:text-right">
                                            <QuantityValue
                                                value={availableQty}
                                                unit={product.unit}
                                            />
                                        </div>

                                        <div className="num text-left text-xs text-muted-foreground @min-[700px]/sourcing:text-right">
                                            {isStock
                                                ? "现货"
                                                : option.expectedDeliveryDate ||
                                                  "待填写"}
                                        </div>
                                    </button>
                                </li>
                            )
                        })}
                    </ul>
                )}
            </div>
        </div>
    )
}
