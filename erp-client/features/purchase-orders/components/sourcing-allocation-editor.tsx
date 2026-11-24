"use client"

import { useMemo, useState } from "react"
import {
    ArrowRightIcon,
    ChevronDownIcon,
    MapPinIcon,
    PackageCheckIcon,
    PlusIcon,
    ScaleIcon,
    Trash2Icon,
    TriangleAlertIcon,
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
import { RequiredMark } from "@/components/ui/required-mark"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    findSourcingOption,
    pickBestSourcingOption,
} from "../lib/purchase-order-create-model"
import type {
    SourcingLineInput,
    SourcingProductLine,
} from "../lib/purchase-order-create-model"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import { sourcingDeliveryAllocations } from "../lib/sourcing/delivery"
import { usesAddressQuantity } from "../lib/sourcing/delivery-editor"
import {
    deliverySummary,
    deliveryTermLabel,
    sourcingRouteLabel,
    warehouseDeliveryTerm,
} from "../lib/sourcing/delivery-terms"
import {
    applySourcingOptionToLine,
    comparePurchaseCostToSalesPrice,
} from "../lib/sourcing/pricing"
import {
    SourcingExpectedDeliveryField,
    SourcingOptionField,
    SourcingTargetWarehouseField,
} from "./create-sourcing/sourcing-fields"
import type { SourcingEditorProps } from "./create-sourcing/types"
import { SourcingDeliveryEditor } from "./sourcing-delivery-plan"
import { SourcingOptionCompareMatrix } from "./sourcing-option-compare-matrix"
import { SourcingRecommendationHint } from "./sourcing-recommendation-hint"

/**
 * 步骤 ① 供货方案编辑卡：
 * 单源与多源共用一张结构化配置卡，收敛价格/毛利/税率/账期/配送覆盖，
 * 消除「来源卡 + 下拉框 + 灰色条款条」三层重复；点击「比价选源」在下方全宽展开对比矩阵。
 */
export function SourcingAllocationEditor({
    form,
    order,
    product,
    line,
    index,
    canRemove,
    onRemoveSplit,
    onAddSplit,
    canAddSplit = false,
    addSplitDisabledReason,
    onGoToAddresses,
    hideAddressEditor = false,
    compact = false,
    condensed = false,
    expandDeliverySummary = false,
    showParticipation = true,
    participationLabel = "本次使用此来源",
    defaultCompareOpen = false,
}: Pick<SourcingEditorProps, "form" | "order" | "onRemoveSplit"> & {
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
    canRemove: boolean
    onAddSplit?: () => void
    canAddSplit?: boolean
    addSplitDisabledReason?: string
    onGoToAddresses?: () => void
    /** 两步弹窗中地址分配由步骤 ② 独立承接，此处仅显示汇总并引导跳转。 */
    hideAddressEditor?: boolean
    compact?: boolean
    condensed?: boolean
    expandDeliverySummary?: boolean
    showParticipation?: boolean
    participationLabel?: string
    defaultCompareOpen?: boolean
}) {
    const option = findSourcingOption(product, line.basisId)
    const isStock = option?.sourceType === "EXISTING_STOCK"
    const needsWarehouse =
        option?.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility === "WAREHOUSE"
    const addressDriven =
        expandDeliverySummary && usesAddressQuantity(product, line)
    const id = `procurement-orders-create-row-${toAutomationIdSegment(line.rowKey)}`

    const [compareOpen, setCompareOpen] = useState(
        () => defaultCompareOpen || !line.basisId,
    )

    const usedBasisIds = useMemo(
        () =>
            new Set(
                form.state.values.lines
                    .filter(
                        (entry) =>
                            entry.salesOrderLineId === line.salesOrderLineId &&
                            entry.rowKey !== line.rowKey &&
                            entry.selected &&
                            entry.basisId,
                    )
                    .map((entry) => entry.basisId),
            ),
        [form.state.values.lines, line.rowKey, line.salesOrderLineId],
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

    return (
        <div
            key={line.rowKey}
            className={cn(!line.selected && "bg-surface-sunken/35")}
        >
            <div
                className={cn(
                    "space-y-4 px-4 py-4",
                    expandDeliverySummary && "@min-[560px]/sourcing:px-6",
                )}
            >
                <div className="rounded-lg border border-border/70 bg-card p-4 shadow-2xs">
                    {showParticipation || canRemove ? (
                        <div className="mb-3 flex items-center justify-between gap-2 border-b border-border/60 pb-2.5">
                            {showParticipation ? (
                                <form.AppField
                                    name={`lines[${index}].selected`}
                                >
                                    {(field) => (
                                        <label
                                            htmlFor={`${id}-select`}
                                            className="flex min-h-7 cursor-pointer items-center gap-2 text-xs font-medium text-muted-foreground hover:text-foreground"
                                        >
                                            <Checkbox
                                                id={`${id}-select`}
                                                checked={
                                                    field.state.value === true
                                                }
                                                onCheckedChange={(checked) =>
                                                    field.handleChange(
                                                        checked === true,
                                                    )
                                                }
                                                aria-label={`本次供给分配 ${product.itemName}`}
                                                data-testid={`purchase-sourcing-selected-${line.rowKey}`}
                                            />
                                            {participationLabel}
                                        </label>
                                    )}
                                </form.AppField>
                            ) : null}
                            {canRemove ? (
                                <Button
                                    id={`${id}-split-remove`}
                                    type="button"
                                    size="icon-sm"
                                    variant="ghost"
                                    className="ml-auto"
                                    aria-label={`删除 ${product.itemName} 的拆分行`}
                                    onClick={() => onRemoveSplit(line.rowKey)}
                                >
                                    <Trash2Icon aria-hidden="true" />
                                </Button>
                            ) : null}
                        </div>
                    ) : null}

                    {/* 主表单区：选择供应商 + 交期 / 数量 / 目标仓 */}
                    <div
                        className={cn(
                            "grid min-w-0 grid-cols-1 items-start gap-4",
                            addressDriven &&
                                "@min-[620px]/sourcing:grid-cols-[minmax(0,1fr)_13.5rem]",
                        )}
                    >
                        <div className="min-w-0 space-y-1.5">
                            <div className="flex min-h-6 flex-wrap items-center justify-between gap-2">
                                <div className="flex flex-wrap items-center gap-2">
                                    <p className="text-xs font-medium text-foreground/75">
                                        供货供应商 / 履约方案
                                        <RequiredMark />
                                    </p>
                                    {option ? (
                                        <Badge
                                            variant={
                                                isStock ? "info" : "neutral"
                                            }
                                            className="h-5 px-1.5 text-tiny"
                                        >
                                            {sourcingRouteLabel(option)}
                                        </Badge>
                                    ) : null}
                                    <SourcingRecommendationHint
                                        id={`${id}-recommendation`}
                                        form={form}
                                        order={order}
                                        product={product}
                                        index={index}
                                    />
                                </div>
                                <div className="flex flex-wrap items-center gap-1.5">
                                    {condensed && product.options.length > 1 ? (
                                        <Button
                                            id={`${id}-compare-toggle`}
                                            type="button"
                                            size="xs"
                                            variant={
                                                compareOpen
                                                    ? "secondary"
                                                    : "outline"
                                            }
                                            aria-expanded={compareOpen}
                                            onClick={() =>
                                                setCompareOpen((prev) => !prev)
                                            }
                                        >
                                            <ScaleIcon aria-hidden="true" />
                                            {compareOpen
                                                ? "收起比价表"
                                                : `比价选源（${product.options.length}）`}
                                        </Button>
                                    ) : null}
                                    {onAddSplit && canAddSplit ? (
                                        <Button
                                            id={`sourcing-product-${toAutomationIdSegment(product.salesOrderLineId)}-split`}
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            disabled={Boolean(
                                                addSplitDisabledReason,
                                            )}
                                            title={
                                                addSplitDisabledReason ??
                                                `共 ${product.options.length} 个可用供给方案，支持拆单给多家供应商`
                                            }
                                            onClick={onAddSplit}
                                        >
                                            <PlusIcon aria-hidden="true" />
                                            拆单给多家
                                        </Button>
                                    ) : null}
                                </div>
                            </div>
                            <SourcingOptionField
                                form={form}
                                order={order}
                                index={index}
                                rowKey={line.rowKey}
                                product={product}
                                showDeliverySummary={false}
                            />
                        </div>

                        {addressDriven ? (
                            <div className="min-w-0 space-y-1.5">
                                <p className="flex min-h-6 items-center text-xs font-medium text-foreground/75">
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
                    </div>

                    {!addressDriven ? (
                        <div
                            className={cn(
                                "mt-4 grid min-w-0 grid-cols-1 gap-3.5 @min-[420px]/sourcing:grid-cols-2",
                                needsWarehouse &&
                                    (compact
                                        ? "@min-[560px]/sourcing:grid-cols-[minmax(0,0.8fr)_minmax(0,1.2fr)_minmax(0,1fr)]"
                                        : "@min-[680px]/sourcing:grid-cols-3"),
                            )}
                        >
                            <div className="min-w-0 space-y-1.5">
                                <p className="text-xs font-medium text-foreground/75">
                                    本次分配数量（{product.unit}）
                                    <RequiredMark />
                                </p>
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
                                            inputClassName="num"
                                            testId={`purchase-sourcing-quantity-${line.rowKey}`}
                                        />
                                    )}
                                </form.AppField>
                            </div>
                            {needsWarehouse ? (
                                <div className="min-w-0 space-y-1.5">
                                    <p className="text-xs font-medium text-foreground/75">
                                        采购入库目标仓
                                        <RequiredMark />
                                    </p>
                                    <SourcingTargetWarehouseField
                                        form={form}
                                        index={index}
                                        rowKey={line.rowKey}
                                        order={order}
                                        salesOrderLineId={line.salesOrderLineId}
                                    />
                                    <WarehouseDeliveryNote
                                        option={option}
                                        warehouseId={
                                            line.targetWarehouseId ?? ""
                                        }
                                    />
                                </div>
                            ) : null}
                            {!isStock ? (
                                <div className="min-w-0 space-y-1.5">
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
                                        showDeadlineHint={!condensed}
                                    />
                                </div>
                            ) : null}
                        </div>
                    ) : null}

                    {/* 商业条款与承接汇总条：一次看清采购价/毛利、税率账期、配送覆盖与承接量 */}
                    {option ? (
                        <div className="mt-4 space-y-2.5 border-t border-border/60 pt-3.5">
                            {isStock ? (
                                <div className="flex flex-wrap items-center gap-x-4 gap-y-1.5 rounded-md bg-surface-sunken px-3 py-2 text-xs text-muted-foreground">
                                    <span className="inline-flex items-center gap-1.5 font-medium text-foreground">
                                        <PackageCheckIcon
                                            aria-hidden="true"
                                            className="size-3.5 text-info"
                                        />
                                        现有库存直配（
                                        {option.warehouseName ||
                                            option.supplierName}
                                        ）
                                    </span>
                                    <span>成本按库存核算</span>
                                    <span>
                                        可用库存{" "}
                                        <QuantityValue
                                            className="font-medium text-foreground"
                                            value={
                                                option.sourceAvailableQuantity ??
                                                option.maxCreateQuantity
                                            }
                                            unit={product.unit}
                                        />
                                    </span>
                                    <span>客户地址由仓库发货时安排</span>
                                </div>
                            ) : (
                                <Collapsible>
                                    <div className="grid grid-cols-1 gap-2.5 rounded-md bg-surface-sunken px-3.5 py-2.5 @min-[560px]/sourcing:grid-cols-2 @min-[820px]/sourcing:grid-cols-4">
                                        <div className="min-w-0 space-y-0.5">
                                            <p className="text-xs text-muted-foreground">
                                                含税采购价
                                            </p>
                                            <div className="flex flex-wrap items-baseline gap-1.5">
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
                                                            "num text-xs font-medium",
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
                                            </div>
                                        </div>

                                        <div className="min-w-0 space-y-0.5">
                                            <p className="text-xs text-muted-foreground">
                                                进项税率 / 付款条件
                                            </p>
                                            <p className="truncate text-body-compact font-medium text-foreground">
                                                {taxRatePercent ? (
                                                    <RateValue
                                                        value={taxRatePercent}
                                                        precision={2}
                                                    />
                                                ) : null}
                                                {option.paymentTermLabel
                                                    ? ` · ${option.paymentTermLabel}`
                                                    : ""}
                                            </p>
                                        </div>

                                        <div className="min-w-0 space-y-0.5">
                                            <p className="text-xs text-muted-foreground">
                                                配送覆盖
                                            </p>
                                            <p
                                                className={cn(
                                                    "truncate text-body-compact font-medium",
                                                    deliveryHasException
                                                        ? "text-warning-soft-foreground"
                                                        : "text-foreground",
                                                )}
                                                title={
                                                    deliveryText ||
                                                    "按仓库或订单履约"
                                                }
                                            >
                                                {deliveryText ||
                                                    (needsWarehouse
                                                        ? "供应商发货至目标仓"
                                                        : "按订单约定履约")}
                                            </p>
                                        </div>

                                        <div className="min-w-0 space-y-0.5">
                                            <div className="flex items-center justify-between gap-2">
                                                <p className="text-xs text-muted-foreground">
                                                    {addressDriven
                                                        ? "承接量（按地址汇总）"
                                                        : "方案来源"}
                                                </p>
                                                <CollapsibleTrigger
                                                    id={`${id}-facts-toggle`}
                                                    render={
                                                        <Button
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className="group h-5 px-1 text-xs text-muted-foreground hover:text-foreground"
                                                        />
                                                    }
                                                >
                                                    详情
                                                    <ChevronDownIcon className="size-3 transition-transform duration-150 group-data-[panel-open]:rotate-180" />
                                                </CollapsibleTrigger>
                                            </div>
                                            {addressDriven ? (
                                                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
                                                    <span className="text-body-compact font-semibold text-foreground">
                                                        <QuantityValue
                                                            value={
                                                                line.quantity ||
                                                                "0"
                                                            }
                                                            unit={product.unit}
                                                        />
                                                    </span>
                                                    <span className="num text-xs text-muted-foreground">
                                                        {assignedAddressCount}/
                                                        {totalAddressCount} 处
                                                    </span>
                                                    {onGoToAddresses ? (
                                                        <button
                                                            id={`${id}-goto-addresses`}
                                                            type="button"
                                                            onClick={
                                                                onGoToAddresses
                                                            }
                                                            className="inline-flex cursor-pointer items-center gap-0.5 text-xs font-medium text-primary hover:underline focus-visible:ring-2 focus-visible:ring-ring focus-visible:outline-none"
                                                        >
                                                            调地址
                                                            <ArrowRightIcon
                                                                aria-hidden="true"
                                                                className="size-3"
                                                            />
                                                        </button>
                                                    ) : null}
                                                </div>
                                            ) : (
                                                <p className="truncate text-body-compact font-medium text-foreground">
                                                    {option.inquirySource
                                                        ? "询价专项报价"
                                                        : "通用供给"}
                                                </p>
                                            )}
                                        </div>
                                    </div>

                                    <CollapsibleContent>
                                        <div className="mt-2 space-y-1 rounded-md border border-border/50 bg-card px-3.5 py-2 text-xs text-muted-foreground">
                                            <p>
                                                来源类型{" "}
                                                <span className="text-foreground">
                                                    {option.inquirySource
                                                        ? "询价专项报价"
                                                        : "通用供给"}
                                                </span>
                                                {option.delivery?.templateName
                                                    ? ` · 配送模板 ${option.delivery.templateName}`
                                                    : ""}
                                                {option.maxCreateQuantity
                                                    ? ` · 本方案上限 ${option.maxCreateQuantity} ${product.unit}`
                                                    : ""}
                                            </p>
                                            {option.inquirySource ? (
                                                <p>
                                                    沿用询价第{" "}
                                                    {
                                                        option.inquirySource
                                                            .feedback_round
                                                    }{" "}
                                                    轮 · 内部报价第{" "}
                                                    {
                                                        option.inquirySource
                                                            .quote_revision_no
                                                    }{" "}
                                                    版
                                                </p>
                                            ) : null}
                                            {option.recommendationReason ? (
                                                <p>
                                                    {
                                                        option.recommendationReason
                                                    }
                                                </p>
                                            ) : null}
                                        </div>
                                    </CollapsibleContent>
                                </Collapsible>
                            )}

                            {deliveryHasException ||
                            option.delivery?.bulkMinimumOrderQuantity ? (
                                <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-warning-border bg-warning-soft/75 px-3 py-2 text-xs text-warning-soft-foreground">
                                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
                                        <TriangleAlertIcon
                                            aria-hidden="true"
                                            className="size-3.5 shrink-0"
                                        />
                                        {deliveryHasException ? (
                                            <span>{deliveryText}</span>
                                        ) : null}
                                        {option.delivery
                                            ?.bulkMinimumOrderQuantity ? (
                                            <span>
                                                集采起订量{" "}
                                                <QuantityValue
                                                    value={
                                                        option.delivery
                                                            .bulkMinimumOrderQuantity
                                                    }
                                                    unit={product.unit}
                                                    tone="warning"
                                                />
                                            </span>
                                        ) : null}
                                    </div>
                                    {deliveryHasException &&
                                    addressDriven &&
                                    onGoToAddresses ? (
                                        <Button
                                            id={`${id}-warning-goto-addresses`}
                                            type="button"
                                            variant="outline"
                                            size="xs"
                                            className="bg-card text-foreground"
                                            onClick={onGoToAddresses}
                                        >
                                            <MapPinIcon aria-hidden="true" />
                                            前往处理异常地址
                                        </Button>
                                    ) : null}
                                </div>
                            ) : null}
                        </div>
                    ) : (
                        <p className="mt-3 rounded-md border border-dashed border-border/70 px-3 py-2 text-xs text-muted-foreground">
                            请先在上方或下方比价表中选择一个供货方案。
                        </p>
                    )}
                </div>

                {/* 全宽比价选源矩阵：与当前方案卡清晰分层 */}
                {condensed && compareOpen && product.options.length > 0 ? (
                    <div className="space-y-1.5">
                        <div className="flex items-center justify-between gap-2">
                            <h4 className="text-xs font-semibold text-foreground">
                                候选方案比价（点击任意可用行直接替换当前来源）
                            </h4>
                            <span className="num text-xs text-muted-foreground">
                                共 {product.options.length} 个合格方案
                            </span>
                        </div>
                        <SourcingOptionCompareMatrix
                            idPrefix={id}
                            order={order}
                            product={product}
                            selectedBasisId={line.basisId}
                            usedBasisIds={usedBasisIds}
                            recommendedBasisId={recommendedBasisId}
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

            {!hideAddressEditor && !addressDriven ? (
                <div
                    className={cn(
                        "px-4 pb-4 empty:hidden",
                        expandDeliverySummary && "@min-[560px]/sourcing:px-6",
                    )}
                >
                    <SourcingDeliveryEditor
                        form={form}
                        product={product}
                        line={line}
                        index={index}
                        showNotApplicable={false}
                        condensed={condensed}
                        expandSummary={expandDeliverySummary}
                    />
                </div>
            ) : null}
        </div>
    )
}

/** 集采入仓目标仓所在地区的配送判定。 */
function WarehouseDeliveryNote({
    option,
    warehouseId,
}: {
    option: ReturnType<typeof findSourcingOption>
    warehouseId: string
}) {
    const judged = warehouseDeliveryTerm(option, warehouseId)
    if (!option || !judged) return null
    return (
        <p
            className={cn(
                "text-xs",
                judged.term === "NOT_DELIVERABLE"
                    ? "text-destructive"
                    : judged.term === "FREIGHT_EXTRA"
                      ? "text-warning-soft-foreground"
                      : "text-muted-foreground",
            )}
        >
            {judged.regionName}：{deliveryTermLabel(option, judged.term)}
        </p>
    )
}
