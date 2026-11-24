"use client"

import { useMemo, useState } from "react"
import {
    ArrowUpDownIcon,
    CheckCircle2Icon,
    PackageCheckIcon,
    PlusIcon,
    RefreshCwIcon,
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
 * 全高比价与选源工作表：
 * - 按履约方式（现有库存 / 集采入仓 / 代发直发 / 集采直发）与配送覆盖快速筛选；
 * - 同表同时支持「替换当前激活来源」与「+ 拆单加入左侧新来源」，多源混配无需反复开关浮层。
 */
export function SourcingOptionCompareMatrix({
    idPrefix,
    order,
    product,
    selectedBasisId,
    usedBasisIds,
    recommendedBasisId,
    activeSourceNumber,
    mode = "replace",
    fullHeight = false,
    canAddAsNewSource = false,
    onSelect,
    onAddAsNewSource,
}: {
    idPrefix: string
    order: SourcingSalesOrder
    product: SourcingProductLine
    selectedBasisId: string
    /** 同一商品其他（或全部，在 add 模式下）拆分来源已占用的 basisId。 */
    usedBasisIds: ReadonlySet<string>
    recommendedBasisId?: string
    activeSourceNumber?: number
    mode?: "replace" | "add"
    /** 右侧工作面全高铺满模式，去除冗余外框与固定限高。 */
    fullHeight?: boolean
    canAddAsNewSource?: boolean
    onSelect: (option: SourcingSupplierOption) => void
    onAddAsNewSource?: (option: SourcingSupplierOption) => void
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
            <div className="flex flex-1 items-center justify-center p-8">
                <p className="rounded-lg border border-dashed border-border bg-surface-sunken px-6 py-8 text-center text-xs text-muted-foreground">
                    当前商品暂无可用的库存或合格采购供给方案。
                </p>
            </div>
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
        <div
            className={cn(
                "@container/compare flex min-h-0 flex-col bg-card",
                fullHeight
                    ? "flex-1 overflow-hidden"
                    : "overflow-hidden rounded-lg border border-border/80",
            )}
        >
            {/* 筛选与排序工具条 */}
            <div
                className={cn(
                    "flex shrink-0 flex-wrap items-center justify-between gap-2 border-b border-border/60 py-2",
                    fullHeight ? "bg-card px-5" : "bg-surface-sunken px-3",
                )}
            >
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

            {/* 固定表头 */}
            <div
                className={cn(
                    "hidden shrink-0 grid-cols-[minmax(0,1.6fr)_5.5rem_7.5rem_minmax(0,1.1fr)_4.75rem_8.5rem] items-center gap-2.5 border-b border-border/70 bg-table-header py-2 text-xs font-medium text-muted-foreground @min-[680px]/compare:grid @min-[920px]/compare:grid-cols-[minmax(0,1.6fr)_5.5rem_7.5rem_6.5rem_minmax(0,1.1fr)_4.75rem_5.25rem_8.75rem]",
                    fullHeight ? "px-5" : "px-3",
                )}
            >
                <span>供给方 / 仓库</span>
                <span>履约方式</span>
                <span className="text-right">含税单价 / 毛利</span>
                <span className="hidden @min-[920px]/compare:inline">
                    税率 / 付款
                </span>
                <span>配送与起订</span>
                <span className="text-right">可供量</span>
                <span className="hidden text-right @min-[920px]/compare:inline">
                    预计交期
                </span>
                <span className="text-right">操作</span>
            </div>

            {/* 独立滚动表身 */}
            <div
                className={cn(
                    "min-h-0 overflow-y-auto overscroll-contain",
                    fullHeight ? "flex-1" : "max-h-72",
                )}
            >
                {visibleOptions.length === 0 ? (
                    <p className="px-4 py-10 text-center text-xs text-muted-foreground">
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
                            const isUsed = usedBasisIds.has(option.basisId)
                            const isUsedElsewhere = !isSelected && isUsed
                            const minimum = bulkMinimumOptionState(
                                order,
                                option,
                            )
                            const undeliverableAll = directOptionUndeliverable(
                                option,
                                product,
                            )
                            const replaceDisabled =
                                isSelected ||
                                isUsedElsewhere ||
                                minimum.disabled
                            const addDisabled =
                                !canAddAsNewSource ||
                                isUsed ||
                                isSelected ||
                                minimum.disabled
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
                            const optSeg = toAutomationIdSegment(option.basisId)

                            return (
                                <li
                                    key={option.basisId}
                                    className={cn(
                                        "grid grid-cols-1 items-center gap-x-2.5 gap-y-2 py-2.5 transition-colors @min-[680px]/compare:grid-cols-[minmax(0,1.6fr)_5.5rem_7.5rem_minmax(0,1.1fr)_4.75rem_8.5rem] @min-[920px]/compare:grid-cols-[minmax(0,1.6fr)_5.5rem_7.5rem_6.5rem_minmax(0,1.1fr)_4.75rem_5.25rem_8.75rem]",
                                        fullHeight ? "px-5" : "px-3",
                                        isSelected
                                            ? "bg-sidebar-accent/55"
                                            : isUsedElsewhere
                                              ? "bg-surface-sunken/50"
                                              : "hover:bg-row-hover",
                                        minimum.disabled && "opacity-60",
                                    )}
                                >
                                    <div className="min-w-0">
                                        <div className="flex flex-wrap items-center gap-1.5">
                                            <span
                                                className="truncate text-body-compact font-medium text-foreground"
                                                title={name}
                                            >
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

                                    <div className="text-left @min-[680px]/compare:text-right">
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
                                                            ? `倒挂 ¥${margin.absSpread}`
                                                            : margin.marginRateText
                                                              ? `毛利 ${margin.marginRateText}`
                                                              : "平价"}
                                                    </p>
                                                ) : null}
                                            </>
                                        )}
                                    </div>

                                    <div className="hidden text-xs text-muted-foreground @min-[920px]/compare:block">
                                        {isStock ? (
                                            "—"
                                        ) : (
                                            <>
                                                {taxPercent ? (
                                                    <span className="block">
                                                        税率{" "}
                                                        <RateValue
                                                            value={taxPercent}
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

                                    <div className="text-left text-xs @min-[680px]/compare:text-right">
                                        <QuantityValue
                                            value={availableQty}
                                            unit={product.unit}
                                        />
                                    </div>

                                    <div className="num hidden text-right text-xs text-muted-foreground @min-[920px]/compare:block">
                                        {isStock
                                            ? "现货"
                                            : option.expectedDeliveryDate ||
                                              "待填写"}
                                    </div>

                                    <div className="flex flex-wrap items-center justify-start gap-1.5 @min-[680px]/compare:justify-end">
                                        {isSelected ? (
                                            <span className="inline-flex items-center gap-1 text-xs font-medium text-primary">
                                                <CheckCircle2Icon
                                                    aria-hidden="true"
                                                    className="size-3.5"
                                                />
                                                {activeSourceNumber
                                                    ? `源${activeSourceNumber}当前`
                                                    : "当前选用"}
                                            </span>
                                        ) : isUsedElsewhere ? (
                                            <Badge
                                                variant="neutral"
                                                className="h-5 px-2 text-tiny"
                                            >
                                                已加入拆单
                                            </Badge>
                                        ) : mode === "add" ? (
                                            <Button
                                                id={`${idPrefix}-compare-option-${optSeg}`}
                                                type="button"
                                                size="xs"
                                                variant="secondary"
                                                disabled={addDisabled}
                                                onClick={() => onSelect(option)}
                                            >
                                                <PlusIcon aria-hidden="true" />
                                                加入拆单
                                            </Button>
                                        ) : (
                                            <>
                                                <Button
                                                    id={`${idPrefix}-compare-option-${optSeg}`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    disabled={replaceDisabled}
                                                    onClick={() =>
                                                        onSelect(option)
                                                    }
                                                >
                                                    <RefreshCwIcon aria-hidden="true" />
                                                    {activeSourceNumber
                                                        ? `换源${activeSourceNumber}`
                                                        : "替换"}
                                                </Button>
                                                {onAddAsNewSource &&
                                                canAddAsNewSource ? (
                                                    <Button
                                                        id={`${idPrefix}-compare-add-${optSeg}`}
                                                        type="button"
                                                        size="xs"
                                                        variant="secondary"
                                                        disabled={addDisabled}
                                                        onClick={() =>
                                                            onAddAsNewSource(
                                                                option,
                                                            )
                                                        }
                                                    >
                                                        <PlusIcon aria-hidden="true" />
                                                        拆单
                                                    </Button>
                                                ) : null}
                                            </>
                                        )}
                                    </div>
                                </li>
                            )
                        })}
                    </ul>
                )}
            </div>
        </div>
    )
}
