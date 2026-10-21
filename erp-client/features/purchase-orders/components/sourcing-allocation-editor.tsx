"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { Trash2Icon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { findSourcingOption } from "../lib/purchase-order-create-model"
import { sourcingQuantityStep } from "../lib/sourcing-quantity"
import type {
    SourcingLineInput,
    SourcingProductLine,
} from "../lib/purchase-order-create-model"
import {
    SourcingExpectedDeliveryField,
    SourcingOptionField,
    SourcingTargetWarehouseField,
} from "./create-sourcing/sourcing-fields"
import type { SourcingEditorProps } from "./create-sourcing/types"
import { SourcingDeliveryEditor } from "./sourcing-delivery-plan"
import { SourcingRecommendationHint } from "./sourcing-recommendation-hint"
import {
    deliverySummary,
    deliveryTermLabel,
    sourcingRouteLabel,
    warehouseDeliveryTerm,
} from "../lib/sourcing/delivery-terms"

/** 调整区沿用正式表单字段；按来源、数量与交期、交付地址的顺序填写。 */
export function SourcingAllocationEditor({
    form,
    order,
    product,
    line,
    index,
    canRemove,
    onRemoveSplit,
    compact = false,
    showParticipation = true,
    participationLabel = "本次分配",
}: Pick<SourcingEditorProps, "form" | "order" | "onRemoveSplit"> & {
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
    canRemove: boolean
    compact?: boolean
    showParticipation?: boolean
    participationLabel?: string
}) {
    const option = findSourcingOption(product, line.basisId)
    const isStock = option?.sourceType === "EXISTING_STOCK"
    const needsWarehouse =
        option?.sourceType === "PURCHASE" &&
        option.fulfillmentResponsibility === "WAREHOUSE"
    const id = `procurement-orders-create-row-${toAutomationIdSegment(line.rowKey)}`
    return (
        <div
            key={line.rowKey}
            className={cn(
                "space-y-3 px-4 py-4",
                !line.selected && "bg-muted/20",
            )}
        >
            {showParticipation || canRemove ? (
                <div className="flex items-center justify-between gap-2">
                    {showParticipation ? (
                        <form.AppField name={`lines[${index}].selected`}>
                            {(field) => (
                                <label
                                    htmlFor={`${id}-select`}
                                    className="flex min-h-8 cursor-pointer items-center gap-2 text-xs font-medium"
                                >
                                    <Checkbox
                                        id={`${id}-select`}
                                        checked={field.state.value === true}
                                        onCheckedChange={(checked) =>
                                            field.handleChange(checked === true)
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
            <div className="min-w-0 space-y-1.5">
                <div className="flex min-h-6 flex-wrap items-center justify-between gap-2">
                    <p className="text-xs text-muted-foreground">
                        供给来源
                        <RequiredMark />
                    </p>
                    <SourcingRecommendationHint
                        id={`${id}-recommendation`}
                        form={form}
                        order={order}
                        product={product}
                        index={index}
                    />
                </div>
                <SourcingOptionField
                    form={form}
                    order={order}
                    index={index}
                    rowKey={line.rowKey}
                    product={product}
                    showDeliverySummary={false}
                />
                <SourcingOptionFacts option={option} product={product} />
                {option?.sourceType === "PURCHASE" &&
                    !option.inquirySource &&
                    (option.inquiryReselectionRequired ||
                        product.options.some(
                            (candidate) => candidate.inquirySource,
                        )) && (
                        <form.AppField
                            name={`lines[${index}].reselectionReason`}
                            children={(field) => (
                                <field.TextareaField
                                    id={`${id}-reselection-reason`}
                                    label="改选原因"
                                    required
                                    placeholder="说明本次改用其他供应商或通用供给的原因"
                                />
                            )}
                        />
                    )}
            </div>
            <div
                className={cn(
                    "grid min-w-0 grid-cols-1 gap-3 @min-[420px]/sourcing:grid-cols-2",
                    needsWarehouse &&
                        (compact
                            ? "@min-[560px]/sourcing:grid-cols-[minmax(0,0.75fr)_minmax(0,1fr)_minmax(0,1.1fr)]"
                            : "@min-[680px]/sourcing:grid-cols-3"),
                )}
            >
                <div className="min-w-0 space-y-1.5">
                    <p className="text-xs text-muted-foreground">
                        本次分配数量（{product.unit}）<RequiredMark />
                    </p>
                    <form.AppField name={`lines[${index}].quantity`}>
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
                        <p className="text-xs text-muted-foreground">
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
                            warehouseId={line.targetWarehouseId ?? ""}
                        />
                    </div>
                ) : null}
                {!isStock ? (
                    <div className="min-w-0 space-y-1.5">
                        <p className="text-xs text-muted-foreground">
                            预计交付日
                            <RequiredMark />
                        </p>
                        <SourcingExpectedDeliveryField
                            form={form}
                            index={index}
                            rowKey={line.rowKey}
                            order={order}
                            salesOrderLineId={line.salesOrderLineId}
                        />
                    </div>
                ) : null}
            </div>
            <SourcingDeliveryEditor
                form={form}
                product={product}
                line={line}
                index={index}
                showNotApplicable={false}
            />
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

/** 方案属性紧随来源选择；先给价格与履约标签，异常配送和起订量单独补一行。 */
function SourcingOptionFacts({
    option,
    product,
}: {
    option: ReturnType<typeof findSourcingOption>
    product: SourcingProductLine
}) {
    if (!option) {
        return (
            <p className="rounded-lg border border-dashed border-border/70 px-3 py-2 text-xs text-muted-foreground">
                选择供给来源后显示价格、付款与配送条件。
            </p>
        )
    }
    if (option.sourceType === "EXISTING_STOCK") {
        return (
            <div className="rounded-lg border border-border/70 bg-muted/25 px-3 py-2.5">
                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                    <Badge variant="secondary">现有库存</Badge>
                    <span className="text-sm font-medium">
                        {option.warehouseName || option.supplierName}
                    </span>
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                    成本按库存核算
                </p>
            </div>
        )
    }
    const delivery = deliverySummary(option, product)
    const deliveryException = delivery.startsWith("全部地址") ? "" : delivery
    const template =
        option.fulfillmentResponsibility === "SUPPLIER_DIRECT" &&
        option.delivery
            ? (option.delivery.templateName ??
              (option.priceBasis === "BULK" ? "全国包邮" : "全国配送"))
            : option.delivery?.templateName
    return (
        <div className="rounded-lg border border-border/70 bg-muted/25 px-3 py-2.5">
            <div className="flex items-start justify-between gap-3">
                <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                    <Badge variant="outline">
                        {option.inquirySource ? "询价专项报价" : "通用供给"}
                    </Badge>
                    <Badge variant="secondary">
                        {sourcingRouteLabel(option)}
                    </Badge>
                    {option.paymentTermLabel ? (
                        <Badge variant="outline">
                            付款 {option.paymentTermLabel}
                        </Badge>
                    ) : null}
                    {template ? (
                        <Badge variant="outline">{template}</Badge>
                    ) : null}
                </div>
                <div className="shrink-0 text-right">
                    <p className="text-sm font-semibold">
                        <MoneyValue
                            display="exact"
                            value={option.unitCostGross}
                        />
                    </p>
                    <p className="mt-0.5 text-xs text-muted-foreground">
                        含税 · 税率{" "}
                        <RateValue
                            value={multiplyFixed(option.inputTaxRate, "100", {
                                leftMaxScale: 6,
                                rightMaxScale: 0,
                                outputScale: 2,
                            })}
                            precision={2}
                        />
                    </p>
                </div>
            </div>
            {option.inquirySource && (
                <p className="mt-2 text-xs text-muted-foreground">
                    沿用询价第 {option.inquirySource.feedback_round} 轮 ·
                    内部报价第 {option.inquirySource.quote_revision_no} 版 ·
                    失效日期 {option.inquirySource.valid_to} · 起订量{" "}
                    {option.inquirySource.minimum_order_quantity}
                </p>
            )}
            {option.recommendationReason && (
                <p className="mt-1 text-xs text-muted-foreground">
                    {option.recommendationReason}
                </p>
            )}
            {deliveryException || option.delivery?.bulkMinimumOrderQuantity ? (
                <div className="mt-2 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                    {deliveryException ? (
                        <span className="text-warning-soft-foreground">
                            {deliveryException}
                        </span>
                    ) : null}
                    {option.delivery?.bulkMinimumOrderQuantity ? (
                        <span>
                            集采起订量{" "}
                            <QuantityValue
                                value={option.delivery.bulkMinimumOrderQuantity}
                                unit={product.unit}
                            />
                        </span>
                    ) : null}
                </div>
            ) : null}
        </div>
    )
}
