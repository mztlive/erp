import type {
    ApprovalMaterials,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import { formatIsoDate, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import type { WorkspaceWorkItem } from "../types"
import type { ResubmissionFieldDiff } from "./workspace-approval-resubmission-banner"
import type {
    PurchaseLineSourceInsight,
    PurchaseSourceComparisonModel,
} from "./workspace-purchase-source-comparison-model"

export type PurchaseSource = NonNullable<
    ApprovalMaterials["display"]["source_sales"]
>[number]

export type ExternalMarginSummary = Readonly<{
    /** 带符号毛利率（如 "16.4" 或 "-8.2"）。 */
    percent: string
    /** 无符号绝对毛利率（用于「亏 8.2%」，避免「亏 -8.2%」）。 */
    absPercent: string
    /** 带符号毛利额（两位小数）。 */
    amount: string
    /** 无符号绝对毛利额（两位小数）。 */
    absAmount: string
    negative: boolean
}>

export type PurchaseMarginAmounts = Readonly<{
    supply: string
    cost: string
    margin: string
}>

export type PurchaseMarginTotals = PurchaseMarginAmounts &
    Readonly<{ lineCount: number }>

export function sumPurchaseMargins(
    lines: readonly PurchaseMarginAmounts[],
): PurchaseMarginTotals | null {
    if (lines.length === 0) return null

    const amountScale = { maxScale: 2, outputScale: 2 }
    const supply = sumFixed(
        lines.map((line) => line.supply),
        amountScale,
    )
    const cost = sumFixed(
        lines.map((line) => line.cost),
        amountScale,
    )
    return {
        supply,
        cost,
        margin: subtractFixed(supply, cost, amountScale),
        lineCount: lines.length,
    }
}

export const PURCHASE_SUPPLY_PRICE_RISK = "实际采购单价高于采购供给价"

export type PurchaseLineRiskSeverity = "high" | "medium" | "info" | "none"

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

export function purchaseDeliveryComparison(line: ApprovalPurchaseLine) {
    const salesDate = purchaseSalesDate(line)
    const salesDay = calendarDay(salesDate)
    const purchaseDay = calendarDay(line.expected_delivery_date)
    const daysFromDeadline =
        salesDay == null || purchaseDay == null ? null : purchaseDay - salesDay
    return { salesDate, daysFromDeadline }
}

export function isPurchaseFeeLine(line: ApprovalPurchaseLine) {
    return !line.source && line.quantity == null && line.gross_amount != null
}

function calculateExternalMargin(
    salesAmount: string,
    purchaseCost: string,
): ExternalMarginSummary | null {
    try {
        if (compareDecimal(salesAmount, "0", 6) <= 0) return null
        const diff = subtractFixed(salesAmount, purchaseCost, {
            maxScale: 6,
            outputScale: 6,
        })
        const amount = subtractFixed(salesAmount, purchaseCost, {
            maxScale: 6,
            outputScale: 2,
        })
        const negative = compareDecimal(diff, "0", 6) < 0
        const absDiff = negative ? diff.slice(1) : diff
        const absAmount = amount.startsWith("-") ? amount.slice(1) : amount
        const absPercent = divideFixed(
            multiplyFixed(absDiff, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 6,
            }),
            salesAmount,
            {
                numeratorMaxScale: 6,
                denominatorMaxScale: 6,
                outputScale: 1,
            },
        )
        return {
            percent: negative ? `-${absPercent}` : absPercent,
            absPercent,
            amount,
            absAmount,
            negative,
        }
    } catch {
        return null
    }
}

/** 只比较本次冻结材料，不推算其他采购覆盖量。 */
export function purchaseComparison(line: ApprovalPurchaseLine) {
    const fee = isPurchaseFeeLine(line)
    const highWarnings: string[] = []
    const mediumWarnings: string[] = []
    const infoWarnings: string[] = []
    const { salesDate, daysFromDeadline } = purchaseDeliveryComparison(line)
    let delayDays = 0
    let supplyUnitShortfall: string | null = null
    let supplyUnitShortfallPercent: string | null = null
    let supplyGrossAmount: string | null = null
    let purchaseMargin: PurchaseMarginAmounts | null = null
    let salesLineGross: string | null = null
    let externalGrossMargin: string | null = null
    let quantityExcess: string | null = null
    let partial = false
    let externalMargin: ExternalMarginSummary | null = null
    let mismatchedProduct = false
    let unitMismatch = false
    let missingSource = false
    let missingSupply = false
    let incompleteDeliveryDate = false

    const source = line.source
    const salesPriceWarnings = source
        ? salesLinePriceComparison(source.sales_prices ?? source).warnings
        : []
    if (!fee) {
        if (!source) {
            missingSource = true
            highWarnings.push("来源销售缺失")
        } else {
            mediumWarnings.push(...salesPriceWarnings)
            if (
                line.title.trim() !== source.title.trim() ||
                purchaseSpecification(line.specification) !==
                    purchaseSpecification(source.specification)
            ) {
                mismatchedProduct = true
                highWarnings.push("商品 / 规格不一致")
            }
            const sameUnit =
                Boolean(line.unit?.trim()) &&
                line.unit?.trim() === source.unit.trim()
            if (!sameUnit) {
                unitMismatch = true
                highWarnings.push("单位需核对")
            }
            const supplyPrice = source.procurement_supply_price_gross
            if (supplyPrice == null || supplyPrice === "") {
                missingSupply = true
                infoWarnings.push("采购供给价未填写")
            }
            try {
                if (sameUnit && line.quantity != null) {
                    salesLineGross = multiplyFixed(
                        source.unit_price_gross,
                        line.quantity,
                        { leftMaxScale: 6, rightMaxScale: 6, outputScale: 2 },
                    )
                }
                if (salesLineGross != null && line.gross_amount != null) {
                    externalMargin = calculateExternalMargin(
                        salesLineGross,
                        line.gross_amount,
                    )
                    externalGrossMargin =
                        externalMargin?.amount ??
                        subtractFixed(salesLineGross, line.gross_amount, {
                            maxScale: 2,
                            outputScale: 2,
                        })
                } else if (sameUnit && line.unit_cost_gross != null) {
                    externalMargin = calculateExternalMargin(
                        source.unit_price_gross,
                        line.unit_cost_gross,
                    )
                }
                if (sameUnit && supplyPrice != null && supplyPrice !== "") {
                    if (
                        line.unit_cost_gross != null &&
                        compareDecimal(line.unit_cost_gross, supplyPrice, 6) > 0
                    ) {
                        mediumWarnings.push(PURCHASE_SUPPLY_PRICE_RISK)
                        supplyUnitShortfall = subtractFixed(
                            line.unit_cost_gross,
                            supplyPrice,
                            { maxScale: 6, outputScale: 6 },
                        )
                        if (compareDecimal(supplyPrice, "0", 6) > 0) {
                            supplyUnitShortfallPercent = divideFixed(
                                multiplyFixed(supplyUnitShortfall, "100", {
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
                    if (line.quantity != null) {
                        supplyGrossAmount = multiplyFixed(
                            supplyPrice,
                            line.quantity,
                            {
                                leftMaxScale: 6,
                                rightMaxScale: 6,
                                outputScale: 2,
                            },
                        )
                        if (line.gross_amount != null) {
                            purchaseMargin = {
                                supply: supplyGrossAmount,
                                cost: line.gross_amount,
                                margin: subtractFixed(
                                    supplyGrossAmount,
                                    line.gross_amount,
                                    { maxScale: 2, outputScale: 2 },
                                ),
                            }
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
                        highWarnings.push("本次数量超过销售需求")
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
                highWarnings.push("数值格式需核对")
            }
            if (daysFromDeadline == null) {
                incompleteDeliveryDate = true
                infoWarnings.push("交期资料不完整")
            } else if (daysFromDeadline > 0) {
                delayDays = daysFromDeadline
                mediumWarnings.push(`交期晚 ${delayDays} 天`)
            }
        }
        if (line.unit_cost_gross == null) highWarnings.push("采购单价缺失")
        if (line.quantity == null) highWarnings.push("采购数量缺失")
    }

    const warnings = [...highWarnings, ...mediumWarnings, ...infoWarnings]
    const actionableWarnings = [...highWarnings, ...mediumWarnings]
    const severity: PurchaseLineRiskSeverity =
        highWarnings.length > 0
            ? "high"
            : mediumWarnings.length > 0
              ? "medium"
              : infoWarnings.length > 0
                ? "info"
                : "none"

    return {
        fee,
        warnings,
        actionableWarnings,
        highWarnings,
        mediumWarnings,
        infoWarnings,
        severity,
        salesDate,
        delayDays,
        supplyUnitShortfall,
        supplyUnitShortfallPercent,
        supplyGrossAmount,
        purchaseMargin,
        salesLineGross,
        externalGrossMargin,
        quantityExcess,
        partial,
        externalMargin,
        salesPriceWarnings,
        mismatchedProduct,
        unitMismatch,
        missingSource,
        missingSupply,
        incompleteDeliveryDate,
    }
}

export type PurchaseComparison = ReturnType<typeof purchaseComparison>

export type EvaluatedPurchaseRow = Readonly<{
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    externalLoss: boolean
    sourceInsight?: PurchaseLineSourceInsight
    taxInverted: boolean
    splitMergedExcess: boolean
    lowerInputTax: boolean
    effectiveSeverity: PurchaseLineRiskSeverity
    /** 用于「仅看风险项」筛选（仅命中高危或中危可操作异常，排除纯参考缺省）。 */
    hasActionableRisk: boolean
}>

export function evaluatePurchaseLines(
    lines: readonly ApprovalPurchaseLine[],
    lineInsights?: ReadonlyMap<string, PurchaseLineSourceInsight>,
): EvaluatedPurchaseRow[] {
    return lines.map((line) => {
        const comparison = purchaseComparison(line)
        const externalLoss = isPurchaseCostAbove(
            line,
            line.source?.unit_price_gross,
        )
        const sourceInsight = lineInsights?.get(line.id)
        const taxInverted = Boolean(sourceInsight?.taxInverted)
        const splitMergedExcess = Boolean(
            sourceInsight &&
            sourceInsight.splitPurchaseLineCount > 1 &&
            sourceInsight.mergedQuantityExcess != null,
        )
        const lowerInputTax = Boolean(
            sourceInsight?.lowerInputTaxGapPoints != null,
        )
        const effectiveSeverity: PurchaseLineRiskSeverity =
            externalLoss ||
            comparison.severity === "high" ||
            taxInverted ||
            splitMergedExcess
                ? "high"
                : comparison.severity === "medium" || lowerInputTax
                  ? "medium"
                  : comparison.severity
        const hasActionableRisk =
            externalLoss ||
            comparison.actionableWarnings.length > 0 ||
            taxInverted ||
            splitMergedExcess ||
            lowerInputTax
        return {
            line,
            comparison,
            externalLoss,
            sourceInsight,
            taxInverted,
            splitMergedExcess,
            lowerInputTax,
            effectiveSeverity,
            hasActionableRisk,
        }
    })
}

/** 决策提示和 ADR-1 高危核对共用同一采购风控规则。 */
export function purchaseApprovalRiskReasons(
    lines?: ApprovalPurchaseLine[] | null,
    sourceComparison?: PurchaseSourceComparisonModel,
): string[] {
    if (!lines?.length) return []
    const evaluated = evaluatePurchaseLines(
        lines,
        sourceComparison?.lineInsights,
    )
    let externalLossCount = 0
    let quantityExcessCount = 0
    let mismatchCount = 0
    let delayCount = 0
    let costAboveSupplyCount = 0

    for (const { comparison, externalLoss } of evaluated) {
        if (externalLoss) externalLossCount += 1
        if (comparison.quantityExcess) quantityExcessCount += 1
        if (comparison.mismatchedProduct) mismatchCount += 1
        if (comparison.delayDays > 0) delayCount += 1
        if (comparison.supplyUnitShortfall) costAboveSupplyCount += 1
    }

    const reasons: string[] = []
    if (externalLossCount > 0) {
        reasons.push(
            `存在 ${externalLossCount} 项实际采购单价高于对外含税售价（对外负毛利风险）`,
        )
    }
    const summary = sourceComparison?.summary
    const hasTaxInverted = Boolean(
        summary &&
        (summary.overallTaxInverted || summary.taxInvertedLineCount > 0),
    )
    if (summary?.overallTaxInverted && summary.overallNetMargin) {
        reasons.push(
            `整单对外税后倒挂：扣除进销项税差后不含税亏损 ¥${summary.overallNetMargin.absAmount}（-${summary.overallNetMargin.absPercent}%）`,
        )
    } else if (summary && summary.taxInvertedLineCount > 0) {
        reasons.push(
            `存在 ${summary.taxInvertedLineCount} 项商品含税盈利但扣除进销项税差后对外税后倒挂`,
        )
    }
    if (quantityExcessCount > 0) {
        reasons.push(`存在 ${quantityExcessCount} 项采购数量超过来源销售需求`)
    } else if (summary && summary.splitMergedExcessLineCount > 0) {
        reasons.push(
            `存在 ${summary.splitMergedExcessLineCount} 项同销售行拆笔采购合计超过来源销售需求`,
        )
    }
    if (mismatchCount > 0) {
        reasons.push(`存在 ${mismatchCount} 项采购商品或规格与来源销售不一致`)
    }
    if (summary && summary.paymentRiskChecks.length > 0) {
        if (summary.paymentRiskChecks.length === 1) {
            const c = summary.paymentRiskChecks[0]!.check
            reasons.push(`资金条款风险（${c.badge}）：${c.detail}`)
        } else {
            reasons.push(
                `有 ${summary.paymentRiskChecks.length} 张来源销售单存在采购垫资或账期错配风险`,
            )
        }
    }
    if (!hasTaxInverted && summary) {
        if (summary.lowerInputTaxLineCount > 0) {
            reasons.push(
                `存在 ${summary.lowerInputTaxLineCount} 项采购进项税率低于销售销项税率，存在税差侵蚀毛利`,
            )
        } else if (summary.specialInvoiceRiskOrderCount > 0) {
            reasons.push(
                "来源销售要求开具增值税专用发票，但采购存在 0% 或未填写进项税率明细",
            )
        }
    }
    if (delayCount > 0) {
        reasons.push(`存在 ${delayCount} 项预计交期晚于客户承诺交期`)
    }
    if (costAboveSupplyCount > 0) {
        reasons.push(
            `存在 ${costAboveSupplyCount} 项实际采购单价高于采购供给价（采购倒挂）`,
        )
    }
    if (summary && summary.fulfillmentRiskChecks.length > 0) {
        if (summary.fulfillmentRiskChecks.length === 1) {
            const c = summary.fulfillmentRiskChecks[0]!.check
            reasons.push(`履约与取价提示（${c.badge}）：${c.detail}`)
        } else {
            reasons.push(
                `有 ${summary.fulfillmentRiskChecks.length} 张来源销售单存在履约方式与供给取价不符或供给价已调整`,
            )
        }
    }
    return reasons
}

/** 结合当前采购单实际异常动态生成驳回快捷短语。 */
export function buildPurchaseDynamicRejectPhrases(
    lines?: ApprovalPurchaseLine[] | null,
    attachmentCount?: number,
    sourceComparison?: PurchaseSourceComparisonModel,
): string[] {
    if (!lines?.length) return []
    const evaluated = evaluatePurchaseLines(
        lines,
        sourceComparison?.lineInsights,
    )
    const phrases: string[] = []
    const negativeLines = evaluated.filter((row) => row.externalLoss)
    if (negativeLines.length === 1) {
        phrases.push(
            `第 ${negativeLines[0]!.line.line_no} 行对外负毛利，实际采购单价高于对外售价`,
        )
    } else if (negativeLines.length > 1) {
        phrases.push(`共 ${negativeLines.length} 项对外负毛利，请核对采购价格`)
    }
    const summary = sourceComparison?.summary
    const hasTaxInverted = Boolean(
        summary &&
        (summary.overallTaxInverted || summary.taxInvertedLineCount > 0),
    )
    if (hasTaxInverted) {
        phrases.push("扣除进销项税差后对外税后倒挂，请重新核价或确认进项税率")
    }
    if (evaluated.some((row) => row.comparison.quantityExcess)) {
        phrases.push("采购数量超过来源销售需求，请核对数量依据")
    } else if (summary && summary.splitMergedExcessLineCount > 0) {
        phrases.push("同销售行拆笔采购合计超过来源销售需求，请核对数量依据")
    }
    if (evaluated.some((row) => row.comparison.mismatchedProduct)) {
        phrases.push("采购商品或规格与来源销售不一致")
    }
    if (summary && summary.paymentRiskChecks.length > 0) {
        phrases.push("采购付款条件早于客户回款，存在垫资或账期错配风险")
    }
    if (
        !hasTaxInverted &&
        summary &&
        (summary.lowerInputTaxLineCount > 0 ||
            summary.specialInvoiceRiskOrderCount > 0)
    ) {
        phrases.push("采购进项税率偏低或专票抵扣不匹配，请核对税票条件")
    }
    if (evaluated.some((row) => row.comparison.delayDays > 0)) {
        phrases.push("预计交期晚于客户承诺交期，请协调供应商")
    }
    if (evaluated.some((row) => row.comparison.supplyUnitShortfall)) {
        phrases.push(PURCHASE_SUPPLY_PRICE_RISK)
    }
    if (
        summary?.fulfillmentRiskChecks.some(
            (entry) => entry.check.badge === "取价与履约不符",
        )
    ) {
        phrases.push("按集采价取价但改由供应商直发，请核对直发成本及运费")
    }
    if (attachmentCount === 0) {
        phrases.push("缺少报价单、合同或相关凭证附件")
    }
    return phrases
}

export type PurchaseApprovalMetrics = Readonly<{
    grossTotal: string | null
    netTotal: string | null
    taxTotal: string | null
    feeGrossTotal: string | null
    goodsLineCount: number
    feeLineCount: number
    correspondingSalesTotal: string | null
    externalGrossMargin: string | null
    externalMargin: ExternalMarginSummary | null
    salesCoveredCount: number
    partialSalesCoverage: boolean
    purchaseMargin: PurchaseMarginTotals | null
    totalQuantity: { value: string; unit?: string } | null
    externalLossCount: number
    costAboveSupplyCount: number
    quantityExcessCount: number
    delayCount: number
    mismatchCount: number
    taxInvertedLineCount: number
    splitMergedExcessCount: number
    lowerInputTaxCount: number
    highRiskLineCount: number
    actionableRiskLineCount: number
    missingSupplyCount: number
    paymentTermLabel: string | null
    isPrepayGate: boolean
}>

export function buildPurchaseApprovalMetrics(
    evaluated: readonly EvaluatedPurchaseRow[],
    extraSections: ApprovalMaterials["display"]["source"]["extra_sections"],
): PurchaseApprovalMetrics {
    const grossAmounts: string[] = []
    const netAmounts: string[] = []
    const taxAmounts: string[] = []
    const feeGrossAmounts: string[] = []
    const salesGrossAmounts: string[] = []
    const comparablePurchaseAmounts: string[] = []
    const purchaseMargins: PurchaseMarginAmounts[] = []
    const units = new Set<string>()
    const quantities: string[] = []

    let allHaveNet = true
    let allHaveTax = true
    let goodsLineCount = 0
    let feeLineCount = 0
    let salesCoveredCount = 0
    let quantityLineCount = 0

    let externalLossCount = 0
    let costAboveSupplyCount = 0
    let quantityExcessCount = 0
    let delayCount = 0
    let mismatchCount = 0
    let taxInvertedLineCount = 0
    let splitMergedExcessCount = 0
    let lowerInputTaxCount = 0
    let highRiskLineCount = 0
    let actionableRiskLineCount = 0
    let missingSupplyCount = 0

    for (const row of evaluated) {
        const { line, comparison, externalLoss } = row
        if (line.gross_amount != null) grossAmounts.push(line.gross_amount)
        if (line.net_amount != null) netAmounts.push(line.net_amount)
        else allHaveNet = false
        if (line.tax_amount != null) taxAmounts.push(line.tax_amount)
        else allHaveTax = false

        if (comparison.fee) {
            feeLineCount += 1
            if (line.gross_amount != null) {
                feeGrossAmounts.push(line.gross_amount)
            }
        } else {
            goodsLineCount += 1
            if (comparison.salesLineGross && line.gross_amount != null) {
                salesGrossAmounts.push(comparison.salesLineGross)
                comparablePurchaseAmounts.push(line.gross_amount)
                salesCoveredCount += 1
            }
            if (comparison.purchaseMargin) {
                purchaseMargins.push(comparison.purchaseMargin)
            }
            if (line.quantity != null) {
                quantityLineCount += 1
                quantities.push(line.quantity)
                units.add(line.unitName ?? line.unit ?? "")
            }
        }

        if (externalLoss) externalLossCount += 1
        if (comparison.supplyUnitShortfall) costAboveSupplyCount += 1
        if (comparison.quantityExcess) quantityExcessCount += 1
        if (comparison.delayDays > 0) delayCount += 1
        if (comparison.mismatchedProduct) mismatchCount += 1
        if (row.taxInverted) taxInvertedLineCount += 1
        if (row.splitMergedExcess) splitMergedExcessCount += 1
        if (row.lowerInputTax) lowerInputTaxCount += 1
        if (comparison.missingSupply) missingSupplyCount += 1
        if (row.effectiveSeverity === "high") highRiskLineCount += 1
        if (row.hasActionableRisk) actionableRiskLineCount += 1
    }

    const grossTotal = grossAmounts.length
        ? sumFixed(grossAmounts, { maxScale: 2, outputScale: 2 })
        : null
    const netTotal =
        allHaveNet && netAmounts.length
            ? sumFixed(netAmounts, { maxScale: 2, outputScale: 2 })
            : null
    const taxTotal =
        allHaveTax && taxAmounts.length
            ? sumFixed(taxAmounts, { maxScale: 2, outputScale: 2 })
            : null
    const feeGrossTotal = feeGrossAmounts.length
        ? sumFixed(feeGrossAmounts, { maxScale: 2, outputScale: 2 })
        : null

    const correspondingSalesTotal = salesGrossAmounts.length
        ? sumFixed(salesGrossAmounts, { maxScale: 2, outputScale: 2 })
        : null
    const totalComparableCost =
        comparablePurchaseAmounts.length > 0
            ? sumFixed([...comparablePurchaseAmounts, ...feeGrossAmounts], {
                  maxScale: 2,
                  outputScale: 2,
              })
            : null
    const externalMargin =
        correspondingSalesTotal && totalComparableCost
            ? calculateExternalMargin(
                  correspondingSalesTotal,
                  totalComparableCost,
              )
            : null

    const totalQuantity =
        units.size === 1 &&
        quantityLineCount > 0 &&
        quantityLineCount === goodsLineCount
            ? {
                  value: compactFixed(
                      sumFixed(quantities, { maxScale: 6, outputScale: 6 }),
                  ),
                  unit: [...units][0] || undefined,
              }
            : null

    const paymentTermLabel =
        extraSections
            .find((section) => section.label === "付款条件")
            ?.value.trim() || null
    const isPrepayGate = Boolean(paymentTermLabel?.includes("先款"))

    return {
        grossTotal,
        netTotal,
        taxTotal,
        feeGrossTotal,
        goodsLineCount,
        feeLineCount,
        correspondingSalesTotal,
        externalGrossMargin:
            externalMargin?.amount ??
            (correspondingSalesTotal != null && totalComparableCost != null
                ? subtractFixed(correspondingSalesTotal, totalComparableCost, {
                      maxScale: 2,
                      outputScale: 2,
                  })
                : null),
        externalMargin,
        salesCoveredCount,
        partialSalesCoverage:
            goodsLineCount > 0 &&
            salesCoveredCount > 0 &&
            salesCoveredCount < goodsLineCount,
        purchaseMargin: sumPurchaseMargins(purchaseMargins),
        totalQuantity,
        externalLossCount,
        costAboveSupplyCount,
        quantityExcessCount,
        delayCount,
        mismatchCount,
        taxInvertedLineCount,
        splitMergedExcessCount,
        lowerInputTaxCount,
        highRiskLineCount,
        actionableRiskLineCount,
        missingSupplyCount,
        paymentTermLabel,
        isPrepayGate,
    }
}

const COMPARISON_SUFFIX = "（前→后）"
const CHANGE_ARROW = " → "

const PURCHASE_EXCLUDED_STATIC_LABELS = new Set([
    "供应商",
    "来源销售单",
    "含税金额",
    "不含税金额",
    "税额",
    "付款条件",
    "提交号",
    "提交来源",
    "提交人",
    "对比基准",
    "重提差异",
    "重新提交说明",
])

export type ParsedPurchaseExtraSections = Readonly<{
    resubmission: Readonly<{
        roundNo?: number
        resubmissionReason?: string
        diffBaseline?: string
        diffSummary?: string
        diffs: readonly ResubmissionFieldDiff[]
        previousRejection?: Readonly<{
            reason: string
            actor?: string | null
        }>
    }> | null
    purchaseType?: string
    businessCategory?: string
    fulfillmentResponsibility?: string
    staticFacts: readonly Readonly<{
        label: string
        value: string
        numeric: boolean
    }>[]
}>

export function parsePurchaseExtraSections(
    extraSections: ApprovalMaterials["display"]["source"]["extra_sections"],
    item: WorkspaceWorkItem,
): ParsedPurchaseExtraSections {
    const diffs: ResubmissionFieldDiff[] = []
    const staticFacts: Array<{
        label: string
        value: string
        numeric: boolean
    }> = []
    let resubmissionReason: string | undefined
    let diffBaseline: string | undefined
    let diffSummary: string | undefined
    let origin: string | undefined
    let purchaseType: string | undefined
    let businessCategory: string | undefined
    let fulfillmentResponsibility: string | undefined

    for (const section of extraSections) {
        const value = section.value.trim()
        if (!value) continue
        if (section.label === "重新提交说明") {
            resubmissionReason = value
            continue
        }
        if (section.label === "对比基准") {
            diffBaseline = value
            continue
        }
        if (section.label === "重提差异") {
            diffSummary = value
            continue
        }
        if (section.label === "提交来源") {
            origin = value
            continue
        }
        if (section.label.endsWith(COMPARISON_SUFFIX)) {
            const arrowIdx = value.indexOf(CHANGE_ARROW)
            if (arrowIdx > 0) {
                diffs.push({
                    label: section.label.slice(0, -COMPARISON_SUFFIX.length),
                    before: value.slice(0, arrowIdx).trim(),
                    after: value.slice(arrowIdx + CHANGE_ARROW.length).trim(),
                    numeric: section.numeric,
                })
                continue
            }
        }
        if (PURCHASE_EXCLUDED_STATIC_LABELS.has(section.label)) continue
        const resolvedValue =
            section.object_id?.trim() === value ? "未记录名称" : value
        if (section.label === "采购类型") {
            purchaseType = resolvedValue
            continue
        }
        if (section.label === "经营类目") {
            businessCategory = resolvedValue
            continue
        }
        if (section.label === "履约责任") {
            fulfillmentResponsibility = resolvedValue
            continue
        }
        staticFacts.push({
            label: section.label,
            value: resolvedValue,
            numeric: section.numeric,
        })
    }

    const roundNo = item.approval?.currentRoundNo
    const previousRejection =
        roundNo != null &&
        roundNo > 1 &&
        item.approval?.status !== "REJECTED" &&
        item.approval?.lastRejectReason?.trim()
            ? {
                  reason: item.approval.lastRejectReason.trim(),
                  actor: item.approval.lastRejectorLabel,
              }
            : undefined

    const hasResubmissionContent = Boolean(
        resubmissionReason ||
        diffSummary ||
        diffs.length > 0 ||
        previousRejection ||
        origin === "再次提交",
    )

    return {
        resubmission: hasResubmissionContent
            ? {
                  roundNo,
                  resubmissionReason,
                  diffBaseline,
                  diffSummary,
                  diffs,
                  previousRejection,
              }
            : null,
        purchaseType,
        businessCategory,
        fulfillmentResponsibility,
        staticFacts,
    }
}
