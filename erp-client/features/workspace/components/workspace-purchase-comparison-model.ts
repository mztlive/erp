import type {
    ApprovalMaterials,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import { formatIsoDate, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import {
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"

export type PurchaseSource = NonNullable<
    ApprovalMaterials["display"]["source_sales"]
>[number]

/** 只有单位可比时才用销售价格提示审批风险。 */
export function isPurchaseCostAbove(
    line: ApprovalPurchaseLine,
    price: string | null | undefined,
) {
    const cost = line.unit_cost_gross
    if (cost == null || cost === "" || price == null || price === "")
        return false
    const purchaseUnit = line.unit?.trim()
    const salesUnit = line.source?.unit?.trim()
    if (purchaseUnit && salesUnit && purchaseUnit !== salesUnit) return false
    try {
        return compareDecimal(cost, price, 6) > 0
    } catch {
        return false
    }
}

export function purchaseSpecification(value: string | null) {
    return value?.trim() === "无规格" ? "" : (value?.trim() ?? "")
}

export function purchaseSalesDate(line: ApprovalPurchaseLine) {
    if (!line.source || !Number.isFinite(line.source.fulfillment_due_at))
        return null
    const iso = unixSecondsToIsoOrEmpty(line.source.fulfillment_due_at)
    return formatIsoDate(iso, { timeZone: "Asia/Shanghai" })
}

function calendarDay(value: string | null) {
    if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return null
    const date = new Date(`${value}T00:00:00Z`)
    return !Number.isNaN(date.getTime()) &&
        date.toISOString().slice(0, 10) === value
        ? date.getTime() / 86_400_000
        : null
}

function calculatePurchaseMarginRate(
    salesUnitPrice: string,
    purchaseUnitCost: string,
): { percent: string; negative: boolean } | null {
    try {
        if (compareDecimal(salesUnitPrice, "0", 6) <= 0) return null
        const diff = subtractFixed(salesUnitPrice, purchaseUnitCost, {
            maxScale: 6,
            outputScale: 6,
        })
        const negative = compareDecimal(diff, "0", 6) < 0
        const absDiff = negative ? diff.slice(1) : diff
        const rate = divideFixed(
            multiplyFixed(absDiff, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 6,
            }),
            salesUnitPrice,
            {
                numeratorMaxScale: 6,
                denominatorMaxScale: 6,
                outputScale: 1,
            },
        )
        return {
            percent: negative ? `-${rate}` : rate,
            negative,
        }
    } catch {
        return null
    }
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
    let marginRate: { percent: string; negative: boolean } | null = null
    let salesSupplyAdjustment: "increased" | "decreased" | "unchanged" | null =
        null
    const source = line.source
    if (!fee) {
        if (!source) warnings.push("来源销售缺失")
        else {
            const sourcePriceCheck = salesLinePriceComparison(
                source.sales_prices ?? source,
            )
            salesSupplyAdjustment = sourcePriceCheck.adjustment
            warnings.push(...sourcePriceCheck.warnings)
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
                warnings.push("销售预估采购价未填写")
            try {
                if (sameUnit && line.unit_cost_gross != null) {
                    marginRate = calculatePurchaseMarginRate(
                        source.unit_price_gross,
                        line.unit_cost_gross,
                    )
                }
                if (
                    sameUnit &&
                    supplyPrice != null &&
                    supplyPrice !== "" &&
                    line.unit_cost_gross != null
                ) {
                    if (
                        compareDecimal(line.unit_cost_gross, supplyPrice, 6) > 0
                    ) {
                        warnings.push("高于销售预估采购价")
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
        marginRate,
        salesSupplyAdjustment,
    }
}

export type PurchaseComparison = ReturnType<typeof purchaseComparison>
