"use client"

import { useContext } from "react"

import { SourcingOptionsRefreshContext } from "@/features/purchase-orders/components/create-sourcing/options-refresh-context"

import { toFieldErrors } from "@/components/form"
import { FieldError } from "@/components/ui/field"
import { WarehouseSearchCombobox } from "@/features/entity-selectors"
import type { PurchaseOrderCreateFormApi } from "../../lib/purchase-order-create-form-types"
import {
    findSourcingOption,
    type SourcingSalesOrder,
} from "../../lib/purchase-order-create-model"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    deliverySummary,
    sourcingRouteLabel,
} from "../../lib/sourcing/delivery-terms"
import { bulkMinimumOptionState } from "../../lib/sourcing/bulk-minimum"

/** 只有「采购 + 入仓」方案必须在建单前选定目标仓。 */
export function SourcingTargetWarehouseField({
    form,
    index,
    rowKey,
    order,
    salesOrderLineId,
}: {
    form: PurchaseOrderCreateFormApi
    index: number
    rowKey: string
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
                                        id={`procurement-orders-create-row-${toAutomationIdSegment(rowKey)}-warehouse`}
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
export function SourcingExpectedDeliveryField({
    form,
    index,
    rowKey,
    order,
    salesOrderLineId,
}: {
    form: PurchaseOrderCreateFormApi
    index: number
    rowKey: string
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
                const deadline = product?.deliveryDeadline
                const validDeadline =
                    deadline && /^\d{4}-\d{2}-\d{2}$/.test(deadline)
                return (
                    <form.AppField
                        name={`lines[${index}].expectedDeliveryDate`}
                    >
                        {(field) => (
                            <field.DateField
                                id={`procurement-orders-create-row-${toAutomationIdSegment(rowKey)}-delivery-date`}
                                label="预计交付日"
                                description={
                                    validDeadline
                                        ? `不晚于销售承诺期限 ${deadline}`
                                        : undefined
                                }
                                required
                                hideLabel
                                clearable={false}
                                disabledDates={
                                    validDeadline
                                        ? {
                                              after: new Date(
                                                  `${deadline}T23:59:59`,
                                              ),
                                          }
                                        : undefined
                                }
                            />
                        )}
                    </form.AppField>
                )
            }}
        </form.Subscribe>
    )
}

/** 供给方案选择器；表格与工作台卡片共用选源后的数量、仓库和交期联动。 */
export function SourcingOptionField({
    form,
    order,
    index,
    rowKey,
    product,
    hideLabel = true,
    showDeliverySummary = true,
}: {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    index: number
    rowKey: string
    product: SourcingSalesOrder["lines"][number]
    hideLabel?: boolean
    /** 紧凑编辑区由下方方案摘要展示配送条件，避免选中值过长。 */
    showDeliverySummary?: boolean
}) {
    const onRefresh = useContext(SourcingOptionsRefreshContext)
    return (
        <form.AppField name={`lines[${index}].basisId`}>
            {(field) => (
                <field.SelectField
                    onRefresh={onRefresh}
                    id={`procurement-orders-create-row-${toAutomationIdSegment(rowKey)}-sourcing-option`}
                    label={`履约方案，${product.itemName}`}
                    required
                    hideLabel={hideLabel}
                    allowClear={false}
                    placeholder="选择履约方案"
                    options={product.options.map((option) => {
                        const minimum = bulkMinimumOptionState(order, option)
                        return {
                            value: option.basisId,
                            disabled: minimum.disabled,
                            label:
                                option.sourceType === "EXISTING_STOCK"
                                    ? `${option.warehouseName || option.supplierName} · 可用 ${option.sourceAvailableQuantity ?? option.maxCreateQuantity}`
                                    : [
                                          option.supplierName,
                                          sourcingRouteLabel(option),
                                          minimum.label,
                                          showDeliverySummary
                                              ? deliverySummary(option, product)
                                              : "",
                                      ]
                                          .filter(Boolean)
                                          .join(" · "),
                            keywords: `${option.sourceType} ${option.supplierId} ${option.warehouseName ?? ""} ${option.fulfillmentResponsibility}`,
                        }
                    })}
                    onValueChange={(value: string) => {
                        const option = findSourcingOption(product, value)
                        if (!option) return
                        const lines = form.state.values.lines
                        const current = lines[index]?.quantity ?? ""
                        if (!current || current === product.remainingQuantity) {
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
                            option.sourceType !== "PURCHASE" ||
                            option.fulfillmentResponsibility !== "WAREHOUSE"
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
    )
}
