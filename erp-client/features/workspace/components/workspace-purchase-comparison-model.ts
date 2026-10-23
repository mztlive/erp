import type {
    ApprovalMaterials,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import {
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"

export type PurchaseSource = NonNullable<
    ApprovalMaterials["display"]["source_sales"]
>[number]

export const purchaseAttentionClass =
    "border-warning-border bg-warning-soft text-warning-soft-foreground"
export const purchaseSelectedClass =
    "border-info bg-info text-info-foreground hover:bg-info/90 hover:text-info-foreground"

export function purchaseSpecification(value: string | null) {
    return value?.trim() === "无规格" ? "" : (value?.trim() ?? "")
}

export function purchaseSalesDate(line: ApprovalPurchaseLine) {
    if (!line.source || !Number.isFinite(line.source.fulfillment_due_at))
        return null
    const date = new Date(line.source.fulfillment_due_at * 1000)
    return Number.isNaN(date.getTime())
        ? null
        : date.toLocaleDateString("sv-SE", {
              timeZone: "Asia/Shanghai",
              year: "numeric",
              month: "2-digit",
              day: "2-digit",
          })
}

function calendarDay(value: string | null) {
    if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return null
    const date = new Date(`${value}T00:00:00Z`)
    return !Number.isNaN(date.getTime()) &&
        date.toISOString().slice(0, 10) === value
        ? date.getTime() / 86_400_000
        : null
}

/** 只比较本次冻结材料，不推算其他采购覆盖量或整单利润。 */
export function purchaseComparison(line: ApprovalPurchaseLine) {
    const fee =
        !line.source && line.quantity == null && line.gross_amount != null
    const warnings: string[] = []
    const salesDate = purchaseSalesDate(line)
    let delayDays = 0
    let priceDifference: string | null = null
    let pricePercent: string | null = null
    let priceDifferenceTotal: string | null = null
    let quantityExcess: string | null = null
    let partial = false
    const source = line.source
    if (!fee) {
        if (!source) warnings.push("来源销售缺失")
        else {
            if (
                line.title.trim() !== source.title.trim() ||
                purchaseSpecification(line.specification) !==
                    purchaseSpecification(source.specification)
            )
                warnings.push("商品 / 规格不一致")
            const sameUnit =
                Boolean(line.unit?.trim()) &&
                line.unit?.trim() === source.unit.trim()
            if (!sameUnit) warnings.push("单位需核对")
            const supplyPrice = source.procurement_supply_price_gross
            if (supplyPrice == null || supplyPrice === "")
                warnings.push("供给价未填写")
            try {
                if (
                    sameUnit &&
                    supplyPrice != null &&
                    supplyPrice !== "" &&
                    line.unit_cost_gross != null
                ) {
                    if (
                        compareDecimal(line.unit_cost_gross, supplyPrice, 6) > 0
                    ) {
                        warnings.push("高于供给价")
                        priceDifference = subtractFixed(
                            line.unit_cost_gross,
                            supplyPrice,
                            { maxScale: 6, outputScale: 6 },
                        )
                        if (line.quantity != null) {
                            priceDifferenceTotal = multiplyFixed(
                                priceDifference,
                                line.quantity,
                                {
                                    leftMaxScale: 6,
                                    rightMaxScale: 6,
                                    outputScale: 2,
                                },
                            )
                        }
                        if (compareDecimal(supplyPrice, "0", 6) > 0) {
                            pricePercent = divideFixed(
                                multiplyFixed(priceDifference, "100", {
                                    leftMaxScale: 6,
                                    rightMaxScale: 0,
                                    outputScale: 6,
                                }),
                                supplyPrice,
                                {
                                    numeratorMaxScale: 6,
                                    denominatorMaxScale: 6,
                                    outputScale: 1,
                                },
                            )
                        }
                    }
                }
                if (sameUnit && line.quantity != null) {
                    const quantityOrder = compareDecimal(
                        line.quantity,
                        source.quantity,
                        6,
                    )
                    partial = quantityOrder < 0
                    if (quantityOrder > 0) {
                        warnings.push("本次数量超过销售需求")
                        quantityExcess = subtractFixed(
                            line.quantity,
                            source.quantity,
                            {
                                maxScale: 6,
                                outputScale: 6,
                            },
                        )
                    }
                }
            } catch {
                warnings.push("数值格式需核对")
            }
            const salesDay = calendarDay(salesDate)
            const purchaseDay = calendarDay(line.expected_delivery_date)
            if (salesDay == null || purchaseDay == null)
                warnings.push("交期资料不完整")
            else if (purchaseDay > salesDay) {
                delayDays = purchaseDay - salesDay
                warnings.push(`交期晚 ${delayDays} 天`)
            }
        }
        if (line.unit_cost_gross == null) warnings.push("采购单价缺失")
        if (line.quantity == null) warnings.push("采购数量缺失")
    }
    return {
        fee,
        warnings,
        salesDate,
        delayDays,
        priceDifference,
        pricePercent,
        priceDifferenceTotal,
        quantityExcess,
        partial,
    }
}

export type PurchaseComparison = ReturnType<typeof purchaseComparison>
