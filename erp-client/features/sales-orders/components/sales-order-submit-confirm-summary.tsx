"use client"

import { multiplyFixed } from "@/lib/fixed-decimal"
import { calculateTotals } from "@/features/sales-orders/lib/sales-order-create-model"
import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-validation"
import type { SalesOrderNature } from "@/features/sales-orders/types"

export type SalesOrderSubmitLineSnapshot = {
    rowKey: string
    name: string
    sku: string
    quantity: string
    unit: string
    unitPriceGross: string
    amountGross: string
    dueDate: string
    faceValue: string
    giftRate: string
    cardForm: string
}

export type SalesOrderSubmitSnapshot = {
    customerName: string
    contractLabel: string
    settlementEntity: string
    ownerName: string
    nature: SalesOrderNature
    welfareScene: string
    paymentTerms: string
    fulfillmentDeadline: string
    targetMallId: string
    taxRatePercent: string
    remark: string
    lineCount: number
    amountGross: string
    amountNet: string
    amountTax: string
    lineItems: readonly SalesOrderSubmitLineSnapshot[]
}

function lineGross(quantity: string, unitPriceGross: string): string {
    try {
        return multiplyFixed(quantity || "0", unitPriceGross || "0", {
            leftMaxScale: 6,
            rightMaxScale: 4,
            outputScale: 2,
        })
    } catch {
        return "0.00"
    }
}

export function buildSalesOrderSubmitSnapshot(
    values: CreateSalesOrderFormValues,
): SalesOrderSubmitSnapshot {
    const totals = calculateTotals(values.lineItems, values.taxRatePercent)
    const lineItems = values.lineItems
        .filter((line) => line.sku.trim() || line.name.trim())
        .map((line) => ({
            rowKey: line.rowKey,
            name: line.name,
            sku: line.sku,
            quantity: line.quantity,
            unit: line.unit,
            unitPriceGross: line.unitPriceGross,
            amountGross: lineGross(line.quantity, line.unitPriceGross),
            dueDate: line.dueDate,
            faceValue: line.faceValue,
            giftRate: line.giftRate,
            cardForm: line.cardForm,
        }))
    return {
        customerName: values.customerName,
        contractLabel: values.contractRevisionLabel,
        settlementEntity: values.settlementEntity,
        ownerName: values.ownerName,
        nature: values.nature,
        welfareScene: values.welfareScene,
        paymentTerms: values.paymentTerms,
        fulfillmentDeadline: values.fulfillmentDeadline,
        targetMallId: values.targetMallId,
        taxRatePercent: values.taxRatePercent,
        remark: values.remark,
        lineCount: lineItems.length,
        amountGross: totals.gross,
        amountNet: totals.net,
        amountTax: totals.tax,
        lineItems,
    }
}
