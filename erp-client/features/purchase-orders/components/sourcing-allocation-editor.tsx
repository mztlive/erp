"use client"

import { RequiredMark } from "@/components/ui/required-mark"

import { Trash2Icon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
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
import { PURCHASE_TYPE_LABEL } from "../types"
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
                <p className="text-xs text-muted-foreground">
                    供给来源
                    <RequiredMark />
                </p>
                <SourcingOptionField
                    form={form}
                    order={order}
                    index={index}
                    rowKey={line.rowKey}
                    product={product}
                />
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                    <SourcingRecommendationHint
                        id={`${id}-recommendation`}
                        form={form}
                        order={order}
                        product={product}
                        index={index}
                    />
                    <SourcingOptionFacts option={option} product={product} />
                </div>
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

/** 方案属性紧随来源选择；每项独立成段，便于逐项核对。 */
function SourcingOptionFacts({
    option,
    product,
}: {
    option: ReturnType<typeof findSourcingOption>
    product: SourcingProductLine
}) {
    if (!option) return <span>请选择供给来源</span>
    if (option.sourceType === "EXISTING_STOCK")
        return (
            <>
                <span>
                    现有库存 · {option.warehouseName || option.supplierName}
                </span>
                <span>成本按库存核算</span>
            </>
        )
    return (
        <>
            <span>
                含税单价 <MoneyValue value={option.unitCostGross} />
            </span>
            <span>
                进项税率{" "}
                <RateValue
                    value={multiplyFixed(option.inputTaxRate, "100", {
                        leftMaxScale: 6,
                        rightMaxScale: 0,
                        outputScale: 2,
                    })}
                    precision={2}
                />
            </span>
            <span>{PURCHASE_TYPE_LABEL[option.purchaseType]}</span>
            <span>付款条件 {option.paymentTermLabel || "—"}</span>
            <span>{sourcingRouteLabel(option)}</span>
            {option.delivery ? (
                <span>
                    配送模板{" "}
                    {option.delivery.templateName ?? "未设置（全国配送、包邮）"}
                </span>
            ) : null}
            {deliverySummary(option, product) ? (
                <span>{deliverySummary(option, product)}</span>
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
        </>
    )
}
