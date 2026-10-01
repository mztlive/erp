"use client"

import { PlusIcon, Trash2Icon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { findSourcingOption } from "@/features/purchase-orders/lib/purchase-order-create-model"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    canSplitSourcingProduct,
    sourcingQuantityStep,
} from "../lib/sourcing-quantity"
import { multiplyFixed } from "@/lib/fixed-decimal"

import {
    SourcingExpectedDeliveryField,
    SourcingOptionField,
    SourcingTargetWarehouseField,
} from "./create-sourcing/sourcing-fields"
import type { SourcingEditorProps } from "./create-sourcing/types"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"
import type { SourcingSalesOrder } from "../lib/purchase-order-create-model"

export type PurchaseOrderCreateSourcingTableProps = SourcingEditorProps
export {
    SourcingExpectedDeliveryField,
    SourcingOptionField,
    SourcingTargetWarehouseField,
} from "./create-sourcing/sourcing-fields"

/**
 * 选源明细表：逐行选择履约方案、填写拆分数量并确认预计交付日。
 */
export function PurchaseOrderCreateSourcingTable({
    form,
    order,
    onAddSplit,
    onRemoveSplit,
}: PurchaseOrderCreateSourcingTableProps) {
    const formLines = form.state.values.lines
    return (
        <div className="overflow-hidden rounded-lg border border-border bg-card">
            <Table data-density="compact">
                <TableHeader>
                    <TableRow>
                        <TableHead className="w-12">本单</TableHead>
                        <TableHead className="min-w-56">销售项目</TableHead>
                        <TableHead data-align="end">销售数量</TableHead>
                        <TableHead data-align="end">已覆盖</TableHead>
                        <TableHead data-align="end">剩余数量</TableHead>
                        <TableHead className="min-w-64">
                            供给来源 / 履约责任
                        </TableHead>
                        <TableHead>采购入库目标仓</TableHead>
                        <TableHead data-align="end">本次分配数量</TableHead>
                        <TableHead data-align="end">含税成本</TableHead>
                        <TableHead data-align="end">进项税率</TableHead>
                        <TableHead data-align="end">预计交期</TableHead>
                        <TableHead className="w-24">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {formLines.map((input, index) => {
                        const product = order.lines.find(
                            (line) =>
                                line.salesOrderLineId ===
                                input.salesOrderLineId,
                        )
                        if (!product) return null
                        const siblingCount = formLines.filter(
                            (line) =>
                                line.salesOrderLineId ===
                                input.salesOrderLineId,
                        ).length
                        return (
                            <TableRow key={input.rowKey}>
                                <TableCell>
                                    <form.AppField
                                        name={`lines[${index}].selected`}
                                    >
                                        {(field) => (
                                            <label
                                                className="inline-flex min-h-10 cursor-pointer items-center justify-center"
                                                htmlFor={`procurement-orders-create-row-${toAutomationIdSegment(input.rowKey)}-select`}
                                            >
                                                <Checkbox
                                                    id={`procurement-orders-create-row-${toAutomationIdSegment(input.rowKey)}-select`}
                                                    checked={
                                                        field.state.value ===
                                                        true
                                                    }
                                                    onCheckedChange={(
                                                        checked,
                                                    ) =>
                                                        field.handleChange(
                                                            checked === true,
                                                        )
                                                    }
                                                    aria-label={`本次供给分配 ${product.itemName}`}
                                                    data-testid={`purchase-sourcing-selected-${input.rowKey}`}
                                                />
                                            </label>
                                        )}
                                    </form.AppField>
                                </TableCell>
                                <TableCell className="min-w-56 max-w-72 whitespace-normal">
                                    <div className="font-medium text-foreground">
                                        {product.itemName}
                                    </div>
                                    <div className="mt-0.5 text-xs text-muted-foreground">
                                        {[
                                            product.salesAllocationLabel,
                                            product.deliveryDeadline
                                                ? `承诺最晚 ${product.deliveryDeadline}`
                                                : undefined,
                                        ]
                                            .filter(Boolean)
                                            .join(" · ")}
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    <QuantityValue
                                        value={product.salesQuantity}
                                        unit={product.unit}
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <QuantityValue
                                        value={product.coveredQuantity}
                                        unit={product.unit}
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <QuantityValue
                                        value={product.remainingQuantity}
                                        unit={product.unit}
                                    />
                                </TableCell>
                                <TableCell className="min-w-64">
                                    <SourcingOptionField
                                        form={form}
                                        index={index}
                                        rowKey={input.rowKey}
                                        product={product}
                                    />
                                </TableCell>
                                <TableCell className="min-w-52">
                                    <SourcingTargetWarehouseField
                                        form={form}
                                        index={index}
                                        rowKey={input.rowKey}
                                        order={order}
                                        salesOrderLineId={
                                            input.salesOrderLineId
                                        }
                                    />
                                </TableCell>
                                <TableCell className="min-w-36">
                                    <form.AppField
                                        name={`lines[${index}].quantity`}
                                    >
                                        {(field) => (
                                            <field.TextField
                                                id={`procurement-orders-create-row-${toAutomationIdSegment(input.rowKey)}-quantity`}
                                                label="本次分配数量"
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
                                                testId={`purchase-sourcing-quantity-${input.rowKey}`}
                                            />
                                        )}
                                    </form.AppField>
                                </TableCell>
                                <SourcingOptionFacts
                                    form={form}
                                    index={index}
                                    order={order}
                                    salesOrderLineId={input.salesOrderLineId}
                                />
                                <TableCell className="min-w-36">
                                    <SourcingExpectedDeliveryField
                                        form={form}
                                        index={index}
                                        rowKey={input.rowKey}
                                        order={order}
                                        salesOrderLineId={
                                            input.salesOrderLineId
                                        }
                                    />
                                </TableCell>
                                <TableCell>
                                    <div className="flex items-center gap-1">
                                        <Button
                                            id={`procurement-orders-create-row-${toAutomationIdSegment(input.rowKey)}-split-add`}
                                            type="button"
                                            size="icon-sm"
                                            variant="ghost"
                                            aria-label={`拆分 ${product.itemName}`}
                                            title="拆分履约"
                                            disabled={
                                                !canSplitSourcingProduct(
                                                    product,
                                                    siblingCount,
                                                )
                                            }
                                            onClick={() =>
                                                onAddSplit(
                                                    input.salesOrderLineId,
                                                )
                                            }
                                        >
                                            <PlusIcon />
                                        </Button>
                                        <Button
                                            id={`procurement-orders-create-row-${toAutomationIdSegment(input.rowKey)}-split-remove`}
                                            type="button"
                                            size="icon-sm"
                                            variant="ghost"
                                            aria-label={`删除 ${product.itemName} 的拆分行`}
                                            title="删除拆分"
                                            disabled={siblingCount <= 1}
                                            onClick={() =>
                                                onRemoveSplit(input.rowKey)
                                            }
                                        >
                                            <Trash2Icon />
                                        </Button>
                                    </div>
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </Table>
        </div>
    )
}

/**
 * 按当前选用履约方案展示成本与税率。
 */
function SourcingOptionFacts({
    form,
    index,
    order,
    salesOrderLineId,
}: {
    form: PurchaseOrderCreateFormApi
    index: number
    order: SourcingSalesOrder
    salesOrderLineId: string
}) {
    return (
        <form.Subscribe
            selector={(state) => {
                const line = state.values.lines[index]
                return line?.basisId ?? ""
            }}
        >
            {(basisId) => {
                const product = order.lines.find(
                    (line) => line.salesOrderLineId === salesOrderLineId,
                )
                const option = findSourcingOption(product, basisId)
                return (
                    <>
                        <TableCell data-align="end">
                            {option?.sourceType === "EXISTING_STOCK" ? (
                                <span className="text-xs text-muted-foreground">
                                    按库存核算
                                </span>
                            ) : option ? (
                                <MoneyValue value={option.unitCostGross} />
                            ) : (
                                "—"
                            )}
                        </TableCell>
                        <TableCell data-align="end">
                            {option?.sourceType === "EXISTING_STOCK" ? (
                                "—"
                            ) : option ? (
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
                            ) : (
                                "—"
                            )}
                        </TableCell>
                    </>
                )
            }}
        </form.Subscribe>
    )
}
