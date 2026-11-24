import type {
    ApprovalMaterials,
    ApprovalSalesSubmission,
} from "@/features/approval-workflow/api/materials"
import { displayReadableName } from "@/features/approval-workflow/display"
import { welfareScenarioLabel } from "@/lib/business-options"
import { displayBusinessText } from "@/lib/display-name"
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

export type SalesApprovalLine = ApprovalSalesSubmission["lines"][number]

export type MarginSummary = Readonly<{
    /** 带符号毛利率（例如 "18.5" 或 "-12.5"）。 */
    percent: string
    /** 无符号绝对毛利率（用于「亏 12.5%」，避免「亏 -12.5%」）。 */
    absPercent: string
    /** 带符号毛利额（单位元，两位小数）。 */
    amount: string
    /** 无符号绝对毛利额（单位元，两位小数）。 */
    absAmount: string
    negative: boolean
}>

export type VoucherDiscountSummary = Readonly<{
    label: string
    kind: "discount" | "par" | "premium"
}>

export type SalesLineRiskSeverity = "high" | "medium" | "info" | "none"

export type EvaluatedSalesLine = Readonly<{
    line: SalesApprovalLine
    unit: string | undefined
    name: string
    spec: string | undefined
    welfare: string | undefined
    isGift: boolean
    negativeMargin: boolean
    belowOriginalFloor: boolean
    priceAdjusted: boolean
    missingSupply: boolean
    priceCheck: ReturnType<typeof salesLinePriceComparison> | null
    margin: MarginSummary | null
    voucherDiscount: VoucherDiscountSummary | null
    severity: SalesLineRiskSeverity
    /** 用于「仅看风险项」筛选（覆盖负毛利、低于原底价及底价调整）。 */
    hasRisk: boolean
}>

export type SalesApprovalMetrics = Readonly<{
    isCard: boolean
    lineCount: number
    totalQuantity: { value: string; unit?: string } | null
    /** 商品与服务单据：按已填成本行汇总的预估成本、毛利额与毛利率（计入赠品成本）。 */
    estimatedCostTotal: string | null
    giftCostTotal: string | null
    estimatedMargin: MarginSummary | null
    costCoveredCount: number
    costExpectedCount: number
    partialCostCoverage: boolean
    /** 卡券单据：总面额、总张数与整单折率。 */
    totalFaceValue: string | null
    totalCards: string | null
    overallVoucherDiscount: VoucherDiscountSummary | null
    /** 风险计数 */
    negativeMarginCount: number
    belowFloorCount: number
    priceAdjustedCount: number
    missingSupplyCount: number
    giftCount: number
    highRiskLineCount: number
    riskLineCount: number
}>

export type SalesResubmissionContext = Readonly<{
    isResubmission: boolean
    roundNo?: number
    resubmissionReason?: string
    previousRejection?: Readonly<{
        reason: string
        actor?: string | null
    }>
}>

export function isNegativeMarginLine(
    line: SalesApprovalLine,
    isCard: boolean,
): boolean {
    if (isCard || line.pricing_mode === "GIFT") return false
    const unitPrice = line.unit_price_gross
    const supplyPrice = line.procurement_supply_price_gross
    if (
        unitPrice == null ||
        unitPrice === "" ||
        supplyPrice == null ||
        supplyPrice === ""
    ) {
        return false
    }
    try {
        return compareDecimal(unitPrice, supplyPrice, 6) < 0
    } catch {
        return false
    }
}

export function isBelowOriginalFloorLine(
    line: SalesApprovalLine,
    isCard: boolean,
): boolean {
    if (isCard || line.pricing_mode === "GIFT") return false
    const unitPrice = line.unit_price_gross
    const originalPrice = line.original_procurement_supply_price_gross
    if (
        unitPrice == null ||
        unitPrice === "" ||
        originalPrice == null ||
        originalPrice === ""
    ) {
        return false
    }
    try {
        return compareDecimal(unitPrice, originalPrice, 6) < 0
    } catch {
        return false
    }
}

export function calculateSalesMargin(
    sellingTotal: string | null | undefined,
    costTotal: string | null | undefined,
): MarginSummary | null {
    if (!sellingTotal || !costTotal) return null
    try {
        if (compareDecimal(sellingTotal, "0", 6) <= 0) return null
        const diff = subtractFixed(sellingTotal, costTotal, {
            maxScale: 6,
            outputScale: 6,
        })
        const amount = subtractFixed(sellingTotal, costTotal, {
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
            sellingTotal,
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

export function calculateVoucherDiscount(
    sellingPrice: string | null | undefined,
    faceValue: string | null | undefined,
): VoucherDiscountSummary | null {
    if (!sellingPrice || !faceValue) return null
    try {
        if (compareDecimal(faceValue, "0", 6) <= 0) return null
        if (compareDecimal(sellingPrice, "0", 6) <= 0) return null
        const cmp = compareDecimal(sellingPrice, faceValue, 6)
        if (cmp === 0) {
            return { label: "平价", kind: "par" }
        }
        if (cmp < 0) {
            const zhe = compactFixed(
                divideFixed(
                    multiplyFixed(sellingPrice, "10", {
                        leftMaxScale: 6,
                        rightMaxScale: 0,
                        outputScale: 6,
                    }),
                    faceValue,
                    {
                        numeratorMaxScale: 6,
                        denominatorMaxScale: 6,
                        outputScale: 2,
                    },
                ),
            )
            return { label: `${zhe} 折`, kind: "discount" }
        }
        const pct = divideFixed(
            multiplyFixed(sellingPrice, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 6,
            }),
            faceValue,
            {
                numeratorMaxScale: 6,
                denominatorMaxScale: 6,
                outputScale: 1,
            },
        )
        return { label: `溢价 ${pct}%`, kind: "premium" }
    } catch {
        return null
    }
}

/**
 * 判定销售审批提交是否存在负毛利或低于原采购底价等高危复核信号（严格使用定点十进制比较）。
 */
export function assessSalesApprovalRisk(
    submission?: ApprovalSalesSubmission | null,
): { highRisk: boolean; reasons: string[] } {
    if (!submission || submission.business_type === "VOUCHER") {
        return { highRisk: false, reasons: [] }
    }
    const negativeLines = submission.lines.filter((line) =>
        isNegativeMarginLine(line, false),
    )
    const belowFloorOnlyLines = submission.lines.filter(
        (line) =>
            !isNegativeMarginLine(line, false) &&
            isBelowOriginalFloorLine(line, false),
    )
    const reasons: string[] = []
    if (negativeLines.length === 1) {
        const line = negativeLines[0]!
        const name =
            displayReadableName(line.item_name_snapshot, line.sku_id) ||
            "未命名商品"
        reasons.push(
            `第 ${line.line_no} 行「${name}」含税售价低于采购供给价（负毛利风险）`,
        )
    } else if (negativeLines.length > 1) {
        const lineNos = negativeLines
            .slice(0, 3)
            .map((line) => `第 ${line.line_no} 行`)
            .join("、")
        const suffix =
            negativeLines.length > 3 ? `等 ${negativeLines.length} 行` : ""
        reasons.push(
            `共 ${negativeLines.length} 行商品含税售价低于采购供给价（负毛利风险：${lineNos}${suffix}）`,
        )
    }

    if (belowFloorOnlyLines.length === 1) {
        const line = belowFloorOnlyLines[0]!
        const name =
            displayReadableName(line.item_name_snapshot, line.sku_id) ||
            "未命名商品"
        reasons.push(
            `第 ${line.line_no} 行「${name}」采购底价已被调低，且含税售价低于原采购底价`,
        )
    } else if (belowFloorOnlyLines.length > 1) {
        const lineNos = belowFloorOnlyLines
            .slice(0, 3)
            .map((line) => `第 ${line.line_no} 行`)
            .join("、")
        const suffix =
            belowFloorOnlyLines.length > 3
                ? `等 ${belowFloorOnlyLines.length} 行`
                : ""
        reasons.push(
            `共 ${belowFloorOnlyLines.length} 行商品含税售价低于原采购底价（已调低供货价：${lineNos}${suffix}）`,
        )
    }

    return { highRisk: reasons.length > 0, reasons }
}

/** 结合当前销售单检测到的具体异常，动态生成驳回快捷短语。 */
export function buildSalesDynamicRejectPhrases(
    submission?: ApprovalSalesSubmission | null,
    attachmentCount?: number,
): string[] {
    if (!submission) return []
    const phrases: string[] = []
    const isCard = submission.business_type === "VOUCHER"
    if (!isCard) {
        const negativeLines = submission.lines.filter((line) =>
            isNegativeMarginLine(line, false),
        )
        if (negativeLines.length === 1) {
            phrases.push(
                `第 ${negativeLines[0]!.line_no} 行负毛利，售价低于采购供给价`,
            )
        } else if (negativeLines.length > 1) {
            phrases.push(`共 ${negativeLines.length} 行商品负毛利，请核对售价`)
        }
        const belowFloorLines = submission.lines.filter(
            (line) =>
                !isNegativeMarginLine(line, false) &&
                isBelowOriginalFloorLine(line, false),
        )
        if (belowFloorLines.length > 0) {
            phrases.push("部分商品售价低于原采购底价，请说明调价依据")
        }
    }
    if (!submission.contract_no?.trim() && attachmentCount === 0) {
        phrases.push("无合同且未附带凭证附件，请补充盖章依据")
    }
    return phrases
}

export function evaluateSalesApprovalLines(
    submission: ApprovalSalesSubmission | null | undefined,
    resolveUnit: (line: SalesApprovalLine) => string | undefined,
): EvaluatedSalesLine[] {
    if (!submission?.lines.length) return []
    const isCard = submission.business_type === "VOUCHER"
    return submission.lines.map((line) => {
        const unit = resolveUnit(line)
        const name =
            displayReadableName(line.item_name_snapshot, line.sku_id) ||
            "未记录商品名称"
        const rawSpec = displayBusinessText(line.spec_snapshot, line.sku_id)
        const spec = rawSpec && rawSpec !== "无规格" ? rawSpec : undefined
        const welfare = welfareScenarioLabel(line.welfare_scenario ?? "")
        const isGift = line.pricing_mode === "GIFT"
        const negativeMargin = isNegativeMarginLine(line, isCard)
        const belowOriginalFloor = isBelowOriginalFloorLine(line, isCard)
        const priceCheck = isCard ? null : salesLinePriceComparison(line)
        const priceAdjusted = Boolean(priceCheck && priceCheck.change != null)
        const missingSupply =
            !isCard &&
            !isGift &&
            (line.procurement_supply_price_gross == null ||
                line.procurement_supply_price_gross === "")

        let margin: MarginSummary | null = null
        if (!isCard && !isGift) {
            if (
                line.procurement_supply_price_gross &&
                line.quantity &&
                line.gross_amount
            ) {
                try {
                    const lineCost = multiplyFixed(
                        line.procurement_supply_price_gross,
                        line.quantity,
                        { leftMaxScale: 6, rightMaxScale: 6, outputScale: 2 },
                    )
                    margin = calculateSalesMargin(line.gross_amount, lineCost)
                } catch {
                    margin = calculateSalesMargin(
                        line.unit_price_gross,
                        line.procurement_supply_price_gross,
                    )
                }
            } else {
                margin = calculateSalesMargin(
                    line.unit_price_gross,
                    line.procurement_supply_price_gross,
                )
            }
        }

        const voucherDiscount = isCard
            ? calculateVoucherDiscount(line.unit_price_gross, line.face_value)
            : null

        const severity: SalesLineRiskSeverity =
            negativeMargin || belowOriginalFloor
                ? "high"
                : priceAdjusted
                  ? "medium"
                  : missingSupply
                    ? "info"
                    : "none"

        const hasRisk =
            negativeMargin ||
            belowOriginalFloor ||
            priceAdjusted ||
            Boolean(priceCheck && priceCheck.warnings.length > 0)

        return {
            line,
            unit,
            name,
            spec,
            welfare: welfare || undefined,
            isGift,
            negativeMargin,
            belowOriginalFloor,
            priceAdjusted,
            missingSupply,
            priceCheck,
            margin,
            voucherDiscount,
            severity,
            hasRisk,
        }
    })
}

export function buildSalesApprovalMetrics(
    submission: ApprovalSalesSubmission | null | undefined,
    evaluatedLines: readonly EvaluatedSalesLine[],
): SalesApprovalMetrics | null {
    if (!submission) return null
    const isCard = submission.business_type === "VOUCHER"
    const units = new Set<string>()
    const quantities: string[] = []

    let coveredRevenue = "0"
    let coveredCost = "0"
    let giftCost = "0"
    let hasGiftCost = false
    let costCoveredCount = 0
    let costExpectedCount = 0

    let faceValues: string[] = []
    let cardCounts: string[] = []

    let negativeMarginCount = 0
    let belowFloorCount = 0
    let priceAdjustedCount = 0
    let missingSupplyCount = 0
    let giftCount = 0
    let highRiskLineCount = 0
    let riskLineCount = 0

    for (const entry of evaluatedLines) {
        const qty =
            entry.line.quantity ??
            (entry.line.card_count == null
                ? null
                : String(entry.line.card_count))
        if (qty != null) {
            quantities.push(qty)
            units.add(entry.unit ?? "")
        }

        if (entry.negativeMargin) negativeMarginCount += 1
        if (!entry.negativeMargin && entry.belowOriginalFloor)
            belowFloorCount += 1
        if (entry.priceAdjusted) priceAdjustedCount += 1
        if (entry.missingSupply) missingSupplyCount += 1
        if (entry.isGift) giftCount += 1
        if (entry.severity === "high") highRiskLineCount += 1
        if (entry.hasRisk) riskLineCount += 1

        if (isCard) {
            if (entry.line.card_count != null) {
                cardCounts.push(String(entry.line.card_count))
                if (entry.line.face_value) {
                    try {
                        faceValues.push(
                            multiplyFixed(
                                entry.line.face_value,
                                String(entry.line.card_count),
                                {
                                    leftMaxScale: 6,
                                    rightMaxScale: 0,
                                    outputScale: 2,
                                },
                            ),
                        )
                    } catch {
                        // 忽略异常面额格式
                    }
                }
            }
        } else {
            const supply = entry.line.procurement_supply_price_gross
            if (entry.isGift) {
                if (supply != null && supply !== "" && qty != null) {
                    try {
                        const lineCost = multiplyFixed(supply, qty, {
                            leftMaxScale: 6,
                            rightMaxScale: 6,
                            outputScale: 2,
                        })
                        giftCost = sumFixed([giftCost, lineCost], {
                            maxScale: 2,
                            outputScale: 2,
                        })
                        coveredCost = sumFixed([coveredCost, lineCost], {
                            maxScale: 2,
                            outputScale: 2,
                        })
                        hasGiftCost = true
                    } catch {
                        // 忽略异常数值
                    }
                }
            } else {
                costExpectedCount += 1
                if (supply != null && supply !== "" && qty != null) {
                    try {
                        const lineCost = multiplyFixed(supply, qty, {
                            leftMaxScale: 6,
                            rightMaxScale: 6,
                            outputScale: 2,
                        })
                        coveredRevenue = sumFixed(
                            [coveredRevenue, entry.line.gross_amount],
                            { maxScale: 2, outputScale: 2 },
                        )
                        coveredCost = sumFixed([coveredCost, lineCost], {
                            maxScale: 2,
                            outputScale: 2,
                        })
                        costCoveredCount += 1
                    } catch {
                        // 忽略异常数值
                    }
                }
            }
        }
    }

    const totalQuantity =
        evaluatedLines.length > 0 &&
        units.size === 1 &&
        quantities.length === evaluatedLines.length
            ? {
                  value: compactFixed(
                      sumFixed(quantities, { maxScale: 6, outputScale: 6 }),
                  ),
                  unit: [...units][0] || undefined,
              }
            : null

    const estimatedMargin =
        !isCard && costCoveredCount > 0
            ? calculateSalesMargin(coveredRevenue, coveredCost)
            : null

    const totalFaceValue =
        isCard && faceValues.length > 0
            ? sumFixed(faceValues, { maxScale: 2, outputScale: 2 })
            : null
    const totalCards =
        isCard && cardCounts.length > 0
            ? compactFixed(
                  sumFixed(cardCounts, { maxScale: 0, outputScale: 0 }),
              )
            : null
    const overallVoucherDiscount =
        isCard && totalFaceValue
            ? calculateVoucherDiscount(submission.gross_amount, totalFaceValue)
            : null

    return {
        isCard,
        lineCount: evaluatedLines.length,
        totalQuantity,
        estimatedCostTotal:
            !isCard && (costCoveredCount > 0 || hasGiftCost)
                ? coveredCost
                : null,
        giftCostTotal: hasGiftCost ? giftCost : null,
        estimatedMargin,
        costCoveredCount,
        costExpectedCount,
        partialCostCoverage:
            !isCard &&
            costCoveredCount > 0 &&
            costCoveredCount < costExpectedCount,
        totalFaceValue,
        totalCards,
        overallVoucherDiscount,
        negativeMarginCount,
        belowFloorCount,
        priceAdjustedCount,
        missingSupplyCount,
        giftCount,
        highRiskLineCount,
        riskLineCount,
    }
}

export function parseSalesResubmission(
    extraSections: ApprovalMaterials["display"]["source"]["extra_sections"],
    item: WorkspaceWorkItem,
): SalesResubmissionContext | null {
    const resubmissionReason = extraSections
        .find((section) => section.label === "重新提交说明")
        ?.value.trim()
    const roundNo = item.approval?.currentRoundNo
    const isResubmission = Boolean(
        resubmissionReason || (roundNo != null && roundNo > 1),
    )
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

    if (!isResubmission && !previousRejection) return null
    return {
        isResubmission,
        roundNo,
        resubmissionReason: resubmissionReason || undefined,
        previousRejection,
    }
}
