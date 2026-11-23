"use client"

import { MoneyValue } from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
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
            <span className="flex min-h-control items-center text-muted-foreground">
                询价覆盖
            </span>
        )
    return (
        <div className="w-28">
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
                        className="gap-0"
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
    const showAmount =
        amount === null || compareDecimal(line.quantity, "1", 6) !== 0
    const missingReference =
        !inquiryLocked &&
        line.procurementPriceBasis &&
        !supplyReferencePrice(line.referencePrices, line.procurementPriceBasis)
    const missingLabel =
        line.procurementPriceBasis === "BULK" ? "集采价" : "一件代发价"
    return (
        <div className="w-32 space-y-1">
            {inquiryLocked ? (
                <span className="flex min-h-control items-center justify-end">
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
                            type="text"
                            inputMode="decimal"
                            placeholder="请填写"
                            className="gap-0"
                            inputClassName="num text-right"
                        />
                    )}
                </form.AppField>
            )}
            {showAmount ? (
                <div className="num text-right text-xs text-muted-foreground">
                    供给总额{" "}
                    {amount === null ? "待完善" : <MoneyValue value={amount} />}
                </div>
            ) : null}
            {missingReference ? (
                <p
                    className="text-right text-xs text-warning-soft-foreground"
                    title={`未配置${missingLabel}，请填写供给单价或切换取价方式`}
                >
                    未配{missingLabel}，请手填
                </p>
            ) : null}
        </div>
    )
}
