"use client"

import { toFieldErrors } from "@/components/form"
import { ChangeOrderSection } from "@/components/business/change-order-section"
import { FieldError } from "@/components/ui/field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compactFixed } from "@/lib/fixed-decimal"
import { afterDueDates, removedDeliveryRows } from "../lib/delivery-adjustment"
import {
    purchaseChangeLineOption,
    purchaseChangeLineRemoved,
    type PurchaseChangeDeliveryOptions,
    type PurchaseChangeLineValues,
} from "../lib/purchase-change-draft"
import type { PurchaseChangeDraftFormApi } from "../lib/purchase-change-draft-form-types"
import { PurchaseDeliveryAllocationEditor } from "./purchase-delivery-allocation-editor"

/** 交付安排：按商品填写预计交付日；供应商直发或服务履约按销售收件资料分配数量。 */
export function PurchaseChangeDeliverySection({
    form,
    id,
    options,
    disabled,
}: {
    form: PurchaseChangeDraftFormApi
    id: string
    options: PurchaseChangeDeliveryOptions
    disabled: boolean
}) {
    return (
        <form.Subscribe selector={(state) => state.values.lines}>
            {(lines) => {
                const rows = lines.flatMap((line, index) =>
                    line.source.line_type === "ITEM_SERVICE" &&
                    !purchaseChangeLineRemoved(line)
                        ? [{ line, index }]
                        : [],
                )
                if (!rows.length) return null
                return (
                    <ChangeOrderSection
                        id={`${id}-delivery`}
                        title="交付安排"
                        description="预计交付日不得晚于销售当前履约期限；收件地址只能取自销售当前收件资料，合计等于采购数量。"
                    >
                        <div className="min-w-0 divide-y divide-grid rounded-lg border border-grid">
                            {rows.map(({ line, index }) => (
                                <DeliveryGroup
                                    key={line.lineKey}
                                    form={form}
                                    id={id}
                                    line={line}
                                    index={index}
                                    options={options}
                                    disabled={disabled}
                                />
                            ))}
                        </div>
                    </ChangeOrderSection>
                )
            }}
        </form.Subscribe>
    )
}

function DeliveryGroup({
    form,
    id,
    line,
    index,
    options,
    disabled,
}: {
    form: PurchaseChangeDraftFormApi
    id: string
    line: PurchaseChangeLineValues
    index: number
    options: PurchaseChangeDeliveryOptions
    disabled: boolean
}) {
    const source = line.source
    const option = purchaseChangeLineOption(source, options)
    const prefix = `${id}-line-${toAutomationIdSegment(line.lineKey)}`
    const unit = source.base_unit_code ?? ""
    return (
        <div className="min-w-0 space-y-3 p-4">
            <p className="break-words text-sm font-medium">
                {source.product_name ?? "采购商品/服务"}
                <span className="num ml-2 font-normal text-muted-foreground">
                    {compactFixed(line.quantity || "0")} {unit}
                </span>
            </p>
            {option ? (
                <div className="max-w-xs">
                    <form.AppField
                        name={`lines[${index}].expectedDeliveryDate`}
                    >
                        {(field) => (
                            <field.DateField
                                id={`${prefix}-expected-date`}
                                label="预计交付日"
                                description={`不晚于销售履约期限 ${option.salesDueDate}`}
                                clearable={false}
                                disabledDates={afterDueDates(
                                    option.salesDueDate,
                                )}
                                disabled={disabled}
                            />
                        )}
                    </form.AppField>
                </div>
            ) : (
                <p className="text-xs text-muted-foreground">
                    预计交付日：{source.expected_delivery_date ?? "—"}
                </p>
            )}
            {option?.requiresAddresses ? (
                <form.AppField name={`lines[${index}].deliveryRows`}>
                    {(field) => (
                        <>
                            <PurchaseDeliveryAllocationEditor
                                id={`${prefix}-delivery`}
                                option={option}
                                rows={field.state.value}
                                removed={removedDeliveryRows(
                                    option,
                                    source.delivery_allocations ?? [],
                                )}
                                quantity={line.quantity}
                                unit={unit || undefined}
                                disabled={disabled}
                                onChange={field.handleChange}
                            />
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </>
                    )}
                </form.AppField>
            ) : null}
        </div>
    )
}
