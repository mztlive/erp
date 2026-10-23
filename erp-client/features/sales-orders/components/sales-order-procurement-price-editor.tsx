"use client"

import { MoneyValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SalesOrderCreateFormApi } from "../lib/sales-order-create-form-types"
import {
    supplyAmount,
    supplyReferencePrice,
} from "../lib/sales-procurement-price"

type Props = {
    form: SalesOrderCreateFormApi
    rowIndex: number
    inquiryLocked: boolean
}

export function SalesProcurementBasisEditor({
    form,
    rowIndex,
    inquiryLocked,
}: Props) {
    const line = form.getFieldValue("lineItems")[rowIndex]
    if (!line) return null
    if (inquiryLocked)
        return (
            <span className="flex min-h-control items-center text-sm">
                询价覆盖
            </span>
        )
    return (
        <div className="w-32">
            <form.AppField
                name={`lineItems[${rowIndex}].procurementPriceBasis`}
                listeners={{
                    onChange: ({ value }) => {
                        if (!value) return
                        const prices = form.getFieldValue(
                            `lineItems[${rowIndex}].referencePrices`,
                        )
                        form.setFieldValue(
                            `lineItems[${rowIndex}].procurementSupplyPrice`,
                            supplyReferencePrice(prices, value),
                        )
                    },
                }}
            >
                {(field) => (
                    <field.SelectField
                        id={`sales-orders-create-line-${toAutomationIdSegment(line.rowKey)}-supply-basis`}
                        label="供给取价方式"
                        hideLabel
                        allowClear={false}
                        placeholder="请选择"
                        options={[
                            { value: "BULK", label: "集采" },
                            { value: "DROPSHIP", label: "一件代发" },
                        ]}
                    />
                )}
            </form.AppField>
        </div>
    )
}

export function SalesProcurementPriceEditor({
    form,
    rowIndex,
    inquiryLocked,
}: Props) {
    const line = form.getFieldValue("lineItems")[rowIndex]
    if (!line) return null
    const amount = supplyAmount(line.procurementSupplyPrice, line.quantity)
    const missingReference =
        !inquiryLocked &&
        line.procurementPriceBasis &&
        !supplyReferencePrice(line.referencePrices, line.procurementPriceBasis)
    return (
        <div className="w-40 space-y-1">
            {inquiryLocked ? (
                <span className="flex min-h-control items-center justify-end text-sm">
                    <MoneyValue value={line.procurementSupplyPrice || null} />
                </span>
            ) : (
                <form.AppField
                    name={`lineItems[${rowIndex}].procurementSupplyPrice`}
                >
                    {(field) => (
                        <field.TextField
                            id={`sales-orders-create-line-${toAutomationIdSegment(line.rowKey)}-supply-price`}
                            label="采购供给单价"
                            required
                            hideLabel
                            type="number"
                            step="any"
                            placeholder="请填写单价"
                            className="gap-0"
                            inputClassName="num text-right"
                        />
                    )}
                </form.AppField>
            )}
            <div className="text-right text-xs text-muted-foreground">
                供给金额{" "}
                {amount === null ? "待完善" : <MoneyValue value={amount} />}
            </div>
            {missingReference ? (
                <p className="text-xs text-warning-soft-foreground">
                    未配置
                    {line.procurementPriceBasis === "BULK"
                        ? "集采价"
                        : "一件代发价"}
                    ，请填写供给单价或切换取价方式。
                </p>
            ) : null}
        </div>
    )
}
