"use client"

import { Trash2Icon } from "lucide-react"
import { MoneyValue, RateValue } from "@/components/business"
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
    type PurchaseOrderCreateSourcingTableProps,
} from "./purchase-order-create-sourcing-table"

/** 调整区沿用正式表单字段；当前供给摘要与编辑状态共用同一份值。 */
export function SourcingAllocationEditor({
    form,
    order,
    product,
    line,
    index,
    canRemove,
    onRemoveSplit,
}: Pick<
    PurchaseOrderCreateSourcingTableProps,
    "form" | "order" | "onRemoveSplit"
> & {
    product: SourcingProductLine
    line: SourcingLineInput
    index: number
    canRemove: boolean
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
            <div className="flex items-center justify-between gap-2">
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
                            本次分配
                        </label>
                    )}
                </form.AppField>
                {canRemove ? (
                    <Button
                        id={`${id}-split-remove`}
                        type="button"
                        size="icon-sm"
                        variant="ghost"
                        aria-label={`删除 ${product.itemName} 的拆分行`}
                        onClick={() => onRemoveSplit(line.rowKey)}
                    >
                        <Trash2Icon aria-hidden="true" />
                    </Button>
                ) : null}
            </div>
            <div className="min-w-0 space-y-1.5">
                <p className="text-xs text-muted-foreground">
                    供给来源 / 履约方式
                </p>
                <SourcingOptionField
                    form={form}
                    index={index}
                    rowKey={line.rowKey}
                    product={product}
                />
            </div>
            <div
                className={cn(
                    "grid min-w-0 grid-cols-1 gap-3 @min-[420px]/sourcing:grid-cols-2",
                    needsWarehouse && "@min-[680px]/sourcing:grid-cols-3",
                )}
            >
                <div className="min-w-0 space-y-1.5">
                    <p className="text-xs text-muted-foreground">
                        本次分配数量（{product.unit}）
                    </p>
                    <form.AppField name={`lines[${index}].quantity`}>
                        {(field) => (
                            <field.TextField
                                id={`${id}-quantity`}
                                label={`本次分配数量，${product.itemName}`}
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
                        </p>
                        <SourcingTargetWarehouseField
                            form={form}
                            index={index}
                            rowKey={line.rowKey}
                            order={order}
                            salesOrderLineId={line.salesOrderLineId}
                        />
                    </div>
                ) : null}
                {!isStock ? (
                    <div className="min-w-0 space-y-1.5">
                        <p className="text-xs text-muted-foreground">
                            预计交付日
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
            <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                {isStock ? (
                    <>
                        <span>
                            现有库存 ·{" "}
                            {option.warehouseName || option.supplierName}
                        </span>
                        <span>成本按库存核算</span>
                    </>
                ) : option ? (
                    <>
                        <span>
                            含税单价 <MoneyValue value={option.unitCostGross} />
                        </span>
                        <span>
                            进项税率{" "}
                            <RateValue
                                value={multiplyFixed(
                                    option.inputTaxRate,
                                    "100",
                                    {
                                        leftMaxScale: 6,
                                        rightMaxScale: 0,
                                        outputScale: 2,
                                    },
                                )}
                                precision={2}
                            />
                        </span>
                        <span>{PURCHASE_TYPE_LABEL[option.purchaseType]}</span>
                        <span>付款条件 {option.paymentTermLabel || "—"}</span>
                    </>
                ) : (
                    <span>请选择供给方案</span>
                )}
            </div>
        </div>
    )
}
