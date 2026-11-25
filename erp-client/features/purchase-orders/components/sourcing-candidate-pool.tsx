"use client"

import { useMemo, useState } from "react"
import {
    ArrowUpDownIcon,
    MapPinIcon,
    PlusIcon,
    RefreshCwIcon,
    SparklesIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"
import {
    FulfillmentRouteBadge,
    MoneyValue,
    QuantityValue,
    RateValue,
    sourceRouteCategory,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    computeOptionAddressCoverage,
    type OptionAddressCoverage,
} from "../lib/sourcing/address-grouping"
import { bulkMinimumOptionState } from "../lib/sourcing/bulk-minimum"
import { sourcingRouteLabel } from "../lib/sourcing/delivery-terms"
import {
    comparePurchaseCostToSalesPrice,
    priceDeltaFromLowest,
} from "../lib/sourcing/pricing"
import type {
    SourcingProductLine,
    SourcingSalesOrder,
    SourcingSupplierOption,
} from "../lib/sourcing/types"

type RouteFilter =
    | "all"
    | "stock"
    | "warehouse"
    | "direct_dropship"
    | "direct_bulk"
    | "deliverable"

type PoolSort = "price" | "delivery" | "recommended"

/**
 * 左侧供给池的「候选方案比价区」：
 * - 直接常驻在左侧已选来源下方，彻底消除“先点添加来源 → 再去右侧切 Tab 比价”的冗余路径；
 * - 默认按「低价优先」排序（现货置顶，采购方案按含税单价升序、全包邮优先），一眼定位最低价与价差；
 * - 支持右侧地址异常/待配一键联动「仅看这 N 处地址可配的候选方案」。
 */
export function SourcingCandidatePool({
    idPrefix,
    order,
    product,
    usedBasisIds,
    lowestPurchaseCost,
    recommendedBasisId,
    activeSourceNumber,
    canSplit,
    showReplaceAction,
    rescueAddressScope,
    onClearRescueScope,
    onAddOption,
    onReplaceActive,
}: {
    idPrefix: string
    order: SourcingSalesOrder
    product: SourcingProductLine
    usedBasisIds: ReadonlySet<string>
    lowestPurchaseCost?: string
    recommendedBasisId?: string
    activeSourceNumber: number
    canSplit: boolean
    showReplaceAction: boolean
    /** 若由右侧“为异常/待配 N 处地址找方案”触发，仅针对这些地址计算覆盖率并优先排序。 */
    rescueAddressScope?: {
        label: string
        keys: ReadonlySet<string>
    } | null
    onClearRescueScope?: () => void
    onAddOption: (option: SourcingSupplierOption) => void
    onReplaceActive: (option: SourcingSupplierOption) => void
}) {
    const [routeFilter, setRouteFilter] = useState<RouteFilter>("all")
    const [sort, setSort] = useState<PoolSort>("price")

    const salesPrice = product.salesPrices?.unit_price_gross?.trim()
    const hasAddresses = product.deliveryAllocations.length > 0

    const unusedOptions = useMemo(
        () => product.options.filter((opt) => !usedBasisIds.has(opt.basisId)),
        [product.options, usedBasisIds],
    )

    const enriched = useMemo(() => {
        const scope = rescueAddressScope?.keys
        return unusedOptions.map((option) => {
            const coverage = computeOptionAddressCoverage(
                option,
                product,
                scope,
            )
            return { option, coverage }
        })
    }, [product, rescueAddressScope?.keys, unusedOptions])

    const counts = useMemo(() => {
        let stock = 0
        let warehouse = 0
        let directDropship = 0
        let directBulk = 0
        let deliverable = 0
        for (const { option, coverage } of enriched) {
            const cat = sourceRouteCategory(option)
            if (cat === "stock") stock += 1
            else if (cat === "warehouse") warehouse += 1
            else if (cat === "direct_dropship") directDropship += 1
            else if (cat === "direct_bulk") directBulk += 1
            if (hasAddresses && coverage.allDeliverable) deliverable += 1
        }
        return { stock, warehouse, directDropship, directBulk, deliverable }
    }, [enriched, hasAddresses])

    const visible = useMemo(() => {
        const filtered = enriched.filter(({ option, coverage }) => {
            if (rescueAddressScope && coverage.deliverableCount === 0) {
                return false
            }
            if (
                routeFilter === "stock" ||
                routeFilter === "warehouse" ||
                routeFilter === "direct_dropship" ||
                routeFilter === "direct_bulk"
            ) {
                return sourceRouteCategory(option) === routeFilter
            }
            if (routeFilter === "deliverable") {
                return coverage.allDeliverable
            }
            return true
        })

        return [...filtered].sort((left, right) => {
            // 若正在为指定异常/未配地址找补缺方案，优先把能覆盖更多目标地址的排在前面
            if (
                rescueAddressScope &&
                left.coverage.deliverableCount !==
                    right.coverage.deliverableCount
            ) {
                return (
                    right.coverage.deliverableCount -
                    left.coverage.deliverableCount
                )
            }

            if (sort === "recommended") {
                const lRec = left.option.basisId === recommendedBasisId ? 0 : 1
                const rRec = right.option.basisId === recommendedBasisId ? 0 : 1
                if (lRec !== rRec) return lRec - rRec
            }

            if (sort === "delivery") {
                const lDate =
                    left.option.sourceType === "EXISTING_STOCK"
                        ? "0000-00-00"
                        : left.option.expectedDeliveryDate || "9999-99-99"
                const rDate =
                    right.option.sourceType === "EXISTING_STOCK"
                        ? "0000-00-00"
                        : right.option.expectedDeliveryDate || "9999-99-99"
                const dateCmp = lDate.localeCompare(rDate)
                if (dateCmp !== 0) return dateCmp
            }

            // 默认低价优先：现货优先，其次含税单价升序，同价按包邮与全可配优先
            if (left.option.sourceType !== right.option.sourceType) {
                return left.option.sourceType === "EXISTING_STOCK" ? -1 : 1
            }
            if (left.option.sourceType === "PURCHASE") {
                try {
                    const priceCmp = compareDecimal(
                        left.option.unitCostGross,
                        right.option.unitCostGross,
                        4,
                    )
                    if (priceCmp !== 0) return priceCmp
                } catch {
                    // ignore
                }
            }
            if (
                left.coverage.allDeliverable !== right.coverage.allDeliverable
            ) {
                return left.coverage.allDeliverable ? -1 : 1
            }
            if (
                left.coverage.allFreightIncluded !==
                right.coverage.allFreightIncluded
            ) {
                return left.coverage.allFreightIncluded ? -1 : 1
            }
            const lDate = left.option.expectedDeliveryDate || "9999-99-99"
            const rDate = right.option.expectedDeliveryDate || "9999-99-99"
            return lDate.localeCompare(rDate)
        })
    }, [enriched, recommendedBasisId, rescueAddressScope, routeFilter, sort])

    if (unusedOptions.length === 0) {
        return (
            <p className="rounded-lg border border-dashed border-border/70 bg-card/60 px-3 py-3 text-center text-xs text-muted-foreground">
                全部 {product.options.length} 个合格供给方案均已加入上方分配。
            </p>
        )
    }

    const filterChips: Array<{ value: RouteFilter; label: string }> = [
        { value: "all", label: `全部 ${unusedOptions.length}` },
        ...(counts.stock > 0
            ? [{ value: "stock" as const, label: `现货 ${counts.stock}` }]
            : []),
        ...(counts.directDropship > 0
            ? [
                  {
                      value: "direct_dropship" as const,
                      label: `代发 ${counts.directDropship}`,
                  },
              ]
            : []),
        ...(counts.warehouse > 0
            ? [
                  {
                      value: "warehouse" as const,
                      label: `入仓 ${counts.warehouse}`,
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
        counts.deliverable < unusedOptions.length
            ? [
                  {
                      value: "deliverable" as const,
                      label: `全可配 ${counts.deliverable}`,
                  },
              ]
            : []),
    ]

    const distinctRouteKinds = [
        counts.stock > 0,
        counts.warehouse > 0,
        counts.directDropship > 0,
        counts.directBulk > 0,
    ].filter(Boolean).length

    return (
        <div
            id={`${idPrefix}-candidate-pool`}
            className="space-y-2"
            aria-label="候选供给方案比价"
        >
            {/* 区块标题 + 排序切换（默认低价优先） */}
            <div className="flex flex-wrap items-center justify-between gap-2 pt-1">
                <div className="flex items-center gap-1.5">
                    <h3 className="text-xs font-semibold text-foreground">
                        其他候选方案
                    </h3>
                    <span className="num rounded-sm bg-muted px-1.5 py-0 text-tiny font-medium text-muted-foreground">
                        {unusedOptions.length}
                    </span>
                </div>

                <div
                    role="group"
                    aria-label="候选排序方式"
                    className="inline-flex items-center gap-0.5 text-tiny"
                >
                    <ArrowUpDownIcon
                        aria-hidden="true"
                        className="mr-0.5 size-3 text-muted-foreground"
                    />
                    {(
                        [
                            ["price", "低价优先"],
                            ["delivery", "交期优先"],
                            ["recommended", "系统推荐"],
                        ] as const
                    ).map(([value, label]) => (
                        <button
                            key={value}
                            id={`${idPrefix}-pool-sort-${value}`}
                            type="button"
                            aria-pressed={sort === value}
                            onClick={() => setSort(value)}
                            className={cn(
                                "cursor-pointer rounded-sm px-1.5 py-0.5 text-tiny font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                sort === value
                                    ? "bg-card text-primary shadow-2xs ring-1 ring-border/75"
                                    : "text-muted-foreground hover:text-foreground",
                            )}
                        >
                            {label}
                        </button>
                    ))}
                </div>
            </div>

            {/* 补缺地址联动提示条 */}
            {rescueAddressScope ? (
                <div className="flex items-center justify-between gap-2 rounded-md border border-info-border bg-info-soft/65 px-2.5 py-1.5 text-xs text-info-soft-foreground">
                    <span className="flex items-center gap-1.5">
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        <span>
                            已按 <strong>{rescueAddressScope.label}</strong>{" "}
                            筛出可配送方案（低价优先）
                        </span>
                    </span>
                    {onClearRescueScope ? (
                        <button
                            id={`${idPrefix}-pool-clear-rescue`}
                            type="button"
                            aria-label="清除地址补缺筛选"
                            onClick={onClearRescueScope}
                            className="inline-flex cursor-pointer items-center gap-0.5 rounded-sm px-1 py-0.5 text-tiny font-medium hover:bg-card/70"
                        >
                            <XIcon className="size-3" />
                            清除
                        </button>
                    ) : null}
                </div>
            ) : null}

            {/* 履约方式快捷筛选条（仅存在多种类型或部分不配送时展示） */}
            {distinctRouteKinds > 1 || filterChips.length > 2 ? (
                <div
                    role="group"
                    aria-label="按履约方式筛选候选方案"
                    className="flex flex-wrap items-center gap-1"
                >
                    {filterChips.map((chip) => (
                        <button
                            key={chip.value}
                            id={`${idPrefix}-pool-filter-${chip.value}`}
                            type="button"
                            aria-pressed={routeFilter === chip.value}
                            onClick={() => setRouteFilter(chip.value)}
                            className={cn(
                                "cursor-pointer rounded-sm px-2 py-0.5 text-tiny font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                routeFilter === chip.value
                                    ? "bg-card text-foreground shadow-2xs ring-1 ring-border/80"
                                    : "text-muted-foreground hover:bg-card/60 hover:text-foreground",
                            )}
                        >
                            {chip.label}
                        </button>
                    ))}
                </div>
            ) : null}

            {/* 候选方案卡片列表（高密度 2 行结构，零截断） */}
            {visible.length === 0 ? (
                <p className="rounded-lg border border-dashed border-border/70 bg-card/60 px-3 py-5 text-center text-xs text-muted-foreground">
                    {rescueAddressScope
                        ? "当前没有其他候选方案可配送目标地址，建议切回仓库/库存统筹。"
                        : "当前筛选条件下无匹配候选方案。"}
                </p>
            ) : (
                <ul className="space-y-1.5">
                    {visible.map(({ option, coverage }) => (
                        <CandidateRow
                            key={option.basisId}
                            idPrefix={idPrefix}
                            order={order}
                            product={product}
                            option={option}
                            coverage={coverage}
                            salesPrice={salesPrice}
                            lowestPurchaseCost={lowestPurchaseCost}
                            isRecommended={
                                recommendedBasisId === option.basisId
                            }
                            activeSourceNumber={activeSourceNumber}
                            canSplit={canSplit}
                            showReplaceAction={showReplaceAction}
                            scopedMode={Boolean(rescueAddressScope)}
                            onAdd={() => onAddOption(option)}
                            onReplace={() => onReplaceActive(option)}
                        />
                    ))}
                </ul>
            )}
        </div>
    )
}

function CandidateRow({
    idPrefix,
    order,
    product,
    option,
    coverage,
    salesPrice,
    lowestPurchaseCost,
    isRecommended,
    activeSourceNumber,
    canSplit,
    showReplaceAction,
    scopedMode,
    onAdd,
    onReplace,
}: {
    idPrefix: string
    order: SourcingSalesOrder
    product: SourcingProductLine
    option: SourcingSupplierOption
    coverage: OptionAddressCoverage
    salesPrice?: string
    lowestPurchaseCost?: string
    isRecommended: boolean
    activeSourceNumber: number
    canSplit: boolean
    showReplaceAction: boolean
    scopedMode: boolean
    onAdd: () => void
    onReplace: () => void
}) {
    const isStock = option.sourceType === "EXISTING_STOCK"
    const isDirect =
        !isStock && option.fulfillmentResponsibility === "SUPPLIER_DIRECT"
    const name = isStock
        ? option.warehouseName || option.supplierName
        : option.supplierName
    const route = sourcingRouteLabel(option)
    const minimum = bulkMinimumOptionState(order, option)
    const margin = !isStock
        ? comparePurchaseCostToSalesPrice(option.unitCostGross, salesPrice)
        : null
    const priceDelta = !isStock
        ? priceDeltaFromLowest(option.unitCostGross, lowestPurchaseCost)
        : null
    const taxPercent = !isStock
        ? multiplyFixed(option.inputTaxRate, "100", {
              leftMaxScale: 6,
              rightMaxScale: 0,
              outputScale: 2,
          })
        : null
    const availableQty = isStock
        ? (option.sourceAvailableQuantity ?? option.maxCreateQuantity)
        : option.maxCreateQuantity
    const optSeg = toAutomationIdSegment(option.basisId)

    const coverageText = (() => {
        if (isStock) return "仓库现货出库"
        if (!isDirect) return "入仓统筹发货"
        if (coverage.totalCount === 0) return "按订单直发"
        if (coverage.allFreightIncluded) {
            return scopedMode
                ? `目标 ${coverage.deliverableCount}/${coverage.totalCount} 处全配`
                : `${coverage.totalCount}/${coverage.totalCount} 处全配送`
        }
        if (coverage.notDeliverableCount > 0) {
            return `${coverage.deliverableCount}/${coverage.totalCount} 处可配 · ${coverage.notDeliverableCount} 不配送`
        }
        return `${coverage.totalCount}/${coverage.totalCount} 处可配 · ${coverage.freightExtraCount} 运费另计`
    })()

    const coverageTone =
        isDirect && coverage.notDeliverableCount > 0
            ? coverage.deliverableCount === 0
                ? "text-destructive"
                : "text-warning-soft-foreground"
            : isDirect && coverage.freightExtraCount > 0
              ? "text-warning-soft-foreground"
              : "text-muted-foreground"

    return (
        <li
            className={cn(
                "rounded-lg border border-border/70 bg-card px-3 py-2 transition-colors hover:border-primary/35 hover:bg-card",
                minimum.disabled && "opacity-60",
            )}
        >
            {/* 第一行：身份 + 价格地位 + 1-click 选用/替换 */}
            <div className="flex items-start justify-between gap-2">
                <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span
                            className="truncate text-body-compact font-medium text-foreground"
                            title={name}
                        >
                            {name}
                        </span>
                        <FulfillmentRouteBadge
                            option={option}
                            label={route}
                            size="compact"
                            className="shrink-0"
                        />
                        {priceDelta?.isLowest ? (
                            <StatusBadge
                                tone="success"
                                label="最低价"
                                className="h-4.5 shrink-0 px-1.5 text-tiny"
                            />
                        ) : null}
                        {isRecommended ? (
                            <StatusBadge
                                tone="info"
                                icon={SparklesIcon}
                                label="推荐"
                                className="h-4.5 shrink-0 px-1.5 text-tiny"
                            />
                        ) : null}
                        {option.inquirySource ? (
                            <Badge
                                variant="violet"
                                className="h-4.5 shrink-0 px-1.5 text-tiny"
                            >
                                询价
                            </Badge>
                        ) : null}
                    </div>
                </div>

                <div className="flex shrink-0 items-center gap-2">
                    <div className="text-right">
                        {isStock ? (
                            <span className="text-xs font-medium text-teal-soft-foreground">
                                库存现货
                            </span>
                        ) : (
                            <div className="flex items-baseline justify-end gap-1">
                                <span
                                    className={
                                        margin?.inverted
                                            ? "text-destructive"
                                            : undefined
                                    }
                                >
                                    <MoneyValue
                                        display="exact"
                                        value={option.unitCostGross}
                                        inheritColor={Boolean(margin?.inverted)}
                                    />
                                </span>
                                {priceDelta && !priceDelta.isLowest ? (
                                    <span className="num text-tiny text-muted-foreground">
                                        +¥{priceDelta.delta}
                                    </span>
                                ) : null}
                            </div>
                        )}
                    </div>

                    <div className="flex items-center gap-1">
                        {showReplaceAction || !canSplit ? (
                            <Button
                                id={`${idPrefix}-pool-replace-${optSeg}`}
                                type="button"
                                size="xs"
                                variant={canSplit ? "ghost" : "outline"}
                                disabled={minimum.disabled}
                                title={`替换为 源${activeSourceNumber}`}
                                onClick={onReplace}
                            >
                                <RefreshCwIcon aria-hidden="true" />
                                替换
                            </Button>
                        ) : null}
                        {canSplit ? (
                            <Button
                                id={`${idPrefix}-pool-add-${optSeg}`}
                                type="button"
                                size="xs"
                                variant="secondary"
                                disabled={minimum.disabled}
                                onClick={onAdd}
                            >
                                <PlusIcon aria-hidden="true" />
                                选用
                            </Button>
                        ) : null}
                    </div>
                </div>
            </div>

            {/* 第二行：毛利 · 税率付款 · 配送覆盖 · 可供量 · 交期 */}
            <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                {margin ? (
                    <span
                        className={cn(
                            "num font-medium",
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
                    </span>
                ) : null}
                {!isStock && (taxPercent || option.paymentTermLabel) ? (
                    <span>
                        {taxPercent ? (
                            <RateValue value={taxPercent} precision={2} />
                        ) : null}
                        {option.paymentTermLabel
                            ? ` ${option.paymentTermLabel}`
                            : ""}
                    </span>
                ) : null}
                <span
                    className={cn(
                        "inline-flex items-center gap-0.5",
                        coverageTone,
                    )}
                >
                    {isDirect &&
                    (coverage.notDeliverableCount > 0 ||
                        coverage.freightExtraCount > 0) ? (
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="size-2.5 shrink-0"
                        />
                    ) : null}
                    {coverageText}
                </span>
                <span className="num">
                    可供{" "}
                    <QuantityValue value={availableQty} unit={product.unit} />
                </span>
                {!isStock && option.expectedDeliveryDate ? (
                    <span className="num">
                        交期 {option.expectedDeliveryDate}
                    </span>
                ) : null}
                {minimum.label ? (
                    <span
                        className={cn(
                            minimum.disabled && "font-medium text-destructive",
                        )}
                    >
                        {minimum.label}
                    </span>
                ) : null}
            </div>
        </li>
    )
}
