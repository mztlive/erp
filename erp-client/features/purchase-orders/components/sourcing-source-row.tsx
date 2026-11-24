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
import {
    findSourcingOption,
    pickBestSourcingOption,
} from "../lib/purchase-order-create-model"
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
import {
    applySourcingOptionToLine,
    comparePurchaseCostToSalesPrice,
} from "../lib/sourcing/pricing"
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
import { SourcingOptionCompareMatrix } from "./sourcing-option-compare-matrix"
import { SourcingRecommendationHint } from "./sourcing-recommendation-hint"

/**
 * 一体化多源分配行：
 * - 所有来源同屏并排可见、直接行内编辑（供应商/履约方式、目标仓、预计交期、数量或直发配额），
 *   彻底告别「点一个源才能看一个源」的盲人摸象式拆单；
 * - 当前聚焦行挂载 `aria-label="当前来源编辑"`，行首按钮保持 `sourcing-source-*-edit` 稳定语义。
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
    defaultCompareOpen = false,
    onActivate,
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
    defaultCompareOpen?: boolean
    onActivate: () => void
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

    const [compareOpen, setCompareOpen] = useState(
        () => defaultCompareOpen || !line.basisId,
    )
    const [quotaOpen, setQuotaOpen] = useState(false)
    const [quotaQty, setQuotaQty] = useState("")
    const [quotaAllowReallocate, setQuotaAllowReallocate] = useState(true)
    const [syncWarehouseNotice, setSyncWarehouseNotice] = useState("")

    const allLines = form.state.values.lines

    const usedBasisIds = useMemo(
        () =>
            new Set(
                allLines
                    .filter(
                        (entry) =>
                            entry.salesOrderLineId === line.salesOrderLineId &&
                            entry.rowKey !== line.rowKey &&
                            entry.selected &&
                            entry.basisId,
                    )
                    .map((entry) => entry.basisId),
            ),
        [allLines, line.rowKey, line.salesOrderLineId],
    )

    const recommendedBasisId = useMemo(
        () =>
            pickBestSourcingOption(
                product.options,
                product.remainingQuantity,
                product,
            )?.basisId,
        [product],
    )

    // 计算当前行相对商品总需求的差额，提供 1-click「补齐」/「减至」快捷配平
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
        <div className="space-y-2.5">
            {/* 第一行：身份角标 + 履约方式 + 供应商方案选择器 + 比价 / 详情 / 移除 */}
            <div className="flex flex-wrap items-center gap-2">
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
                        "inline-flex h-6 shrink-0 cursor-pointer items-center gap-1.5 rounded-sm border px-1.5 text-tiny font-semibold transition-colors outline-none focus-visible:ring-2 focus-visible:ring-ring",
                        isActive
                            ? accent.chipClass
                            : "border-border/70 bg-muted/70 text-muted-foreground hover:bg-muted hover:text-foreground",
                    )}
                >
                    <span
                        aria-hidden="true"
                        className={cn("size-1.5 rounded-full", accent.barClass)}
                    />
                    <span className="num">源{sourceNumber}</span>
                </button>

                <Badge variant={routeVariant} className="h-5 shrink-0 px-1.5">
                    {isStock ? <PackageCheckIcon aria-hidden="true" /> : null}
                    {route}
                </Badge>

                <div className="min-w-52 flex-1">
                    <SourcingOptionField
                        form={form}
                        order={order}
                        index={index}
                        rowKey={line.rowKey}
                        product={product}
                        showDeliverySummary={false}
                    />
                </div>

                <div className="flex shrink-0 flex-wrap items-center gap-1.5">
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

                    {product.options.length > 1 ? (
                        <Button
                            id={`${id}-compare-toggle`}
                            type="button"
                            size="xs"
                            variant={compareOpen ? "secondary" : "outline"}
                            aria-expanded={compareOpen}
                            onClick={() => {
                                onActivate()
                                setCompareOpen((prev) => !prev)
                            }}
                        >
                            <ScaleIcon aria-hidden="true" />
                            {compareOpen
                                ? "收起比价"
                                : `比价（${product.options.length}）`}
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

            {/* 第二行：商业事实摘要（左） + 履约目标仓 / 交期 / 分配数量与一键配平（右） */}
            <Collapsible>
                <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 rounded-md bg-surface-sunken/85 px-3 py-2">
                    {/* 左侧：价格/毛利/税率/账期/可用库存 */}
                    <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                        {!option ? (
                            <span className="text-muted-foreground">
                                请先选择供货方案或展开比价表挑选。
                            </span>
                        ) : isStock ? (
                            <>
                                <span className="font-medium text-foreground">
                                    库存核算
                                </span>
                                <span className="text-muted-foreground">
                                    可用{" "}
                                    <QuantityValue
                                        className="font-medium text-foreground"
                                        value={availableCapStr}
                                        unit={product.unit}
                                    />
                                </span>
                                <span className="text-muted-foreground">
                                    由仓库出库时安排客户地址
                                </span>
                            </>
                        ) : (
                            <>
                                <span className="inline-flex items-baseline gap-1.5">
                                    <MoneyValue
                                        display="exact"
                                        value={option.unitCostGross}
                                        className={cn(
                                            "text-body-compact font-semibold",
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
                                                ? `高于售价 ¥${margin.absSpread}`
                                                : margin.marginRateText
                                                  ? `毛利 ${margin.marginRateText}`
                                                  : "平价"}
                                        </span>
                                    ) : null}
                                </span>

                                <span className="text-muted-foreground">
                                    {taxRatePercent ? (
                                        <RateValue
                                            value={taxRatePercent}
                                            precision={2}
                                        />
                                    ) : null}
                                    {option.paymentTermLabel
                                        ? ` · ${option.paymentTermLabel}`
                                        : ""}
                                </span>

                                {deliveryText ? (
                                    <span
                                        className={cn(
                                            "truncate",
                                            deliveryHasException
                                                ? "font-medium text-warning-soft-foreground"
                                                : "text-muted-foreground",
                                        )}
                                        title={deliveryText}
                                    >
                                        {deliveryText}
                                    </span>
                                ) : needsWarehouse ? (
                                    <span className="text-muted-foreground">
                                        入仓后由仓库统一发货
                                    </span>
                                ) : null}

                                <CollapsibleTrigger
                                    id={`${id}-facts-toggle`}
                                    render={
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className="group h-5 px-1 text-tiny text-muted-foreground hover:text-foreground"
                                        />
                                    }
                                >
                                    条款详情
                                    <ChevronDownIcon className="size-3 transition-transform duration-150 group-data-[panel-open]:rotate-180" />
                                </CollapsibleTrigger>
                            </>
                        )}
                    </div>

                    {/* 右侧：行内履约控件（目标仓 + 交期 + 数量/直发承接） */}
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
                        {needsWarehouse ? (
                            <div className="flex items-center gap-1.5">
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    目标仓
                                    <RequiredMark />
                                </span>
                                <div className="w-48">
                                    <SourcingTargetWarehouseField
                                        form={form}
                                        index={index}
                                        rowKey={line.rowKey}
                                        order={order}
                                        salesOrderLineId={line.salesOrderLineId}
                                    />
                                </div>
                            </div>
                        ) : null}

                        {!isStock && option ? (
                            <div className="flex items-center gap-1.5">
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    交期
                                    <RequiredMark />
                                </span>
                                <div className="w-36">
                                    <SourcingExpectedDeliveryField
                                        form={form}
                                        index={index}
                                        rowKey={line.rowKey}
                                        order={order}
                                        salesOrderLineId={line.salesOrderLineId}
                                        showDeadlineHint={false}
                                    />
                                </div>
                            </div>
                        ) : null}

                        {/* 核心分配数量控制单元 */}
                        {addressDriven ? (
                            <div className="flex flex-wrap items-center gap-1.5 border-l border-border/60 pl-3">
                                <span className="text-xs text-muted-foreground">
                                    直发量
                                </span>
                                <span className="text-body-compact font-semibold text-foreground">
                                    <QuantityValue
                                        value={line.quantity || "0"}
                                        unit={product.unit}
                                    />
                                </span>
                                <span className="num text-xs text-muted-foreground">
                                    ({assignedAddressCount}/{totalAddressCount}
                                    处)
                                </span>

                                {balanceHint?.kind === "trim" &&
                                onTrimDirectExcessForSource ? (
                                    <Button
                                        id={`${sourceId}-quick-trim`}
                                        type="button"
                                        size="xs"
                                        variant="destructive"
                                        onClick={() =>
                                            onTrimDirectExcessForSource(
                                                line.rowKey,
                                                balanceHint.delta,
                                            )
                                        }
                                    >
                                        <ScissorsIcon aria-hidden="true" />
                                        缩至 {balanceHint.target}
                                    </Button>
                                ) : null}

                                {balanceHint?.kind === "fill" &&
                                onFillDirectShortfallForSource ? (
                                    <Button
                                        id={`${sourceId}-quick-fill`}
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        onClick={() =>
                                            onFillDirectShortfallForSource(
                                                line.rowKey,
                                                balanceHint.delta,
                                            )
                                        }
                                    >
                                        <ZapIcon aria-hidden="true" />
                                        +补 {balanceHint.delta}
                                    </Button>
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
                                                <Button
                                                    id={`${sourceId}-quota-trigger`}
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    className="bg-card"
                                                />
                                            }
                                        >
                                            按量配
                                        </PopoverTrigger>
                                        <PopoverContent
                                            align="end"
                                            className="w-72 space-y-2.5 p-3.5"
                                        >
                                            <div className="space-y-0.5">
                                                <p className="text-body-compact font-semibold text-foreground">
                                                    设定 源{sourceNumber}{" "}
                                                    直发目标量
                                                </p>
                                                <p className="text-xs text-muted-foreground">
                                                    自动按数量挑选可配送地址，优先包邮地址。
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
                                                            event.target.value,
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
                                                        onCheckedChange={(c) =>
                                                            setQuotaAllowReallocate(
                                                                c === true,
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
                                                                {product.unit}
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
                                                        setQuotaOpen(false)
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
                                                            `已将 源${sourceNumber} 直发配额调整为 ${quotaPreview.allocatedQty} ${product.unit}（${quotaPreview.allocatedAddressCount} 处地址）。`,
                                                        )
                                                        setQuotaOpen(false)
                                                    }}
                                                >
                                                    应用
                                                </Button>
                                            </div>
                                        </PopoverContent>
                                    </Popover>
                                ) : null}

                                {onToggleAddressFilter &&
                                hasMultipleDirectSources ? (
                                    <Button
                                        id={`${sourceId}-filter-addresses`}
                                        type="button"
                                        size="xs"
                                        variant={
                                            isAddressFiltered
                                                ? "secondary"
                                                : "ghost"
                                        }
                                        aria-pressed={Boolean(
                                            isAddressFiltered,
                                        )}
                                        onClick={onToggleAddressFilter}
                                        title="在下方地址表中筛选此来源承接的地址"
                                    >
                                        <FilterIcon aria-hidden="true" />
                                        {isAddressFiltered
                                            ? "已筛地址"
                                            : "看地址"}
                                    </Button>
                                ) : null}
                            </div>
                        ) : (
                            <div className="flex items-center gap-1.5 border-l border-border/60 pl-3">
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    数量
                                    <RequiredMark />
                                </span>
                                <div className="w-22">
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
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    {product.unit}
                                </span>

                                {balanceHint?.kind === "fill" ? (
                                    <Button
                                        id={`${sourceId}-quick-fill`}
                                        type="button"
                                        size="xs"
                                        variant="secondary"
                                        onClick={() =>
                                            form.setFieldValue(
                                                `lines[${index}].quantity`,
                                                balanceHint.target,
                                            )
                                        }
                                        title={`自动设为 ${balanceHint.target} ${product.unit} 以填补缺口`}
                                    >
                                        填余量
                                    </Button>
                                ) : balanceHint?.kind === "trim" ? (
                                    <Button
                                        id={`${sourceId}-quick-trim`}
                                        type="button"
                                        size="xs"
                                        variant="outline"
                                        className="border-destructive-border bg-card text-destructive hover:bg-destructive-soft"
                                        onClick={() =>
                                            form.setFieldValue(
                                                `lines[${index}].quantity`,
                                                balanceHint.target,
                                            )
                                        }
                                        title={`自动减至 ${balanceHint.target} ${product.unit} 以消除超配`}
                                    >
                                        调至 {balanceHint.target}
                                    </Button>
                                ) : capSmallerThanDemand ? (
                                    <span className="num text-tiny text-muted-foreground">
                                        上限 {availableCapStr}
                                    </span>
                                ) : null}
                            </div>
                        )}
                    </div>
                </div>

                {/* 可展开商业条款详情与同仓同步 */}
                {option && !isStock ? (
                    <CollapsibleContent>
                        <div className="mt-2 flex flex-wrap items-center justify-between gap-2 rounded-md border border-border/50 bg-card px-3 py-2 text-xs text-muted-foreground">
                            <div className="space-y-0.5">
                                <p>
                                    来源类型：
                                    <span className="text-foreground">
                                        {option.inquirySource
                                            ? "询价专项报价"
                                            : "通用供给"}
                                    </span>
                                    {option.delivery?.templateName
                                        ? ` · 配送模板 ${option.delivery.templateName}`
                                        : ""}
                                    {option.maxCreateQuantity
                                        ? ` · 本方案可供上限 ${option.maxCreateQuantity} ${product.unit}`
                                        : ""}
                                </p>
                                {option.inquirySource ? (
                                    <p>
                                        沿用询价第{" "}
                                        {option.inquirySource.feedback_round} 轮
                                        · 内部报价第{" "}
                                        {option.inquirySource.quote_revision_no}{" "}
                                        版
                                    </p>
                                ) : null}
                                {option.recommendationReason ? (
                                    <p>{option.recommendationReason}</p>
                                ) : null}
                            </div>
                            {otherWarehouseLineIndexes.length > 0 &&
                            line.targetWarehouseId ? (
                                <div className="flex items-center gap-2">
                                    {syncWarehouseNotice ? (
                                        <span className="text-success-soft-foreground">
                                            {syncWarehouseNotice}
                                        </span>
                                    ) : null}
                                    <Button
                                        id={`${sourceId}-sync-warehouse`}
                                        type="button"
                                        size="xs"
                                        variant="outline"
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
                                                `已同步到其余 ${otherWarehouseLineIndexes.length} 条入仓明细`,
                                            )
                                        }}
                                    >
                                        目标仓同步到其余{" "}
                                        {otherWarehouseLineIndexes.length}{" "}
                                        条入仓明细
                                    </Button>
                                </div>
                            ) : null}
                        </div>
                    </CollapsibleContent>
                ) : null}
            </Collapsible>

            {/* 配送 / 起订量 / 目标仓地区提醒 */}
            {warehouseJudgement ||
            option?.delivery?.bulkMinimumOrderQuantity ||
            deliveryHasException ? (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border border-warning-border/80 bg-warning-soft/60 px-3 py-1.5 text-xs text-warning-soft-foreground">
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
                    className="flex items-start gap-1.5 rounded-md border border-destructive-border bg-destructive-soft/80 px-3 py-1.5 text-xs text-destructive-soft-foreground"
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

            {/* 展开比价换源矩阵 */}
            {compareOpen && product.options.length > 0 ? (
                <div className="space-y-1.5 pt-1">
                    <div className="flex items-center justify-between gap-2">
                        <p className="text-xs font-medium text-foreground">
                            切换 源{sourceNumber}{" "}
                            的履约方案（点击任意可用方案直接替换）
                        </p>
                        <Button
                            id={`${id}-compare-close`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            onClick={() => setCompareOpen(false)}
                        >
                            收起
                        </Button>
                    </div>
                    <SourcingOptionCompareMatrix
                        idPrefix={id}
                        order={order}
                        product={product}
                        selectedBasisId={line.basisId}
                        usedBasisIds={usedBasisIds}
                        recommendedBasisId={recommendedBasisId}
                        mode="replace"
                        onSelect={(picked) => {
                            applySourcingOptionToLine(
                                form,
                                product,
                                index,
                                picked,
                            )
                            setCompareOpen(false)
                        }}
                    />
                </div>
            ) : null}
        </div>
    )

    return (
        <div
            onFocusCapture={onActivate}
            className={cn(
                "rounded-lg border bg-card px-3.5 py-2.5 transition-colors",
                hasError
                    ? "border-destructive/55 ring-1 ring-destructive/15"
                    : isActive && isMultiSource
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
