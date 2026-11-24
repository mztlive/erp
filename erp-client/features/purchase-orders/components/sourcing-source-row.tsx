"use client"

import { useMemo, useState } from "react"
import {
    CheckCircle2Icon,
    ChevronDownIcon,
    FilterIcon,
    PackageCheckIcon,
    ScissorsIcon,
    Trash2Icon,
    TriangleAlertIcon,
    ZapIcon,
} from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
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
    isPositiveQuantity,
    type GlobalAddressState,
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
import {
    comparePurchaseCostToSalesPrice,
    priceDeltaFromLowest,
} from "../lib/sourcing/pricing"
import {
    formatQuantityUnits,
    minimumUnits,
    quantityUnits,
} from "../lib/sourcing/quantity"
import { sourceAccentForIndex } from "../lib/sourcing/source-palette"
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
 * 左侧供给池中的「已选用来源」精简卡片：
 * - 头部点击「源N」或「N 处地址」胶囊即可在右侧一键筛选只看本源地址（点击卡内输入框/选择器不触发切换）；
 * - 清晰标注「最低价」或「最低 ¥X」价差提示，不再放多余的复选框、「比价换源」按钮与重复配量弹窗；
 * - 修复「目标仓地区：包邮」误用警告色的问题，仅运费另计/不配送才高亮提醒。
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
    canRemove,
    errors,
    lowestPurchaseCost,
    addressState,
    isAddressFiltered,
    onActivate,
    onRemove,
    onToggleAddressFilter,
    onFocusCheapestCandidate,
    onTrimDirectExcessForSource,
    onFillDirectShortfallForSource,
}: {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    product: SourcingProductLine
    row: SourcingEditorRow
    line: SourcingLineInput
    index: number
    sourceNumber: number
    isActive: boolean
    canRemove: boolean
    errors: readonly string[]
    lowestPurchaseCost?: string
    addressState?: GlobalAddressState | null
    isAddressFiltered?: boolean
    onActivate: () => void
    onRemove: () => void
    onToggleAddressFilter?: () => void
    onFocusCheapestCandidate?: () => void
    onTrimDirectExcessForSource?: (rowKey: string, excessQty: string) => void
    onFillDirectShortfallForSource?: (
        rowKey: string,
        shortfallQty: string,
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
    const name = isStock
        ? option.warehouseName || option.supplierName
        : option?.supplierName || "请选择供应商"
    const hasError = errors.length > 0

    const [syncWarehouseNotice, setSyncWarehouseNotice] = useState("")
    const allLines = form.state.values.lines

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
    const priceDelta =
        option && !isStock
            ? priceDeltaFromLowest(option.unitCostGross, lowestPurchaseCost)
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
    const warehouseCoveredCount =
        !addressDriven && addressState
            ? addressState.counts.unassigned
            : totalAddressCount

    const warehouseJudgement = needsWarehouse
        ? warehouseDeliveryTerm(option, line.targetWarehouseId ?? "")
        : undefined
    const warehouseIsAlert =
        warehouseJudgement?.term === "NOT_DELIVERABLE" ||
        warehouseJudgement?.term === "FREIGHT_EXTRA"

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

    const addressPillLabel = addressDriven
        ? `${assignedAddressCount} 处地址`
        : `${warehouseCoveredCount} 处统筹`

    const formContent = (
        <div className="space-y-2">
            {/* 卡顶行：点击「源N · 履约方式」或右侧「地址数」胶囊均可一键筛选右侧只看本源地址 */}
            <div className="flex items-center justify-between gap-2">
                <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                    <button
                        id={`${sourceId}-edit`}
                        type="button"
                        aria-pressed={Boolean(isAddressFiltered ?? isActive)}
                        aria-label={`编辑 ${name} · ${route}，分配数量 ${line.quantity || "待填写"}${product.unit}${hasError ? "，待调整" : ""}`}
                        onClick={() => {
                            onActivate()
                            onToggleAddressFilter?.()
                        }}
                        className={cn(
                            "inline-flex h-6 shrink-0 cursor-pointer items-center gap-1.5 rounded-sm border px-2 text-tiny font-semibold transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            isAddressFiltered || isActive
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
                        <span aria-hidden="true" className="opacity-45">
                            ·
                        </span>
                        <span className="inline-flex items-center gap-1 font-medium">
                            {isStock ? (
                                <PackageCheckIcon
                                    aria-hidden="true"
                                    className="size-3"
                                />
                            ) : null}
                            {route}
                        </span>
                    </button>

                    {priceDelta?.isLowest && product.options.length > 1 ? (
                        <Badge
                            variant="success"
                            className="h-5 shrink-0 px-1.5 text-tiny"
                        >
                            最低价
                        </Badge>
                    ) : priceDelta &&
                      !priceDelta.isLowest &&
                      lowestPurchaseCost ? (
                        <button
                            type="button"
                            onClick={() => onFocusCheapestCandidate?.()}
                            className="num inline-flex h-5 shrink-0 cursor-pointer items-center rounded-sm bg-warning-soft/80 px-1.5 text-tiny font-medium text-warning-soft-foreground transition-colors hover:bg-warning-soft focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                            title={`当前候选最低采购价为 ¥${lowestPurchaseCost}，点击查看下方候选比价`}
                        >
                            比最低贵 ¥{priceDelta.delta}
                        </button>
                    ) : null}

                    {option?.inquirySource ? (
                        <Badge
                            variant="violet"
                            className="h-5 shrink-0 px-1.5 text-tiny"
                        >
                            询价
                        </Badge>
                    ) : null}

                    <SourcingRecommendationHint
                        id={`${id}-recommendation`}
                        form={form}
                        order={order}
                        product={product}
                        index={index}
                    />

                    {!line.selected ? (
                        <button
                            type="button"
                            onClick={() =>
                                form.setFieldValue(
                                    `lines[${index}].selected`,
                                    true,
                                )
                            }
                            className="inline-flex h-5 cursor-pointer items-center rounded-sm border border-dashed border-border px-1.5 text-tiny text-muted-foreground hover:border-primary/40 hover:text-primary"
                        >
                            已暂缓 · 恢复
                        </button>
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
                    {onToggleAddressFilter ? (
                        <button
                            id={`${sourceId}-filter-addresses`}
                            type="button"
                            aria-pressed={Boolean(isAddressFiltered)}
                            onClick={() => {
                                onActivate()
                                onToggleAddressFilter()
                            }}
                            title={
                                isAddressFiltered
                                    ? "当前右侧仅看本源地址，点击恢复全部地址"
                                    : "点击在右侧仅查看本源承接的地址"
                            }
                            className={cn(
                                "num inline-flex h-5.5 cursor-pointer items-center gap-1 rounded-sm border px-1.5 text-tiny font-medium transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                isAddressFiltered
                                    ? accent.chipClass
                                    : "border-transparent bg-muted/65 text-muted-foreground hover:bg-muted hover:text-foreground",
                            )}
                        >
                            <FilterIcon
                                aria-hidden="true"
                                className="size-2.5"
                            />
                            {isAddressFiltered
                                ? `仅看 ${addressPillLabel}`
                                : addressPillLabel}
                        </button>
                    ) : null}

                    {canRemove ? (
                        <Button
                            id={`${id}-split-remove`}
                            type="button"
                            size="icon-xs"
                            variant="ghost"
                            className="text-muted-foreground hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"
                            title="移回候选池"
                            aria-label={`移除 源${sourceNumber} ${name} · ${route}`}
                            onClick={onRemove}
                        >
                            <Trash2Icon aria-hidden="true" />
                        </Button>
                    ) : null}
                </div>
            </div>

            {/* 供货方案选择器 + 一行核心商业摘要 */}
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
                                    · 仓库出库统筹
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
                                    <span className="truncate">
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
                                <div className="mt-1.5 space-y-1 border-t border-border/50 pt-1.5 text-xs text-muted-foreground">
                                    <p>
                                        {option.inquirySource
                                            ? `询价专项报价（第 ${option.inquirySource.feedback_round} 轮 · v${option.inquirySource.quote_revision_no}）`
                                            : "通用供给"}
                                        {option.delivery?.templateName
                                            ? ` · 模板 ${option.delivery.templateName}`
                                            : ""}
                                        {option.maxCreateQuantity
                                            ? ` · 可供上限 ${option.maxCreateQuantity} ${product.unit}`
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

            {/* 履约参数区 */}
            {option ? (
                <div className="space-y-2 border-t border-border/50 pt-2">
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
                            "grid gap-2.5",
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
                                <p className="text-xs font-medium text-foreground/75">
                                    直发承接
                                </p>
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

            {/* 目标仓包邮正向提示（冷静绿色，不误用警告框） */}
            {warehouseJudgement && !warehouseIsAlert && option ? (
                <div className="flex items-center gap-1.5 text-xs text-success-soft-foreground">
                    <CheckCircle2Icon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    <span>
                        目标仓地区 {warehouseJudgement.regionName}：
                        {deliveryTermLabel(option, warehouseJudgement.term)}
                    </span>
                </div>
            ) : null}

            {/* 真正的配送异常 / 起订量提醒 */}
            {warehouseIsAlert ||
            option?.delivery?.bulkMinimumOrderQuantity ||
            deliveryHasException ? (
                <div
                    className={cn(
                        "flex flex-wrap items-center gap-x-2.5 gap-y-1 rounded-md px-2.5 py-1.5 text-xs",
                        warehouseJudgement?.term === "NOT_DELIVERABLE"
                            ? "bg-destructive-soft text-destructive-soft-foreground"
                            : "bg-warning-soft/65 text-warning-soft-foreground",
                    )}
                >
                    <TriangleAlertIcon
                        aria-hidden="true"
                        className="size-3.5 shrink-0"
                    />
                    {warehouseIsAlert && warehouseJudgement && option ? (
                        <span className="font-medium">
                            目标仓地区 {warehouseJudgement.regionName}：
                            {deliveryTermLabel(option, warehouseJudgement.term)}
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
            onPointerDownCapture={onActivate}
            onFocusCapture={onActivate}
            className={cn(
                "rounded-lg border bg-card p-2.5 shadow-2xs transition-colors",
                hasError
                    ? "border-destructive/55 ring-1 ring-destructive/15"
                    : isAddressFiltered
                      ? accent.activeBorderClass
                      : isActive
                        ? "border-primary/40"
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
