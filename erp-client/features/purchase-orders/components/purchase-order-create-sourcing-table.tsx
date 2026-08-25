"use client"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type { PurchaseOrderCreateFormApi } from "@/features/purchase-orders/lib/purchase-order-create-form-types"
import {
    findSourcingOption,
    type SourcingSalesOrder,
} from "@/features/purchase-orders/lib/purchase-order-create-model"
import { multiplyFixed } from "@/lib/fixed-decimal"

export type PurchaseOrderCreateSourcingTableProps = {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
}

/**
 * 选源明细表：逐行或批量选择供应商，并填写本次采购数量。
 */
export function PurchaseOrderCreateSourcingTable({
    form,
    order,
}: PurchaseOrderCreateSourcingTableProps) {
    return (
        <div className="overflow-hidden rounded-lg border border-border bg-card">
            <Table data-density="compact">
                <TableHeader>
                    <TableRow>
                        <TableHead className="w-12">本单</TableHead>
                        <TableHead>销售项目</TableHead>
                        <TableHead data-align="end">销售数量</TableHead>
                        <TableHead data-align="end">已覆盖</TableHead>
                        <TableHead data-align="end">剩余数量</TableHead>
                        <TableHead>供应商</TableHead>
                        <TableHead data-align="end">本次采购数量</TableHead>
                        <TableHead data-align="end">含税成本</TableHead>
                        <TableHead data-align="end">进项税率</TableHead>
                        <TableHead data-align="end">预计交期</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {order.lines.map((product, index) => (
                        <TableRow key={product.salesOrderLineId}>
                            <TableCell>
                                <form.AppField
                                    name={`lines[${index}].selected`}
                                >
                                    {(field) => (
                                        <label
                                            className="inline-flex min-h-10 cursor-pointer items-center justify-center"
                                            htmlFor={`purchase-sourcing-selected-${product.salesOrderLineId}`}
                                        >
                                            <Checkbox
                                                id={`purchase-sourcing-selected-${product.salesOrderLineId}`}
                                                checked={
                                                    field.state.value === true
                                                }
                                                onCheckedChange={(checked) =>
                                                    field.handleChange(
                                                        checked === true,
                                                    )
                                                }
                                                aria-label={`本单采购 ${product.itemName}`}
                                                data-testid={`purchase-sourcing-selected-${product.salesOrderLineId}`}
                                            />
                                        </label>
                                    )}
                                </form.AppField>
                            </TableCell>
                            <TableCell className="max-w-[16rem] whitespace-normal">
                                <div className="font-medium text-foreground">
                                    {product.itemName}
                                </div>
                                <div className="mt-0.5 text-xs text-muted-foreground">
                                    {[
                                        product.itemSku,
                                        product.salesAllocationLabel,
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
                            <TableCell className="min-w-48">
                                <form.AppField
                                    name={`lines[${index}].supplierId`}
                                >
                                    {(field) => (
                                        <field.SelectField
                                            label={`供应商，${product.itemName}`}
                                            hideLabel
                                            allowClear={
                                                product.options.length > 1
                                            }
                                            placeholder="选择供应商"
                                            options={product.options.map(
                                                (option) => ({
                                                    value: option.supplierId,
                                                    label: option.supplierName,
                                                    keywords: option.supplierId,
                                                }),
                                            )}
                                            onValueChange={(value: string) => {
                                                const option =
                                                    findSourcingOption(
                                                        product,
                                                        value,
                                                    )
                                                if (!option) return
                                                const lines =
                                                    form.state.values.lines
                                                const current =
                                                    lines[index]?.quantity ?? ""
                                                if (
                                                    !current ||
                                                    current ===
                                                        product.remainingQuantity
                                                ) {
                                                    form.setFieldValue(
                                                        `lines[${index}].quantity`,
                                                        option.maxCreateQuantity,
                                                    )
                                                }
                                            }}
                                        />
                                    )}
                                </form.AppField>
                            </TableCell>
                            <TableCell className="min-w-36">
                                <form.AppField
                                    name={`lines[${index}].quantity`}
                                >
                                    {(field) => (
                                        <field.TextField
                                            label="本次采购数量"
                                            hideLabel
                                            type="number"
                                            inputMode="decimal"
                                            min="0"
                                            step="any"
                                            inputClassName="num text-right"
                                            testId={`purchase-sourcing-quantity-${product.salesOrderLineId}`}
                                        />
                                    )}
                                </form.AppField>
                            </TableCell>
                            <SourcingOptionFacts
                                form={form}
                                index={index}
                                order={order}
                            />
                        </TableRow>
                    ))}
                </TableBody>
            </Table>
        </div>
    )
}

/**
 * 按当前选用供应商展示成本、税率和交期。
 */
function SourcingOptionFacts({
    form,
    index,
    order,
}: {
    form: PurchaseOrderCreateFormApi
    index: number
    order: SourcingSalesOrder
}) {
    return (
        <form.Subscribe
            selector={(state) => {
                const line = state.values.lines[index]
                return line?.supplierId ?? ""
            }}
        >
            {(supplierId) => {
                const product = order.lines[index]
                const option = findSourcingOption(product, supplierId)
                return (
                    <>
                        <TableCell data-align="end">
                            {option ? (
                                <MoneyValue value={option.unitCostGross} />
                            ) : (
                                "—"
                            )}
                        </TableCell>
                        <TableCell data-align="end">
                            {option ? (
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
                        <TableCell data-align="end" className="num">
                            {option?.expectedDeliveryDate || "—"}
                        </TableCell>
                    </>
                )
            }}
        </form.Subscribe>
    )
}
