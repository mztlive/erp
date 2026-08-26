"use client"

import { PlusIcon, Trash2Icon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { FieldError } from "@/components/ui/field"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
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
import { FULFILLMENT_RESPONSIBILITY_LABEL } from "@/features/purchase-orders/types"
import { multiplyFixed } from "@/lib/fixed-decimal"

export type PurchaseOrderCreateSourcingTableProps = {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    onAddSplit: (salesOrderLineId: string) => void
    onRemoveSplit: (rowKey: string) => void
}

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
                        <TableHead>销售项目</TableHead>
                        <TableHead data-align="end">销售数量</TableHead>
                        <TableHead data-align="end">已覆盖</TableHead>
                        <TableHead data-align="end">剩余数量</TableHead>
                        <TableHead>供给来源 / 履约责任</TableHead>
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
                                                htmlFor={`purchase-sourcing-selected-${input.rowKey}`}
                                            >
                                                <Checkbox
                                                    id={`purchase-sourcing-selected-${input.rowKey}`}
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
                                <TableCell className="max-w-[16rem] whitespace-normal">
                                    <div className="font-medium text-foreground">
                                        {product.itemName}
                                    </div>
                                    <div className="mt-0.5 text-xs text-muted-foreground">
                                        {[
                                            product.itemSku,
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
                                <TableCell className="min-w-48">
                                    <form.AppField
                                        name={`lines[${index}].basisId`}
                                    >
                                        {(field) => (
                                            <field.SelectField
                                                label={`履约方案，${product.itemName}`}
                                                hideLabel
                                                allowClear={
                                                    product.options.length > 1
                                                }
                                                placeholder="选择履约方案"
                                                options={product.options.map(
                                                    (option) => ({
                                                        value: option.basisId,
                                                        label:
                                                            option.sourceType ===
                                                            "EXISTING_STOCK"
                                                                ? `${option.supplierName} · 可用 ${option.sourceAvailableQuantity ?? option.maxCreateQuantity}`
                                                                : `${option.supplierName} · ${FULFILLMENT_RESPONSIBILITY_LABEL[option.fulfillmentResponsibility]}`,
                                                        keywords: `${option.sourceType} ${option.supplierId} ${option.warehouseName ?? ""} ${option.fulfillmentResponsibility}`,
                                                    }),
                                                )}
                                                onValueChange={(
                                                    value: string,
                                                ) => {
                                                    const option =
                                                        findSourcingOption(
                                                            product,
                                                            value,
                                                        )
                                                    if (!option) return
                                                    const lines =
                                                        form.state.values.lines
                                                    const current =
                                                        lines[index]
                                                            ?.quantity ?? ""
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
                                                    form.setFieldValue(
                                                        `lines[${index}].expectedDeliveryDate`,
                                                        option.expectedDeliveryDate,
                                                    )
                                                    if (
                                                        option.sourceType !==
                                                            "PURCHASE" ||
                                                        option.fulfillmentResponsibility !==
                                                            "WAREHOUSE"
                                                    ) {
                                                        form.setFieldValue(
                                                            `lines[${index}].targetWarehouseId`,
                                                            "",
                                                        )
                                                        form.setFieldValue(
                                                            `lines[${index}].targetWarehouseName`,
                                                            "",
                                                        )
                                                    }
                                                }}
                                            />
                                        )}
                                    </form.AppField>
                                </TableCell>
                                <TableCell className="min-w-52">
                                    <SourcingTargetWarehouseField
                                        form={form}
                                        index={index}
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
                                                label="本次分配数量"
                                                hideLabel
                                                type="number"
                                                inputMode="decimal"
                                                min="0"
                                                step="any"
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
                                        order={order}
                                        salesOrderLineId={
                                            input.salesOrderLineId
                                        }
                                    />
                                </TableCell>
                                <TableCell>
                                    <div className="flex items-center gap-1">
                                        <Button
                                            type="button"
                                            size="icon-sm"
                                            variant="ghost"
                                            aria-label={`拆分 ${product.itemName}`}
                                            title="拆分履约"
                                            onClick={() =>
                                                onAddSplit(
                                                    input.salesOrderLineId,
                                                )
                                            }
                                        >
                                            <PlusIcon />
                                        </Button>
                                        <Button
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

/** 只有「采购 + 入仓」方案必须在建单前选定目标仓。 */
function SourcingTargetWarehouseField({
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
            selector={(state) => state.values.lines[index]?.basisId ?? ""}
        >
            {(basisId) => {
                const product = order.lines.find(
                    (line) => line.salesOrderLineId === salesOrderLineId,
                )
                const option = findSourcingOption(product, basisId)
                if (
                    option?.sourceType !== "PURCHASE" ||
                    option.fulfillmentResponsibility !== "WAREHOUSE"
                ) {
                    return (
                        <span className="text-xs text-muted-foreground">
                            不适用
                        </span>
                    )
                }
                return (
                    <form.AppField name={`lines[${index}].targetWarehouseId`}>
                        {(field) => {
                            const invalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <div className="space-y-1">
                                    <WarehouseSearchCombobox
                                        value={field.state.value || undefined}
                                        onValueChange={(warehouseId) =>
                                            field.handleChange(
                                                warehouseId ?? "",
                                            )
                                        }
                                        onItemChange={(warehouse) =>
                                            form.setFieldValue(
                                                `lines[${index}].targetWarehouseName`,
                                                warehouse
                                                    ? `${warehouse.warehouseCode} · ${warehouse.warehouseName}`
                                                    : "",
                                            )
                                        }
                                        purpose="purchase-receipt"
                                        placeholder="选择目标仓"
                                        emptyLabel="没有可用仓库"
                                        aria-label={`采购入库目标仓，${product?.itemName ?? "采购明细"}`}
                                        aria-invalid={invalid || undefined}
                                    />
                                    {invalid ? (
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    ) : null}
                                </div>
                            )
                        }}
                    </form.AppField>
                )
            }}
        </form.Subscribe>
    )
}

/** 采购来源可确认预计交期；现有库存只建立预留，不伪造采购交期。 */
function SourcingExpectedDeliveryField({
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
            selector={(state) => state.values.lines[index]?.basisId ?? ""}
        >
            {(basisId) => {
                const product = order.lines.find(
                    (line) => line.salesOrderLineId === salesOrderLineId,
                )
                const option = findSourcingOption(product, basisId)
                if (option?.sourceType === "EXISTING_STOCK") {
                    return (
                        <span className="text-xs text-muted-foreground">
                            不适用
                        </span>
                    )
                }
                return (
                    <form.AppField
                        name={`lines[${index}].expectedDeliveryDate`}
                    >
                        {(field) => (
                            <field.DateField label="预计交付日" hideLabel />
                        )}
                    </form.AppField>
                )
            }}
        </form.Subscribe>
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
