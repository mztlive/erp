"use client"

import { useMemo, useState } from "react"
import {
    ChevronDownIcon,
    FilterIcon,
    PackageCheckIcon,
    ScaleIcon,
    ScissorsIcon,
    Trash2Icon,
    TriangleAlertIcon,
    ZapIcon,
} from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Input } from "@/components/ui/input"
import {
    Popover,
    PopoverContent,
    PopoverTrigger,
} from "@/components/ui/popover"
import { RequiredMark } from "@/components/ui/required-mark"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    multiplyFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import {
    computeQuotaAutoAllocation,
    isPositiveQuantity,
    type GlobalAddressState,
    type LineUpdatePatch,
} from "../lib/sourcing/address-grouping"
import { sourcingDeliveryAllocations } from "../lib/sourcing/delivery"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"
import {
    deliverySummary,
    deliveryTermLabel,
    sourcingRouteLabel,
    warehouseDeliveryTerm,
} from "../lib/sourcing/delivery-terms"
import type { SourcingEditorRow } from "../lib/sourcing/editor-rows"
import { comparePurchaseCostToSalesPrice } from "../lib/sourcing/pricing"
import {
    formatQuantityUnits,
    minimumUnits,
    quantityUnits,
} from "../lib/sourcing/quantity"
import {
    sourceAccentForIndex,
    sourceRouteBadgeVariant,
} from "../lib/sourcing/source-palette"
import type {
    SourcingLineInput,
    SourcingProductLine,
    SourcingSalesOrder,
} from "../lib/sourcing/types"
import {
    SourcingExpectedDeliveryField,
    SourcingOptionField,
    SourcingTargetWarehouseField,
} from "./create-sourcing/sourcing-fields"
import { SourcingRecommendationHint } from "./sourcing-recommendation-hint"

/**
 * 左栏清爽单源卡片：
 * - 单层纯白卡片，告别「卡片套灰盒」的视觉堆砌；
 * - 全部来源同屏并排可见、直接行内改供应商/目标仓/交期/数量或直发配额；
 * - 点击「比价」平滑联动右侧全高比价矩阵，不再在窄卡内撑开嵌套表格。
 */
export function SourcingSourceRow({
    form,
    order,
    product,
    row,
    line,
    index,
    sourceNumber,
    isActive,
    isMultiSource,
    canRemove,
    errors,
    addressState,
    isAddressFiltered,
    isCompareTarget,
    onActivate,
    onOpenCompare,
    onRemove,
    onToggleAddressFilter,
    onTrimDirectExcessForSource,
    onFillDirectShortfallForSource,
    onApplyPatches,
}: {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    product: SourcingProductLine
    row: SourcingEditorRow
    line: SourcingLineInput
    index: number
    sourceNumber: number
    isActive: boolean
    isMultiSource: boolean
    canRemove: boolean
    errors: readonly string[]
    addressState?: GlobalAddressState | null
    isAddressFiltered?: boolean
    isCompareTarget?: boolean
    onActivate: () => void
    onOpenCompare: () => void
    onRemove: () => void
    onToggleAddressFilter?: () => void
    onTrimDirectExcessForSource?: (rowKey: string, excessQty: string) => void
    onFillDirectShortfallForSource?: (
        rowKey: string,
        shortfallQty: string,
    ) => void
    onApplyPatches?: (
        patches: readonly LineUpdatePatch[],
        feedback?: string,
    ) => void
}) {
    const option = findSourcingOption(product, line.basisId)
    const isStock = option?.sourceType === "EXISTING_STOCK"
    const needsWarehouse =
        option?.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility === "WAREHOUSE"
    const addressDriven = usesAddressQuantity(product, line)
    const rowSeg = toAutomationIdSegment(line.rowKey)
    const id = `procurement-orders-create-row-${rowSeg}`
    const sourceId = `sourcing-source-${rowSeg}`
    const accent = sourceAccentForIndex(sourceNumber - 1)
    const route = option ? sourcingRouteLabel(option) : "待选方案"
    const routeVariant = sourceRouteBadgeVariant(option)
    const name = isStock
        ? option.warehouseName || option.supplierName
        : option?.supplierName || "请选择供应商"
    const hasError = errors.length > 0

    const [quotaOpen, setQuotaOpen] = useState(false)
    const [quotaQty, setQuotaQty] = useState("")
    const [quotaAllowReallocate, setQuotaAllowReallocate] = useState(true)
    const [syncWarehouseNotice, setSyncWarehouseNotice] = useState("")

    const allLines = form.state.values.lines

    // 计算当前行相对商品总需求的差额，提供 1-click「补齐」/「调至」快捷配平
    const balanceHint = useMemo(() => {
        if (!line.selected || !option) return null
        try {
            const demandUnits = quantityUnits(product.remainingQuantity)
            const totalAllocated = sumFixed(
                row.allocations
                    .filter(({ line: entry }) => entry.selected)
                    .map(({ line: entry }) => entry.quantity || "0"),
                { maxScale: 6, outputScale: 6 },
            )
            const totalUnits = quantityUnits(totalAllocated)
            const selfUnits = isPositiveQuantity(line.quantity || "0")
                ? quantityUnits(compactFixed(line.quantity.trim()))
                : BigInt(0)
            const capStr = isStock
                ? (option.sourceAvailableQuantity ?? option.maxCreateQuantity)
                : option.maxCreateQuantity
            const capUnits = quantityUnits(capStr)

            if (totalUnits < demandUnits) {
                const shortfall = demandUnits - totalUnits
                const maxAdd =
                    capUnits > selfUnits ? capUnits - selfUnits : BigInt(0)
                const addable = minimumUnits(shortfall, maxAdd)
                if (addable > BigInt(0)) {
                    return {
                        kind: "fill" as const,
                        delta: formatQuantityUnits(addable),
                        target: formatQuantityUnits(selfUnits + addable),
                    }
                }
            } else if (totalUnits > demandUnits && selfUnits > BigInt(0)) {
                const excess = totalUnits - demandUnits
                const reducible = minimumUnits(excess, selfUnits)
                if (reducible > BigInt(0)) {
                    return {
                        kind: "trim" as const,
                        delta: formatQuantityUnits(reducible),
                        target: formatQuantityUnits(selfUnits - reducible),
                    }
                }
            }
        } catch {
            return null
        }
        return null
    }, [
        isStock,
        line.quantity,
        line.selected,
        option,
        product,
        row.allocations,
    ])

    // 同步目标仓到同销售单其余入仓明细
    const otherWarehouseLineIndexes = useMemo(() => {
        if (!needsWarehouse || !line.targetWarehouseId) return []
        return allLines.flatMap((candidate, candidateIndex) => {
            if (
                !candidate.selected ||
                candidate.salesOrderLineId === product.salesOrderLineId
            ) {
                return []
            }
            const otherProduct = order.lines.find(
                (p) => p.salesOrderLineId === candidate.salesOrderLineId,
            )
            const otherOpt = findSourcingOption(otherProduct, candidate.basisId)
            return otherOpt?.sourceType === "PURCHASE" &&
                otherOpt.fulfillmentResponsibility === "WAREHOUSE"
                ? [candidateIndex]
                : []
        })
    }, [
        allLines,
        line.targetWarehouseId,
        needsWarehouse,
        order.lines,
        product.salesOrderLineId,
    ])

    const salesPrice = product.salesPrices?.unit_price_gross?.trim()
    const margin =
        option && !isStock
            ? comparePurchaseCostToSalesPrice(option.unitCostGross, salesPrice)
            : null
    const taxRatePercent =
        option && !isStock
            ? multiplyFixed(option.inputTaxRate, "100", {
                  leftMaxScale: 6,
                  rightMaxScale: 0,
                  outputScale: 2,
              })
            : null
    const deliveryText =
        option && !isStock ? deliverySummary(option, product) : ""
    const deliveryHasException =
        Boolean(deliveryText) && !deliveryText.startsWith("全部地址")
    const assignedAddressCount = addressDriven
        ? sourcingDeliveryAllocations(product, line).length
        : 0
    const totalAddressCount = product.deliveryAllocations.length
    const hasMultipleDirectSources =
        (addressState?.directSources.length ?? 0) > 1

    const quotaPreview = useMemo(() => {
        if (!quotaOpen || !addressDriven) return null
        return computeQuotaAutoAllocation(
            product,
            allLines,
            line.rowKey,
            quotaQty,
            {
                preferFreightIncluded: true,
                allowReallocateFromOtherDirect:
                    hasMultipleDirectSources && quotaAllowReallocate,
            },
        )
    }, [
        addressDriven,
        allLines,
        hasMultipleDirectSources,
        line.rowKey,
        product,
        quotaAllowReallocate,
        quotaOpen,
        quotaQty,
    ])

    const warehouseJudgement = needsWarehouse
        ? warehouseDeliveryTerm(option, line.targetWarehouseId ?? "")
        : undefined

    const availableCapStr = option
        ? isStock
            ? (option.sourceAvailableQuantity ?? option.maxCreateQuantity)
            : option.maxCreateQuantity
        : ""
    const capSmallerThanDemand = (() => {
        if (!availableCapStr) return false
        try {
            return (
                compareDecimal(availableCapStr, product.remainingQuantity, 6) <
                0
            )
        } catch {
            return false
        }
    })()

    const formContent = (
        <div className="space-y-3">
            {/* 卡顶行：源身份 + 履约类型 + 推荐 + 比价/移除 */}
            <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                    {isMultiSource ? (
                        <form.AppField name={`lines[${index}].selected`}>
                            {(field) => (
                                <Checkbox
                                    id={`${id}-select`}
                                    checked={field.state.value === true}
                                    onCheckedChange={(checked) =>
                                        field.handleChange(checked === true)
                                    }
                                    aria-label={`本次使用 源${sourceNumber} ${name}`}
                                    data-testid={`purchase-sourcing-selected-${line.rowKey}`}
                                />
                            )}
                        </form.AppField>
                    ) : null}

                    <button
                        id={`${sourceId}-edit`}
                        type="button"
                        aria-pressed={isActive}
                        aria-label={`编辑 ${name} · ${route}，分配数量 ${line.quantity || "待填写"}${product.unit}${hasError ? "，待调整" : ""}`}
                        onClick={onActivate}
                        className={cn(
                            "inline-flex h-5.5 shrink-0 cursor-pointer items-center gap-1.5 rounded-sm border px-1.5 text-tiny font-semibold transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            isActive
                                ? accent.chipClass
                                : "border-border/70 bg-muted/70 text-muted-foreground hover:bg-muted hover:text-foreground",
                        )}
                    >
                        <span
                            aria-hidden="true"
                            className={cn(
                                "size-1.5 rounded-full",
                                accent.barClass,
                            )}
                        />
                        <span className="num">源{sourceNumber}</span>
                    </button>

                    <Badge
                        variant={routeVariant}
                        className="h-5 shrink-0 px-1.5"
                    >
                        {isStock ? (
                            <PackageCheckIcon aria-hidden="true" />
                        ) : null}
                        {route}
                    </Badge>

                    <SourcingRecommendationHint
                        id={`${id}-recommendation`}
                        form={form}
                        order={order}
                        product={product}
                        index={index}
                    />

                    {!line.selected ? (
                        <Badge variant="outline" className="h-5 text-tiny">
                            暂不用
                        </Badge>
                    ) : null}

                    {hasError ? (
                        <StatusBadge
                            tone="destructive"
                            appearance="text"
                            label="待调整"
                            className="h-5 text-tiny"
                        />
                    ) : null}
                </div>

                <div className="flex shrink-0 items-center gap-1">
                    {product.options.length > 1 ? (
                        <Button
                            id={`${id}-compare-toggle`}
                            type="button"
                            size="xs"
                            variant={isCompareTarget ? "secondary" : "ghost"}
                            onClick={() => {
                                onActivate()
                                onOpenCompare()
                            }}
                        >
                            <ScaleIcon aria-hidden="true" />
                            比价换源
                        </Button>
                    ) : null}

                    {canRemove ? (
                        <Button
                            id={`${id}-split-remove`}
                            type="button"
                            size="icon-xs"
                            variant="ghost"
                            className="text-muted-foreground hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"
                            title="移除此来源"
                            aria-label={`移除 源${sourceNumber} ${name} · ${route}`}
                            onClick={onRemove}
                        >
                            <Trash2Icon aria-hidden="true" />
                        </Button>
                    ) : null}
                </div>
            </div>

            {/* 供货方案选择器 + 一行静谧商业摘要 */}
            <div className="space-y-1.5">
                <SourcingOptionField
                    form={form}
                    order={order}
                    index={index}
                    rowKey={line.rowKey}
                    product={product}
                    showDeliverySummary={false}
                />

                {option ? (
                    <Collapsible>
                        <div className="flex flex-wrap items-center justify-between gap-x-2 gap-y-1 text-xs text-muted-foreground">
                            {isStock ? (
                                <span>
                                    库存核算 · 可用{" "}
                                    <QuantityValue
                                        className="font-medium text-foreground"
                                        value={availableCapStr}
                                        unit={product.unit}
                                    />{" "}
                                    · 仓库出库时选址
                                </span>
                            ) : (
                                <div className="flex min-w-0 flex-wrap items-center gap-x-2 gap-y-0.5">
                                    <span className="inline-flex items-baseline gap-1">
                                        <MoneyValue
                                            display="exact"
                                            value={option.unitCostGross}
                                            className={cn(
                                                "font-semibold",
                                                margin?.inverted
                                                    ? "text-destructive"
                                                    : "text-foreground",
                                            )}
                                        />
                                        {margin ? (
                                            <span
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
                                            </span>
                                        ) : null}
                                    </span>
                                    <span aria-hidden="true">·</span>
                                    <span>
                                        {taxRatePercent ? (
                                            <RateValue
                                                value={taxRatePercent}
                                                precision={2}
                                            />
                                        ) : null}
                                        {option.paymentTermLabel
                                            ? ` ${option.paymentTermLabel}`
                                            : ""}
                                    </span>
                                </div>
                            )}

                            {!isStock ? (
                                <CollapsibleTrigger
                                    id={`${id}-facts-toggle`}
                                    render={
                                        <button
                                            type="button"
                                            aria-label={`展开或收起 源${sourceNumber} 条款详情`}
                                            className="group inline-flex cursor-pointer items-center gap-0.5 text-tiny text-muted-foreground hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                        />
                                    }
                                >
                                    详情
                                    <ChevronDownIcon className="size-3 transition-transform duration-150 group-data-[panel-open]:rotate-180" />
                                </CollapsibleTrigger>
                            ) : null}
                        </div>

                        {!isStock ? (
                            <CollapsibleContent>
                                <div className="mt-2 space-y-1 border-t border-border/50 pt-2 text-xs text-muted-foreground">
                                    <p>
                                        {option.inquirySource
                                            ? `询价专项报价（第 ${option.inquirySource.feedback_round} 轮 · v${option.inquirySource.quote_revision_no}）`
                                            : "通用供给"}
                                        {option.delivery?.templateName
                                            ? ` · 模板 ${option.delivery.templateName}`
                                            : ""}
                                        {option.maxCreateQuantity
                                            ? ` · 上限 ${option.maxCreateQuantity} ${product.unit}`
                                            : ""}
                                    </p>
                                    {deliveryText ? (
                                        <p>{deliveryText}</p>
                                    ) : null}
                                    {option.recommendationReason ? (
                                        <p>{option.recommendationReason}</p>
                                    ) : null}
                                </div>
                            </CollapsibleContent>
                        ) : null}
                    </Collapsible>
                ) : null}
            </div>

            {/* 履约参数区：干净 2 列网格，无嵌套灰框 */}
            {option ? (
                <div className="space-y-2.5 border-t border-border/50 pt-2.5">
                    {needsWarehouse ? (
                        <div className="space-y-1">
                            <div className="flex items-center justify-between gap-2">
                                <span className="text-xs font-medium text-foreground/75">
                                    采购入库目标仓
                                    <RequiredMark />
                                </span>
                                {otherWarehouseLineIndexes.length > 0 &&
                                line.targetWarehouseId ? (
                                    <button
                                        id={`${sourceId}-sync-warehouse`}
                                        type="button"
                                        onClick={() => {
                                            for (const targetIndex of otherWarehouseLineIndexes) {
                                                form.setFieldValue(
                                                    `lines[${targetIndex}].targetWarehouseId`,
                                                    line.targetWarehouseId,
                                                )
                                                form.setFieldValue(
                                                    `lines[${targetIndex}].targetWarehouseName`,
                                                    line.targetWarehouseName,
                                                )
                                            }
                                            setSyncWarehouseNotice(
                                                `已同步 ${otherWarehouseLineIndexes.length} 条`,
                                            )
                                        }}
                                        className="cursor-pointer text-tiny font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                    >
                                        {syncWarehouseNotice ||
                                            `同步到其余 ${otherWarehouseLineIndexes.length} 条入仓`}
                                    </button>
                                ) : null}
                            </div>
                            <SourcingTargetWarehouseField
                                form={form}
                                index={index}
                                rowKey={line.rowKey}
                                order={order}
                                salesOrderLineId={line.salesOrderLineId}
                            />
                        </div>
                    ) : null}

                    <div
                        className={cn(
                            "grid gap-3",
                            isStock ? "grid-cols-1" : "grid-cols-2",
                        )}
                    >
                        {!isStock ? (
                            <div className="min-w-0 space-y-1">
                                <p className="text-xs font-medium text-foreground/75">
                                    预计交付日
                                    <RequiredMark />
                                </p>
                                <SourcingExpectedDeliveryField
                                    form={form}
                                    index={index}
                                    rowKey={line.rowKey}
                                    order={order}
                                    salesOrderLineId={line.salesOrderLineId}
                                    showDeadlineHint={false}
                                />
                            </div>
                        ) : null}

                        {/* 数量 / 直发承接 */}
                        {addressDriven ? (
                            <div className="min-w-0 space-y-1">
                                <div className="flex items-center justify-between gap-1">
                                    <p className="text-xs font-medium text-foreground/75">
                                        直发承接
                                    </p>
                                    {onToggleAddressFilter &&
                                    hasMultipleDirectSources ? (
                                        <button
                                            id={`${sourceId}-filter-addresses`}
                                            type="button"
                                            aria-pressed={Boolean(
                                                isAddressFiltered,
                                            )}
                                            onClick={onToggleAddressFilter}
                                            className={cn(
                                                "inline-flex cursor-pointer items-center gap-0.5 text-tiny font-medium hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none",
                                                isAddressFiltered
                                                    ? "text-primary"
                                                    : "text-muted-foreground hover:text-foreground",
                                            )}
                                        >
                                            <FilterIcon className="size-2.5" />
                                            {isAddressFiltered
                                                ? "已筛地址"
                                                : "筛右侧地址"}
                                        </button>
                                    ) : null}
                                </div>
                                <div className="flex min-h-control flex-wrap items-center justify-between gap-1.5 rounded-md border border-border/70 bg-surface-sunken/70 px-2.5">
                                    <div className="flex items-baseline gap-1">
                                        <span className="text-body-compact font-semibold text-foreground">
                                            <QuantityValue
                                                value={line.quantity || "0"}
                                                unit={product.unit}
                                            />
                                        </span>
                                        <span className="num text-tiny text-muted-foreground">
                                            {assignedAddressCount}/
                                            {totalAddressCount}处
                                        </span>
                                    </div>

                                    <div className="flex items-center gap-1">
                                        {balanceHint?.kind === "trim" &&
                                        onTrimDirectExcessForSource ? (
                                            <button
                                                id={`${sourceId}-quick-trim`}
                                                type="button"
                                                onClick={() =>
                                                    onTrimDirectExcessForSource(
                                                        line.rowKey,
                                                        balanceHint.delta,
                                                    )
                                                }
                                                className="inline-flex cursor-pointer items-center gap-0.5 rounded-sm bg-destructive-soft px-1.5 py-0.5 text-tiny font-medium text-destructive-soft-foreground hover:bg-destructive-soft/75 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                            >
                                                <ScissorsIcon className="size-2.5" />
                                                缩至 {balanceHint.target}
                                            </button>
                                        ) : balanceHint?.kind === "fill" &&
                                          onFillDirectShortfallForSource ? (
                                            <button
                                                id={`${sourceId}-quick-fill`}
                                                type="button"
                                                onClick={() =>
                                                    onFillDirectShortfallForSource(
                                                        line.rowKey,
                                                        balanceHint.delta,
                                                    )
                                                }
                                                className="inline-flex cursor-pointer items-center gap-0.5 rounded-sm bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-soft-foreground hover:bg-info-soft/75 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                            >
                                                <ZapIcon className="size-2.5" />
                                                +补 {balanceHint.delta}
                                            </button>
                                        ) : null}

                                        {onApplyPatches ? (
                                            <Popover
                                                open={quotaOpen}
                                                onOpenChange={(next) => {
                                                    setQuotaOpen(next)
                                                    if (next) {
                                                        setQuotaQty(
                                                            balanceHint?.target ??
                                                                line.quantity ??
                                                                "",
                                                        )
                                                    }
                                                }}
                                            >
                                                <PopoverTrigger
                                                    render={
                                                        <button
                                                            id={`${sourceId}-quota-trigger`}
                                                            type="button"
                                                            aria-label={`按目标量自动配置 源${sourceNumber} 直发地址`}
                                                            className="cursor-pointer rounded-sm px-1 py-0.5 text-tiny font-medium text-primary hover:bg-primary/10 focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                                        />
                                                    }
                                                >
                                                    配量
                                                </PopoverTrigger>
                                                <PopoverContent
                                                    align="end"
                                                    className="w-72 space-y-2.5 p-3.5"
                                                >
                                                    <div className="space-y-0.5">
                                                        <p className="text-body-compact font-semibold text-foreground">
                                                            设定 源
                                                            {sourceNumber}{" "}
                                                            直发目标量
                                                        </p>
                                                        <p className="text-xs text-muted-foreground">
                                                            按量自动挑选可配送地址，优先包邮地区。
                                                        </p>
                                                    </div>
                                                    <div className="flex items-center gap-2">
                                                        <Input
                                                            id={`${sourceId}-quota-input`}
                                                            type="number"
                                                            inputMode="decimal"
                                                            min="0"
                                                            step={
                                                                sourcingQuantityStep(
                                                                    product.quantityScale,
                                                                ) ?? "1"
                                                            }
                                                            placeholder={`例如 ${addressState?.directTargetQty ?? product.remainingQuantity}`}
                                                            className="num h-control-sm flex-1"
                                                            value={quotaQty}
                                                            onChange={(event) =>
                                                                setQuotaQty(
                                                                    event.target
                                                                        .value,
                                                                )
                                                            }
                                                        />
                                                        <span className="text-xs text-muted-foreground">
                                                            {product.unit}
                                                        </span>
                                                    </div>
                                                    {hasMultipleDirectSources ? (
                                                        <label
                                                            htmlFor={`${sourceId}-quota-reallocate`}
                                                            className="flex cursor-pointer items-center gap-2 text-xs text-foreground"
                                                        >
                                                            <Checkbox
                                                                id={`${sourceId}-quota-reallocate`}
                                                                checked={
                                                                    quotaAllowReallocate
                                                                }
                                                                onCheckedChange={(
                                                                    c,
                                                                ) =>
                                                                    setQuotaAllowReallocate(
                                                                        c ===
                                                                            true,
                                                                    )
                                                                }
                                                            />
                                                            余量不足时从其他直发源调拨
                                                        </label>
                                                    ) : null}
                                                    {quotaPreview ? (
                                                        <div
                                                            className={cn(
                                                                "rounded-md px-2.5 py-1.5 text-xs",
                                                                quotaPreview.valid
                                                                    ? "bg-surface-sunken text-foreground"
                                                                    : "bg-destructive-soft text-destructive-soft-foreground",
                                                            )}
                                                        >
                                                            {quotaPreview.valid ? (
                                                                <>
                                                                    将分配{" "}
                                                                    <strong className="num font-semibold">
                                                                        {
                                                                            quotaPreview.allocatedAddressCount
                                                                        }{" "}
                                                                        处
                                                                    </strong>
                                                                    ，合计{" "}
                                                                    <strong className="num font-semibold">
                                                                        {
                                                                            quotaPreview.allocatedQty
                                                                        }{" "}
                                                                        {
                                                                            product.unit
                                                                        }
                                                                    </strong>
                                                                    {isPositiveQuantity(
                                                                        quotaPreview.shortfallQty,
                                                                    ) ? (
                                                                        <span className="block text-warning-soft-foreground">
                                                                            可配地址不足，尚差{" "}
                                                                            {
                                                                                quotaPreview.shortfallQty
                                                                            }{" "}
                                                                            {
                                                                                product.unit
                                                                            }
                                                                        </span>
                                                                    ) : null}
                                                                </>
                                                            ) : (
                                                                quotaPreview.error
                                                            )}
                                                        </div>
                                                    ) : null}
                                                    <div className="flex justify-end gap-1.5">
                                                        <Button
                                                            id={`${sourceId}-quota-cancel`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            onClick={() =>
                                                                setQuotaOpen(
                                                                    false,
                                                                )
                                                            }
                                                        >
                                                            取消
                                                        </Button>
                                                        <Button
                                                            id={`${sourceId}-quota-apply`}
                                                            type="button"
                                                            size="xs"
                                                            disabled={
                                                                !quotaPreview?.valid
                                                            }
                                                            onClick={() => {
                                                                if (
                                                                    !quotaPreview?.valid
                                                                )
                                                                    return
                                                                onApplyPatches(
                                                                    quotaPreview.patches,
                                                                    `已将 源${sourceNumber} 直发配额设为 ${quotaPreview.allocatedQty} ${product.unit}（${quotaPreview.allocatedAddressCount} 处地址）。`,
                                                                )
                                                                setQuotaOpen(
                                                                    false,
                                                                )
                                                            }}
                                                        >
                                                            应用
                                                        </Button>
                                                    </div>
                                                </PopoverContent>
                                            </Popover>
                                        ) : null}
                                    </div>
                                </div>
                            </div>
                        ) : (
                            <div className="min-w-0 space-y-1">
                                <div className="flex items-center justify-between gap-1">
                                    <p className="text-xs font-medium text-foreground/75">
                                        分配数量（{product.unit}）
                                        <RequiredMark />
                                    </p>
                                    {balanceHint?.kind === "fill" ? (
                                        <button
                                            id={`${sourceId}-quick-fill`}
                                            type="button"
                                            onClick={() =>
                                                form.setFieldValue(
                                                    `lines[${index}].quantity`,
                                                    balanceHint.target,
                                                )
                                            }
                                            className="cursor-pointer text-tiny font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                        >
                                            填余量 ({balanceHint.target})
                                        </button>
                                    ) : balanceHint?.kind === "trim" ? (
                                        <button
                                            id={`${sourceId}-quick-trim`}
                                            type="button"
                                            onClick={() =>
                                                form.setFieldValue(
                                                    `lines[${index}].quantity`,
                                                    balanceHint.target,
                                                )
                                            }
                                            className="cursor-pointer text-tiny font-medium text-destructive hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                        >
                                            调至 {balanceHint.target}
                                        </button>
                                    ) : capSmallerThanDemand ? (
                                        <span className="num text-tiny text-muted-foreground">
                                            上限 {availableCapStr}
                                        </span>
                                    ) : null}
                                </div>
                                <form.AppField
                                    name={`lines[${index}].quantity`}
                                >
                                    {(field) => (
                                        <field.TextField
                                            id={`${id}-quantity`}
                                            label={`本次分配数量，${product.itemName}`}
                                            required
                                            hideLabel
                                            type="number"
                                            inputMode="decimal"
                                            min="0"
                                            step={
                                                sourcingQuantityStep(
                                                    product.quantityScale,
                                                ) ?? "1"
                                            }
                                            inputClassName="num text-right"
                                            testId={`purchase-sourcing-quantity-${line.rowKey}`}
                                        />
                                    )}
                                </form.AppField>
                            </div>
                        )}
                    </div>
                </div>
            ) : null}

            {/* 配送 / 起订量 / 目标仓地区提醒 */}
            {warehouseJudgement ||
            option?.delivery?.bulkMinimumOrderQuantity ||
            deliveryHasException ? (
                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 rounded-md bg-warning-soft/65 px-2.5 py-1.5 text-xs text-warning-soft-foreground">
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    {warehouseJudgement ? (
                        <span
                            className={cn(
                                warehouseJudgement.term === "NOT_DELIVERABLE" &&
                                    "font-medium text-destructive",
                            )}
                        >
                            目标仓地区 {warehouseJudgement.regionName}：
                            {option
                                ? deliveryTermLabel(
                                      option,
                                      warehouseJudgement.term,
                                  )
                                : ""}
                        </span>
                    ) : null}
                    {deliveryHasException ? <span>{deliveryText}</span> : null}
                    {option?.delivery?.bulkMinimumOrderQuantity ? (
                        <span>
                            集采起订量{" "}
                            <QuantityValue
                                value={option.delivery.bulkMinimumOrderQuantity}
                                unit={product.unit}
                                tone="warning"
                            />
                        </span>
                    ) : null}
                </div>
            ) : null}

            {/* 行内校验错误 */}
            {hasError ? (
                <div
                    role="alert"
                    className="flex items-start gap-1.5 rounded-md bg-destructive-soft px-2.5 py-1.5 text-xs text-destructive-soft-foreground"
                >
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0"
                    />
                    <div className="space-y-0.5">
                        {errors.map((msg) => (
                            <p key={msg}>{msg}</p>
                        ))}
                    </div>
                </div>
            ) : null}
        </div>
    )

    return (
        <div
            onFocusCapture={onActivate}
            className={cn(
                "rounded-lg border bg-card p-3.5 shadow-2xs transition-colors",
                hasError
                    ? "border-destructive/55 ring-1 ring-destructive/15"
                    : isActive
                      ? accent.activeBorderClass
                      : "border-border/75 hover:border-border",
                !line.selected && "border-dashed opacity-70",
            )}
        >
            {isActive ? (
                <section
                    id="sourcing-product-adjust-detail"
                    aria-label="当前来源编辑"
                >
                    {formContent}
                </section>
            ) : (
                formContent
            )}
        </div>
    )
}
