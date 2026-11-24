import type {
    ApprovalMaterials,
    ApprovalPurchaseFulfillment,
    ApprovalPurchaseLine,
    ApprovalSalesRevision,
} from "@/features/approval-workflow/api/materials"
import { approvalSalesTaxPoint } from "@/features/approval-workflow/display"
import {
    countUniqueDeliveryPoints,
    formatPurchaseFulfillmentSummary,
} from "@/features/approval-workflow/purchase-fulfillment"
import { paymentTermCode, welfareScenarioLabel } from "@/lib/business-options"
import { formatIsoDate, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { displayBusinessText, displayName } from "@/lib/display-name"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    normalizeFixed,
    splitGrossByPercentRate,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import { parsePeriodicTerm } from "@/lib/supplier-payment-terms"
import {
    isPurchaseFeeLine,
    purchaseComparison,
    purchaseSpecification,
    sumPurchaseMargins,
    type ExternalMarginSummary,
    type PurchaseMarginAmounts,
    type PurchaseMarginTotals,
} from "./workspace-purchase-comparison-model"

export type SourceSalesOrderEntry = NonNullable<
    ApprovalMaterials["source_sales_orders"]
>[number]

export type SourceSalesSnapshotEntry = NonNullable<
    ApprovalMaterials["display"]["source_sales"]
>[number]

export type ComparisonSeverity =
    | "severe"
    | "warning"
    | "info"
    | "ok"
    | "unknown"

export type CommercialComparisonItem = Readonly<{
    id: "payment" | "tax" | "fulfillment"
    title: string
    shortTitle: string
    salesLabel: string
    salesValue: string
    purchaseLabel: string
    purchaseValue: string
    /** 紧凑对照短值（去除重复前缀，便于 销 → 采 并排扫读）。 */
    compactSalesValue: string
    compactPurchaseValue: string
    /** 采销相对方向：down 表示采购侧弱于销售侧（如垫资/低进项），up 表示顺差。 */
    direction: "down" | "up" | "neutral"
    /** 精简量化摘要（如「含税 4.2% → 税后 -4.5%」「账期短 15 天」），无则省略。 */
    metric: string | null
    status: ComparisonSeverity
    badge: string
    detail: string
}>

export type MatchedPurchaseLineSummary = Readonly<{
    id: string
    lineNo: number
    title: string
    specification: string
    quantity: string | null
    unit: string | null
    unitName?: string
    unitCostGross: string | null
    grossAmount: string | null
    netAmount: string | null
    taxAmount: string | null
    inputTaxRatePercent: string | null
    expectedDeliveryDate: string | null
    delayDays: number
    mismatchedProduct: boolean
    unitMismatch: boolean
}>

export type SalesLineCoverageStatus =
    | "full"
    | "partial"
    | "excess"
    | "uncovered"
    | "mismatch"

export type SalesLineCoverageRow = Readonly<{
    salesLineId: string
    salesLineNo: number
    title: string
    specification: string
    unitCode: string | null
    unitLabel: string | null
    isGift: boolean
    welfareLabel: string | null
    salesQuantity: string | null
    salesUnitPriceGross: string | null
    salesGrossAmount: string
    salesNetAmount: string
    salesTaxRatePercent: string
    salesDueDate: string | null
    priceBasisLabel: "集采" | "一件代发" | null
    originalSupplyPrice: string | null
    supplyPrice: string | null
    supplyAdjustment: "increased" | "decreased" | "unchanged" | null
    coverageStatus: SalesLineCoverageStatus
    /** 本单采购数量占销售行需求数量的百分比（0–100，用于行内进度条）。 */
    coverageRatioPercent: string | null
    matchedPurchaseLines: readonly MatchedPurchaseLineSummary[]
    purchasedQuantityTotal: string | null
    excessQuantity: string | null
    remainingQuantity: string | null
    purchaseGrossTotal: string | null
    purchaseMargin: PurchaseMarginTotals | null
    purchaseNetTotal: string | null
    inputTaxRatesPercent: readonly string[]
    lowerInputTaxGapPoints: string | null
    grossMargin: ExternalMarginSummary | null
    netMargin: ExternalMarginSummary | null
    taxInverted: boolean
    costAboveSupply: boolean
    maxDelayDays: number
    mismatchedProduct: boolean
    unitMismatch: boolean
    hasAttention: boolean
    severity: Exclude<ComparisonSeverity, "ok" | "unknown"> | "none"
    attentionTags: readonly Readonly<{
        label: string
        tone: "destructive" | "warning" | "info"
    }>[]
    /** 销售单内存在完全相同品规价格数量的多行时提示聚合口径。 */
    duplicateGroupSize: number
}>

export type ExtraPurchaseLineItem = Readonly<{
    id: string
    lineNo: number
    title: string
    grossAmount: string | null
    netAmount: string | null
    inputTaxRatePercent: string | null
    isFee: boolean
    ratioOfSalesPercent: string | null
}>

export type SourceOrderBusinessLedger = Readonly<{
    isExpenseOrder: boolean
    salesGrossTotal: string
    salesNetTotal: string
    salesTaxTotal: string
    totalSalesLineCount: number
    giftSalesLineCount: number
    fullCount: number
    partialCount: number
    excessCount: number
    mismatchCount: number
    uncoveredCount: number
    coveredLineCount: number
    purchaseGoodsGrossTotal: string | null
    purchaseGoodsLineCount: number
    purchaseMargin: PurchaseMarginTotals | null
    purchaseGoodsNetTotal: string | null
    purchaseFeeGrossTotal: string | null
    purchaseFeeRatioPercent: string | null
    unallocatedSharedFeeTotal: string | null
    correspondingSalesGrossTotal: string | null
    correspondingSalesNetTotal: string | null
    salesCoveragePercent: string | null
    /** 本单可比采购成本占对应对外销售额的百分比（0–100，用于结构占比条）。 */
    costBarPercent: string | null
    goodsCostBarPercent: string | null
    feeCostBarPercent: string | null
    /** 费用单：本单费用占销售总额的百分比（0–100）。 */
    expenseFeeBarPercent: string | null
    grossMargin: ExternalMarginSummary | null
    netMargin: ExternalMarginSummary | null
    vatGap: string | null
    taxDragPoints: string | null
    taxInverted: boolean
    giftCoveredCount: number
    giftPurchaseGrossTotal: string | null
    isFullOrderCovered: boolean
    extraCostSummary: Readonly<{
        occurredGrossTotal: string
        pendingGrossTotal: string
        combinedWithCurrentFeeTotal: string | null
        combinedRatioPercent: string | null
    }> | null
}>

export type SourceOrderComparison = Readonly<{
    revisionId: string
    documentNo: string
    revisionNo: number
    customerName: string
    projectName: string | null
    settlementPartyName: string | null
    contractNo: string | null
    contractNotices: readonly string[]
    businessRemark: string | null
    hasFullOrder: boolean
    commercialChecks: readonly CommercialComparisonItem[]
    ledger: SourceOrderBusinessLedger | null
    coverageRows: readonly SalesLineCoverageRow[]
    extraPurchaseLines: readonly ExtraPurchaseLineItem[]
    attentionCount: number
    hasSevereAttention: boolean
    /** 本单是否合并了多张来源销售单。 */
    mergedOrderCount: number
}>

export type PurchaseSourceComparisonModel = Readonly<{
    orders: readonly SourceOrderComparison[]
    totalAttentionCount: number
    hasSevereAttention: boolean
}>

export type PurchaseSourceComparisonModelInput = Readonly<{
    orders: readonly SourceSalesOrderEntry[]
    snapshots?: readonly SourceSalesSnapshotEntry[]
    purchaseLines?: readonly ApprovalPurchaseLine[] | null
    purchaseFulfillment?: ApprovalPurchaseFulfillment | null
    purchaseExtraSections?: ApprovalMaterials["display"]["source"]["extra_sections"]
    extraCostSummary?: ApprovalMaterials["extra_cost_summary"]
    unitNames?: ReadonlyMap<string, string>
}>

type SalesCommercialLine = ApprovalSalesRevision["commercial_lines"][number]

type PaymentTiming =
    | { kind: "prepay"; tier: 100 | 50 | 30 | null }
    | { kind: "cash" }
    | { kind: "postpay"; netDays: number | null; periodicRank: number | null }
    | { kind: "contract" }
    | { kind: "unknown" }

function safeNormalizeDecimal(
    value: string | null | undefined,
    scale = 6,
): string | null {
    const trimmed = value?.trim()
    if (!trimmed) return null
    try {
        return compactFixed(
            normalizeFixed(trimmed, {
                maxScale: scale,
                outputScale: scale,
                allowNegative: true,
            }),
        )
    } catch {
        return trimmed
    }
}

function formatRatePercent(rate: string | null | undefined): string | null {
    const trimmed = rate?.trim()
    if (!trimmed) return null
    try {
        return compactFixed(
            multiplyFixed(trimmed, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 2,
            }),
        )
    } catch {
        return null
    }
}

function formatDueDate(unixSecs: number | null | undefined): string | null {
    if (unixSecs == null || !Number.isFinite(unixSecs)) return null
    const iso = unixSecondsToIsoOrEmpty(unixSecs)
    return formatIsoDate(iso, { timeZone: "Asia/Shanghai" })
}

function calculateMargin(
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

function signedSubtract(left: string, right: string, scale: number): string {
    const trimmedRight = right.trim()
    const negatedRight = trimmedRight.startsWith("-")
        ? trimmedRight.slice(1)
        : `-${trimmedRight}`
    return sumFixed([left.trim(), negatedRight], {
        maxScale: scale,
        outputScale: scale,
        allowNegative: true,
    })
}

function safeRatioPercent(
    numerator: string,
    denominator: string,
): string | null {
    try {
        if (compareDecimal(denominator, "0", 2) <= 0) return null
        return divideFixed(
            multiplyFixed(numerator, "100", {
                leftMaxScale: 2,
                rightMaxScale: 0,
                outputScale: 4,
            }),
            denominator,
            {
                numeratorMaxScale: 4,
                denominatorMaxScale: 2,
                outputScale: 1,
            },
        )
    } catch {
        return null
    }
}

function clampPercent(value: string | null): string | null {
    if (value == null) return null
    try {
        if (compareDecimal(value, "0", 1) <= 0) return "0"
        if (compareDecimal(value, "100", 1) >= 0) return "100"
        return value
    } catch {
        return null
    }
}

function safeClampedRatioPercent(
    numerator: string,
    denominator: string,
    scale = 2,
): string | null {
    try {
        if (compareDecimal(denominator, "0", scale) <= 0) return null
        if (compareDecimal(numerator, "0", scale) <= 0) return "0"
        if (compareDecimal(numerator, denominator, scale) >= 0) return "100"
        return clampPercent(
            divideFixed(
                multiplyFixed(numerator, "100", {
                    leftMaxScale: scale,
                    rightMaxScale: 0,
                    outputScale: scale + 2,
                }),
                denominator,
                {
                    numeratorMaxScale: scale + 2,
                    denominatorMaxScale: scale,
                    outputScale: 1,
                },
            ),
        )
    } catch {
        return null
    }
}

function shortInvoiceType(invoiceTypeText: string): string {
    if (invoiceTypeText.includes("专票") || invoiceTypeText.includes("专用")) {
        return "专票"
    }
    if (invoiceTypeText.includes("普票") || invoiceTypeText.includes("普通")) {
        return "普票"
    }
    if (invoiceTypeText === "未记录开票类型") return ""
    return invoiceTypeText
}

function periodicRankFor(code: string): number | null {
    const upper = code.toUpperCase()
    if (upper.includes("WEEK") || code.includes("周结")) return 7
    if (upper.includes("SEMIANNUAL") || code.includes("半年结")) return 180
    if (upper.includes("QUARTER") || code.includes("季结")) return 90
    if (upper.includes("ANNUAL") || code.includes("年结")) return 365
    if (upper.includes("MONTH") || code.includes("月结")) return 30
    return null
}

function classifyPaymentTiming(
    code?: string | null,
    label?: string | null,
): PaymentTiming {
    const rawCode = code?.trim() ?? ""
    const rawLabel = label?.trim() ?? ""
    const normalized =
        (rawCode && paymentTermCode(rawCode)) ||
        (rawLabel && paymentTermCode(rawLabel)) ||
        undefined

    if (normalized === "PREPAY_100") return { kind: "prepay", tier: 100 }
    if (normalized === "PREPAY_50") return { kind: "prepay", tier: 50 }
    if (normalized === "PREPAY_30") return { kind: "prepay", tier: 30 }
    if (normalized === "CASH_ON_APPROVAL") return { kind: "cash" }
    if (normalized === "POSTPAY_NET15") {
        return { kind: "postpay", netDays: 15, periodicRank: null }
    }
    if (normalized === "POSTPAY_NET30") {
        return { kind: "postpay", netDays: 30, periodicRank: null }
    }
    if (normalized === "CONTRACT") return { kind: "contract" }

    const periodic =
        (rawCode && parsePeriodicTerm(rawCode)) ||
        (rawLabel && parsePeriodicTerm(rawLabel)) ||
        null
    if (periodic) {
        return {
            kind: "postpay",
            netDays: null,
            periodicRank: periodicRankFor(periodic.code),
        }
    }

    const combined = `${rawCode} ${rawLabel}`
    if (/先款|预付/.test(combined)) {
        if (combined.includes("100%")) return { kind: "prepay", tier: 100 }
        if (combined.includes("50%")) return { kind: "prepay", tier: 50 }
        if (combined.includes("30%")) return { kind: "prepay", tier: 30 }
        return { kind: "prepay", tier: null }
    }
    if (combined.includes("现结")) return { kind: "cash" }
    const netMatch = /货到\s*(\d+)\s*天|NET[-_]?(\d+)/i.exec(combined)
    if (netMatch) {
        const parsedDays = parseInt(netMatch[1] ?? netMatch[2] ?? "", 10)
        return {
            kind: "postpay",
            netDays: Number.isFinite(parsedDays) ? parsedDays : null,
            periodicRank: null,
        }
    }
    const rank = periodicRankFor(combined)
    if (rank != null || /账期|货到|后付/.test(combined)) {
        return { kind: "postpay", netDays: null, periodicRank: rank }
    }
    if (combined.includes("按合同")) return { kind: "contract" }
    return { kind: "unknown" }
}

function buildPaymentCheck(
    revision: ApprovalSalesRevision,
    purchasePaymentLabel: string | null,
): CommercialComparisonItem {
    const salesLabelText =
        displayName(revision.payment_term_name) ||
        revision.payment_term_code?.trim() ||
        ""
    const salesTiming = classifyPaymentTiming(
        revision.payment_term_code,
        salesLabelText,
    )
    const purchaseTiming = classifyPaymentTiming(null, purchasePaymentLabel)

    const base = {
        id: "payment" as const,
        title: "资金与付款条件",
        shortTitle: "资金",
        salesLabel: "客户付款",
        salesValue: salesLabelText || "未记录",
        purchaseLabel: "供应商付款",
        purchaseValue: purchasePaymentLabel || "未记录",
        compactSalesValue: salesLabelText || "未记录",
        compactPurchaseValue: purchasePaymentLabel || "未记录",
        direction: "neutral" as const,
        metric: null as string | null,
    }

    if (!salesLabelText || !purchasePaymentLabel) {
        return {
            ...base,
            status: "unknown",
            badge: "待核对",
            detail: "采销付款条件记录不完整，请结合合同确认收付款节奏。",
        }
    }

    if (
        (purchaseTiming.kind === "prepay" || purchaseTiming.kind === "cash") &&
        salesTiming.kind === "postpay"
    ) {
        return {
            ...base,
            direction: "down",
            status: "warning",
            badge: "需垫资",
            detail:
                purchaseTiming.kind === "prepay"
                    ? "采购需先款，但客户为后付/周期结算，存在资金垫付风险。"
                    : "采购需现结，但客户为后付/周期结算，存在资金垫付风险。",
        }
    }

    if (purchaseTiming.kind === "prepay" && salesTiming.kind === "cash") {
        return {
            ...base,
            direction: "down",
            status: "warning",
            badge: "提前垫款",
            detail: "采购需先款，早于客户现结收款节点。",
        }
    }

    if (purchaseTiming.kind === "prepay" && salesTiming.kind === "contract") {
        return {
            ...base,
            status: "info",
            badge: "核对合同账期",
            detail: "采购需先款，客户按合同约定收款，请核对合同回款节点。",
        }
    }

    if (purchaseTiming.kind === "prepay" && salesTiming.kind === "prepay") {
        if (
            purchaseTiming.tier != null &&
            salesTiming.tier != null &&
            purchaseTiming.tier > salesTiming.tier
        ) {
            return {
                ...base,
                direction: "down",
                metric: `预付高 ${purchaseTiming.tier - salesTiming.tier} 个点`,
                status: "warning",
                badge: "预付比例偏高",
                detail: `采购先款比例（${purchaseTiming.tier}%）高于客户预付比例（${salesTiming.tier}%），需部分垫资。`,
            }
        }
        return {
            ...base,
            status: "ok",
            badge: "资金匹配",
            detail: "采销均为先款条件，资金节奏基本匹配。",
        }
    }

    if (
        salesTiming.kind === "prepay" &&
        (purchaseTiming.kind === "cash" || purchaseTiming.kind === "postpay")
    ) {
        return {
            ...base,
            direction: "up",
            status: "ok",
            badge: "资金顺差",
            detail: "客户先款、采购后付或现结，资金占用压力较低。",
        }
    }

    if (purchaseTiming.kind === "postpay" && salesTiming.kind === "postpay") {
        if (
            purchaseTiming.netDays != null &&
            salesTiming.netDays != null &&
            purchaseTiming.netDays < salesTiming.netDays
        ) {
            const gap = salesTiming.netDays - purchaseTiming.netDays
            return {
                ...base,
                direction: "down",
                metric: `账期短 ${gap} 天`,
                status: "warning",
                badge: "账期错配",
                detail: `供应商账期比客户账期短 ${gap} 天，存在阶段垫资。`,
            }
        }
        if (
            purchaseTiming.periodicRank != null &&
            salesTiming.periodicRank != null &&
            purchaseTiming.periodicRank < salesTiming.periodicRank
        ) {
            return {
                ...base,
                direction: "down",
                status: "warning",
                badge: "结算周期更短",
                detail: "供应商结算周期短于客户结算周期，请留意回款衔接。",
            }
        }
        return {
            ...base,
            status: "ok",
            badge: "资金匹配",
            detail: "采销均为后付/周期结算，账期条件可控。",
        }
    }

    if (salesTiming.kind === "contract") {
        return {
            ...base,
            status: "info",
            badge: "按合同核对",
            detail: "客户按合同约定收款，请对照附件合同确认回款节奏。",
        }
    }

    return {
        ...base,
        status: "info",
        badge: "待核对",
        detail: "请结合附件合同核对采销收付款条件。",
    }
}

function salesLineMatchKey(input: {
    title: string
    specification: string | null | undefined
    unit: string | null | undefined
    quantity: string | null | undefined
    unitPriceGross: string | null | undefined
    supplyPriceGross: string | null | undefined
    fulfillmentDueAt: number | null | undefined
}): string {
    return JSON.stringify([
        input.title.trim(),
        purchaseSpecification(input.specification ?? null),
        (input.unit ?? "").trim(),
        safeNormalizeDecimal(input.quantity) ?? "",
        safeNormalizeDecimal(input.unitPriceGross) ?? "",
        safeNormalizeDecimal(input.supplyPriceGross) ?? "",
        input.fulfillmentDueAt ?? null,
    ])
}

function resolveSalesUnitLabel(
    line: SalesCommercialLine,
    unitNames?: ReadonlyMap<string, string>,
): string | null {
    const direct = displayName(line.unit_name)
    if (direct) return direct
    const code = line.base_unit_code?.trim()
    const snapshot = displayName(line.unit_snapshot)
    if (code && unitNames?.get(code)) return unitNames.get(code)!
    if (snapshot && unitNames?.get(snapshot)) return unitNames.get(snapshot)!
    return snapshot || code || null
}

function priceBasisLabelOf(
    basis?: "BULK" | "DROPSHIP" | null,
): "集采" | "一件代发" | null {
    if (basis === "BULK") return "集采"
    if (basis === "DROPSHIP") return "一件代发"
    return null
}

function uniqueSortedRates(rates: readonly string[]): string[] {
    const unique = [...new Set(rates)]
    return unique.toSorted((left, right) => {
        try {
            return compareDecimal(right, left, 2)
        } catch {
            return left.localeCompare(right)
        }
    })
}

function formatRateList(rates: readonly string[]): string {
    if (!rates.length) return "未记录"
    return uniqueSortedRates(rates)
        .map((value) => `${value}%`)
        .join(" / ")
}

function buildOrderComparison({
    order,
    snapshot,
    allPurchaseLines,
    mergedOrderCount,
    purchaseFulfillment,
    purchasePaymentLabel,
    extraCostSummary,
    unitNames,
}: {
    order?: SourceSalesOrderEntry
    snapshot?: SourceSalesSnapshotEntry
    allPurchaseLines: readonly ApprovalPurchaseLine[]
    mergedOrderCount: number
    purchaseFulfillment?: ApprovalPurchaseFulfillment | null
    purchasePaymentLabel: string | null
    extraCostSummary?: ApprovalMaterials["extra_cost_summary"]
    unitNames?: ReadonlyMap<string, string>
}): SourceOrderComparison {
    const revision = order?.revision
    const revisionId = revision?.id ?? snapshot?.revision_id ?? ""
    const documentNo =
        displayBusinessText(
            order?.document_no ?? snapshot?.document_no,
            snapshot?.document_id ?? revisionId,
        ) || "未记录销售单号"
    const revisionNo = revision?.revision_no ?? snapshot?.revision_no ?? 1
    const customerName =
        displayName(revision?.customer_name) ||
        displayName(snapshot?.source.customer) ||
        "未记录客户名称"
    const contractNotices = (snapshot?.source.extra_sections ?? [])
        .filter((field) => field.label === "合同或凭证" && field.value.trim())
        .map((field) => field.value.trim())

    const goodsPurchaseLines = allPurchaseLines.filter(
        (line) =>
            !isPurchaseFeeLine(line) &&
            (mergedOrderCount <= 1 || line.source?.revision_id === revisionId),
    )
    const feePurchaseLines = allPurchaseLines.filter((line) =>
        isPurchaseFeeLine(line),
    )
    const isExpenseOrder =
        allPurchaseLines.length > 0 &&
        allPurchaseLines.every((line) => isPurchaseFeeLine(line))

    if (!revision) {
        return {
            revisionId,
            documentNo,
            revisionNo,
            customerName,
            projectName: null,
            settlementPartyName: null,
            contractNo: null,
            contractNotices,
            businessRemark: null,
            hasFullOrder: false,
            commercialChecks: [],
            ledger: null,
            coverageRows: [],
            extraPurchaseLines: [],
            attentionCount: 0,
            hasSevereAttention: false,
            mergedOrderCount,
        }
    }

    // 1. 将来源销售单明细与本次采购商品行按不可变快照字段精确配对
    const salesLines = revision.commercial_lines
    const salesByKey = new Map<string, SalesCommercialLine[]>()
    for (const salesLine of salesLines) {
        const key = salesLineMatchKey({
            title: salesLine.item_name_snapshot,
            specification: salesLine.spec_snapshot,
            unit: salesLine.base_unit_code ?? salesLine.unit_snapshot,
            quantity: salesLine.quantity,
            unitPriceGross: salesLine.unit_price_gross,
            supplyPriceGross: salesLine.procurement_supply_price_gross,
            fulfillmentDueAt: salesLine.fulfillment_due_at,
        })
        const bucket = salesByKey.get(key)
        if (bucket) bucket.push(salesLine)
        else salesByKey.set(key, [salesLine])
    }

    const purchaseByKey = new Map<string, ApprovalPurchaseLine[]>()
    const unmatchedPurchaseLines: ApprovalPurchaseLine[] = []
    for (const line of goodsPurchaseLines) {
        if (!line.source || line.source.revision_id !== revisionId) {
            if (!line.source) unmatchedPurchaseLines.push(line)
            continue
        }
        const key = salesLineMatchKey({
            title: line.source.title,
            specification: line.source.specification,
            unit: line.source.unit,
            quantity: line.source.quantity,
            unitPriceGross: line.source.unit_price_gross,
            supplyPriceGross: line.source.procurement_supply_price_gross,
            fulfillmentDueAt: line.source.fulfillment_due_at,
        })
        if (!salesByKey.has(key)) {
            unmatchedPurchaseLines.push(line)
            continue
        }
        const bucket = purchaseByKey.get(key)
        if (bucket) bucket.push(line)
        else purchaseByKey.set(key, [line])
    }

    // 处理罕见同名同价同量多销售行：优先把匹配到的采购行顺序分配给对应桶，不重复统计金额
    const assignedPurchaseLinesBySalesId = new Map<
        string,
        ApprovalPurchaseLine[]
    >()
    const duplicateGroupSizeBySalesId = new Map<string, number>()

    for (const [key, groupSalesLines] of salesByKey.entries()) {
        const matched = purchaseByKey.get(key) ?? []
        for (const sLine of groupSalesLines) {
            duplicateGroupSizeBySalesId.set(sLine.id, groupSalesLines.length)
            assignedPurchaseLinesBySalesId.set(sLine.id, [])
        }
        if (matched.length === 0) continue
        if (groupSalesLines.length === 1) {
            assignedPurchaseLinesBySalesId.set(groupSalesLines[0]!.id, matched)
        } else if (matched.length === groupSalesLines.length) {
            groupSalesLines.forEach((sLine, idx) => {
                assignedPurchaseLinesBySalesId.set(sLine.id, [matched[idx]!])
            })
        } else {
            // 按各销售行剩余需求量依次归集，避免多条完全相同销售行拆分采购时误报超量
            const remainingByIndex = groupSalesLines.map(
                (sLine) => sLine.quantity ?? "0",
            )
            for (const pLine of matched) {
                let targetIdx = 0
                if (pLine.quantity != null) {
                    const fitIdx = remainingByIndex.findIndex((rem) => {
                        try {
                            return compareDecimal(rem, pLine.quantity!, 6) >= 0
                        } catch {
                            return false
                        }
                    })
                    if (fitIdx >= 0) {
                        targetIdx = fitIdx
                        try {
                            remainingByIndex[fitIdx] = subtractFixed(
                                remainingByIndex[fitIdx]!,
                                pLine.quantity,
                                { maxScale: 6, outputScale: 6 },
                            )
                        } catch {
                            // 保持原剩余量
                        }
                    } else {
                        const positiveIdx = remainingByIndex.findIndex(
                            (rem) => {
                                try {
                                    return compareDecimal(rem, "0", 6) > 0
                                } catch {
                                    return false
                                }
                            },
                        )
                        if (positiveIdx >= 0) {
                            targetIdx = positiveIdx
                            remainingByIndex[positiveIdx] = "0"
                        }
                    }
                }
                const targetSalesId = groupSalesLines[targetIdx]!.id
                assignedPurchaseLinesBySalesId.get(targetSalesId)!.push(pLine)
            }
        }
    }

    // 2. 逐条构造销售行覆盖视图 + 聚合整单财务/税务/履约统计
    const coverageRows: SalesLineCoverageRow[] = []

    const orderPurchaseGoodsGross: string[] = []
    const orderPurchaseMargins: PurchaseMarginAmounts[] = []
    const orderPurchaseGoodsNet: string[] = []
    const orderPurchaseGoodsTax: string[] = []
    const orderComparableSalesGross: string[] = []
    const orderComparableSalesNet: string[] = []
    const orderComparableSalesTax: string[] = []
    const orderComparableCostGross: string[] = []
    const orderComparableCostNet: string[] = []
    const orderComparableCostTax: string[] = []
    const giftPurchaseGrossAmounts: string[] = []

    const matchedSalesTaxRates: string[] = []
    const matchedInputTaxRates: string[] = []
    const matchedBases = new Set<"BULK" | "DROPSHIP">()
    let hasSupplyWithoutBasis = false
    let adjustedSupplyLineCount = 0
    let lowerInputTaxLineCount = 0
    let missingInputTaxCount = 0
    let zeroInputTaxCount = 0
    let comparableTaxLineCount = 0
    let invertedTaxLineCount = 0
    let allComparableHaveNet = true

    let fullCount = 0
    let partialCount = 0
    let excessCount = 0
    let mismatchCount = 0
    let uncoveredCount = 0
    let giftSalesLineCount = 0
    let giftCoveredCount = 0

    for (const salesLine of salesLines) {
        const isGift = salesLine.pricing_mode === "GIFT"
        if (isGift) giftSalesLineCount += 1

        const matched = assignedPurchaseLinesBySalesId.get(salesLine.id) ?? []
        const duplicateGroupSize =
            duplicateGroupSizeBySalesId.get(salesLine.id) ?? 1
        const salesTaxRatePercent =
            formatRatePercent(salesLine.sales_tax_rate) ?? "0"
        const salesUnitLabel = resolveSalesUnitLabel(salesLine, unitNames)
        const salesDueDate = formatDueDate(salesLine.fulfillment_due_at)
        const salesPriceCheck = salesLinePriceComparison(salesLine)
        const priceBasisLabel = priceBasisLabelOf(
            salesLine.procurement_price_basis,
        )

        if (matched.length === 0) {
            uncoveredCount += 1
            coverageRows.push({
                salesLineId: salesLine.id,
                salesLineNo: salesLine.line_no,
                title:
                    displayName(salesLine.item_name_snapshot) ||
                    "未记录商品名称",
                specification: purchaseSpecification(salesLine.spec_snapshot),
                unitCode: salesLine.base_unit_code ?? null,
                unitLabel: salesUnitLabel,
                isGift,
                welfareLabel: salesLine.welfare_scenario
                    ? welfareScenarioLabel(salesLine.welfare_scenario)
                    : null,
                salesQuantity: salesLine.quantity
                    ? compactFixed(salesLine.quantity)
                    : salesLine.card_count != null
                      ? String(salesLine.card_count)
                      : null,
                salesUnitPriceGross: salesLine.unit_price_gross ?? null,
                salesGrossAmount: salesLine.gross_amount,
                salesNetAmount: salesLine.net_amount,
                salesTaxRatePercent,
                salesDueDate,
                priceBasisLabel,
                originalSupplyPrice:
                    salesLine.original_procurement_supply_price_gross ?? null,
                supplyPrice: salesLine.procurement_supply_price_gross ?? null,
                supplyAdjustment: salesPriceCheck.adjustment,
                coverageStatus: "uncovered",
                coverageRatioPercent: "0",
                matchedPurchaseLines: [],
                purchasedQuantityTotal: null,
                excessQuantity: null,
                remainingQuantity: salesLine.quantity
                    ? compactFixed(salesLine.quantity)
                    : null,
                purchaseGrossTotal: null,
                purchaseMargin: null,
                purchaseNetTotal: null,
                inputTaxRatesPercent: [],
                lowerInputTaxGapPoints: null,
                grossMargin: null,
                netMargin: null,
                taxInverted: false,
                costAboveSupply: false,
                maxDelayDays: 0,
                mismatchedProduct: false,
                unitMismatch: false,
                hasAttention: false,
                severity: "none",
                attentionTags: [],
                duplicateGroupSize,
            })
            continue
        }

        if (isGift) giftCoveredCount += 1
        if (salesLine.procurement_price_basis) {
            matchedBases.add(salesLine.procurement_price_basis)
        } else if (salesLine.procurement_supply_price_gross) {
            hasSupplyWithoutBasis = true
        }
        if (
            salesPriceCheck.adjustment === "increased" ||
            salesPriceCheck.adjustment === "decreased"
        ) {
            adjustedSupplyLineCount += 1
        }
        matchedSalesTaxRates.push(salesTaxRatePercent)

        const matchedSummaries: MatchedPurchaseLineSummary[] = []
        const lineQuantities: string[] = []
        const linePurchaseGross: string[] = []
        const linePurchaseMargins: PurchaseMarginAmounts[] = []
        const linePurchaseNet: string[] = []
        const linePurchaseTax: string[] = []
        const lineSalesGrossParts: string[] = []
        const lineSalesNetParts: string[] = []
        const lineSalesTaxParts: string[] = []
        const lineInputRatesPercent: string[] = []

        let lineHasUnitMismatch = false
        let lineHasProductMismatch = false
        let lineHasCostAboveSupply = false
        let lineMaxDelayDays = 0
        let lineMaxTaxGap: string | null = null
        let lineAllNetReady = true

        for (const pLine of matched) {
            const cmp = purchaseComparison(pLine)
            if (cmp.unitMismatch) lineHasUnitMismatch = true
            if (cmp.mismatchedProduct) lineHasProductMismatch = true
            if (cmp.supplyUnitShortfall) lineHasCostAboveSupply = true
            if (cmp.purchaseMargin) {
                linePurchaseMargins.push(cmp.purchaseMargin)
                orderPurchaseMargins.push(cmp.purchaseMargin)
            }
            if (cmp.delayDays > lineMaxDelayDays) {
                lineMaxDelayDays = cmp.delayDays
            }

            const inputRatePercent = formatRatePercent(pLine.input_tax_rate)
            if (inputRatePercent != null) {
                lineInputRatesPercent.push(inputRatePercent)
                matchedInputTaxRates.push(inputRatePercent)
                comparableTaxLineCount += 1
                try {
                    if (compareDecimal(inputRatePercent, "0", 2) === 0) {
                        zeroInputTaxCount += 1
                    }
                    if (
                        compareDecimal(
                            inputRatePercent,
                            salesTaxRatePercent,
                            2,
                        ) < 0
                    ) {
                        const gap = compactFixed(
                            subtractFixed(
                                salesTaxRatePercent,
                                inputRatePercent,
                                { maxScale: 2, outputScale: 2 },
                            ),
                        )
                        if (
                            lineMaxTaxGap == null ||
                            compareDecimal(gap, lineMaxTaxGap, 2) > 0
                        ) {
                            lineMaxTaxGap = gap
                        }
                    }
                } catch {
                    // 忽略异常税率格式
                }
            } else {
                missingInputTaxCount += 1
                lineAllNetReady = false
                allComparableHaveNet = false
            }

            if (pLine.gross_amount != null) {
                linePurchaseGross.push(pLine.gross_amount)
                orderPurchaseGoodsGross.push(pLine.gross_amount)
                if (isGift) giftPurchaseGrossAmounts.push(pLine.gross_amount)
            }
            if (pLine.net_amount != null && pLine.tax_amount != null) {
                linePurchaseNet.push(pLine.net_amount)
                linePurchaseTax.push(pLine.tax_amount)
                orderPurchaseGoodsNet.push(pLine.net_amount)
                orderPurchaseGoodsTax.push(pLine.tax_amount)
            } else {
                lineAllNetReady = false
                allComparableHaveNet = false
            }

            if (!cmp.unitMismatch && pLine.quantity != null) {
                lineQuantities.push(pLine.quantity)
                try {
                    const exactSingleFull =
                        matched.length === 1 &&
                        salesLine.quantity != null &&
                        compareDecimal(
                            pLine.quantity,
                            salesLine.quantity,
                            6,
                        ) === 0
                    const sGross = exactSingleFull
                        ? normalizeFixed(salesLine.gross_amount, {
                              maxScale: 2,
                              outputScale: 2,
                          })
                        : multiplyFixed(
                              salesLine.unit_price_gross ??
                                  pLine.source?.unit_price_gross ??
                                  "0",
                              pLine.quantity,
                              {
                                  leftMaxScale: 6,
                                  rightMaxScale: 6,
                                  outputScale: 2,
                              },
                          )
                    const sSplit = exactSingleFull
                        ? {
                              net: normalizeFixed(salesLine.net_amount, {
                                  maxScale: 2,
                                  outputScale: 2,
                              }),
                              tax: normalizeFixed(salesLine.tax_amount, {
                                  maxScale: 2,
                                  outputScale: 2,
                              }),
                          }
                        : splitGrossByPercentRate(sGross, salesTaxRatePercent)

                    lineSalesGrossParts.push(sGross)
                    lineSalesNetParts.push(sSplit.net)
                    lineSalesTaxParts.push(sSplit.tax)

                    if (pLine.gross_amount != null) {
                        orderComparableSalesGross.push(sGross)
                        orderComparableCostGross.push(pLine.gross_amount)
                        if (
                            pLine.net_amount != null &&
                            pLine.tax_amount != null &&
                            pLine.input_tax_rate != null
                        ) {
                            orderComparableSalesNet.push(sSplit.net)
                            orderComparableSalesTax.push(sSplit.tax)
                            orderComparableCostNet.push(pLine.net_amount)
                            orderComparableCostTax.push(pLine.tax_amount)
                        }
                    }
                } catch {
                    lineAllNetReady = false
                    allComparableHaveNet = false
                }
            }

            matchedSummaries.push({
                id: pLine.id,
                lineNo: pLine.line_no,
                title: displayName(pLine.title) || "未记录商品名称",
                specification: purchaseSpecification(pLine.specification),
                quantity: pLine.quantity ? compactFixed(pLine.quantity) : null,
                unit: pLine.unit,
                unitName: pLine.unitName,
                unitCostGross: pLine.unit_cost_gross,
                grossAmount: pLine.gross_amount ?? null,
                netAmount: pLine.net_amount ?? null,
                taxAmount: pLine.tax_amount ?? null,
                inputTaxRatePercent: inputRatePercent,
                expectedDeliveryDate: pLine.expected_delivery_date,
                delayDays: cmp.delayDays,
                mismatchedProduct: cmp.mismatchedProduct,
                unitMismatch: cmp.unitMismatch,
            })
        }

        if (lineMaxTaxGap != null) {
            lowerInputTaxLineCount += 1
        }

        // 计算覆盖状态（基于同单位合并后的采购数量 vs 销售行数量）
        let coverageStatus: SalesLineCoverageStatus = "full"
        let purchasedQuantityTotal: string | null = null
        let excessQuantity: string | null = null
        let remainingQuantity: string | null = null

        if (
            lineHasUnitMismatch ||
            !salesLine.quantity ||
            !lineQuantities.length
        ) {
            coverageStatus = "mismatch"
            mismatchCount += 1
        } else {
            try {
                const totalQty = sumFixed(lineQuantities, {
                    maxScale: 6,
                    outputScale: 6,
                })
                purchasedQuantityTotal = compactFixed(totalQty)
                const cmpQty = compareDecimal(totalQty, salesLine.quantity, 6)
                if (cmpQty > 0) {
                    coverageStatus = "excess"
                    excessCount += 1
                    excessQuantity = compactFixed(
                        subtractFixed(totalQty, salesLine.quantity, {
                            maxScale: 6,
                            outputScale: 6,
                        }),
                    )
                } else if (cmpQty < 0) {
                    coverageStatus = "partial"
                    partialCount += 1
                    remainingQuantity = compactFixed(
                        subtractFixed(salesLine.quantity, totalQty, {
                            maxScale: 6,
                            outputScale: 6,
                        }),
                    )
                } else {
                    coverageStatus = "full"
                    fullCount += 1
                }
            } catch {
                coverageStatus = "mismatch"
                mismatchCount += 1
            }
        }

        const purchaseGrossTotal = linePurchaseGross.length
            ? sumFixed(linePurchaseGross, { maxScale: 2, outputScale: 2 })
            : null
        const purchaseNetTotal =
            lineAllNetReady && linePurchaseNet.length
                ? sumFixed(linePurchaseNet, { maxScale: 2, outputScale: 2 })
                : null
        const lineSalesGrossTotal = lineSalesGrossParts.length
            ? sumFixed(lineSalesGrossParts, { maxScale: 2, outputScale: 2 })
            : null
        const lineSalesNetTotal =
            lineAllNetReady && lineSalesNetParts.length
                ? sumFixed(lineSalesNetParts, { maxScale: 2, outputScale: 2 })
                : null

        const grossMargin =
            !isGift && lineSalesGrossTotal && purchaseGrossTotal
                ? calculateMargin(lineSalesGrossTotal, purchaseGrossTotal)
                : null
        const netMargin =
            !isGift && lineSalesNetTotal && purchaseNetTotal
                ? calculateMargin(lineSalesNetTotal, purchaseNetTotal)
                : null

        const taxInverted = Boolean(
            grossMargin && !grossMargin.negative && netMargin?.negative,
        )
        if (taxInverted) invertedTaxLineCount += 1

        const attentionTags: Array<{
            label: string
            tone: "destructive" | "warning" | "info"
        }> = []
        if (taxInverted) {
            attentionTags.push({ label: "对外税后倒挂", tone: "destructive" })
        } else if (grossMargin?.negative) {
            attentionTags.push({ label: "对外负毛利", tone: "destructive" })
        }
        if (coverageStatus === "excess") {
            attentionTags.push({ label: "超需求", tone: "destructive" })
        }
        if (lineHasProductMismatch) {
            attentionTags.push({ label: "品规不符", tone: "destructive" })
        }
        if (lineHasUnitMismatch) {
            attentionTags.push({ label: "单位不符", tone: "destructive" })
        }
        if (lineMaxTaxGap != null && !taxInverted) {
            attentionTags.push({
                label: `进项低 ${lineMaxTaxGap}%`,
                tone: "warning",
            })
        }
        if (lineMaxDelayDays > 0) {
            attentionTags.push({
                label: `晚 ${lineMaxDelayDays} 天`,
                tone: "warning",
            })
        }
        if (lineHasCostAboveSupply) {
            attentionTags.push({ label: "采购倒挂", tone: "warning" })
        }
        if (
            salesPriceCheck.adjustment === "increased" ||
            salesPriceCheck.adjustment === "decreased"
        ) {
            attentionTags.push({
                label:
                    salesPriceCheck.adjustment === "increased"
                        ? "销售调高供给价"
                        : "销售调低供给价",
                tone: "info",
            })
        }

        const hasSevere = attentionTags.some(
            (tag) => tag.tone === "destructive",
        )
        const hasWarning = attentionTags.some((tag) => tag.tone === "warning")
        const hasInfo = attentionTags.some((tag) => tag.tone === "info")
        const severity = hasSevere
            ? "severe"
            : hasWarning
              ? "warning"
              : hasInfo
                ? "info"
                : "none"

        const coverageRatioPercent =
            coverageStatus === "full" || coverageStatus === "excess"
                ? "100"
                : coverageStatus === "partial" &&
                    purchasedQuantityTotal &&
                    salesLine.quantity
                  ? safeClampedRatioPercent(
                        purchasedQuantityTotal,
                        salesLine.quantity,
                        6,
                    )
                  : null

        coverageRows.push({
            salesLineId: salesLine.id,
            salesLineNo: salesLine.line_no,
            title:
                displayName(salesLine.item_name_snapshot) || "未记录商品名称",
            specification: purchaseSpecification(salesLine.spec_snapshot),
            unitCode: salesLine.base_unit_code ?? null,
            unitLabel: salesUnitLabel,
            isGift,
            welfareLabel: salesLine.welfare_scenario
                ? welfareScenarioLabel(salesLine.welfare_scenario)
                : null,
            salesQuantity: salesLine.quantity
                ? compactFixed(salesLine.quantity)
                : salesLine.card_count != null
                  ? String(salesLine.card_count)
                  : null,
            salesUnitPriceGross: salesLine.unit_price_gross ?? null,
            salesGrossAmount: salesLine.gross_amount,
            salesNetAmount: salesLine.net_amount,
            salesTaxRatePercent,
            salesDueDate,
            priceBasisLabel,
            originalSupplyPrice:
                salesLine.original_procurement_supply_price_gross ?? null,
            supplyPrice: salesLine.procurement_supply_price_gross ?? null,
            supplyAdjustment: salesPriceCheck.adjustment,
            coverageStatus,
            coverageRatioPercent,
            matchedPurchaseLines: matchedSummaries,
            purchasedQuantityTotal,
            excessQuantity,
            remainingQuantity,
            purchaseGrossTotal,
            purchaseMargin: sumPurchaseMargins(linePurchaseMargins),
            purchaseNetTotal,
            inputTaxRatesPercent: uniqueSortedRates(lineInputRatesPercent),
            lowerInputTaxGapPoints: lineMaxTaxGap,
            grossMargin,
            netMargin,
            taxInverted,
            costAboveSupply: lineHasCostAboveSupply,
            maxDelayDays: lineMaxDelayDays,
            mismatchedProduct: lineHasProductMismatch,
            unitMismatch: lineHasUnitMismatch,
            hasAttention: attentionTags.length > 0,
            severity,
            attentionTags,
            duplicateGroupSize,
        })
    }

    // 3. 费用行与兜底未匹配采购行
    const includeFeesInThisOrder = mergedOrderCount <= 1
    const feeGrossAmounts: string[] = []
    const feeNetAmounts: string[] = []
    const feeTaxAmounts: string[] = []
    let allFeesHaveNet = true
    const extraPurchaseLines: ExtraPurchaseLineItem[] = []

    for (const feeLine of feePurchaseLines) {
        const inputRatePercent = formatRatePercent(feeLine.input_tax_rate)
        if (isExpenseOrder && inputRatePercent != null) {
            matchedInputTaxRates.push(inputRatePercent)
            if (compareDecimal(inputRatePercent, "0", 2) === 0) {
                zeroInputTaxCount += 1
            }
        }
        if (feeLine.gross_amount != null) {
            feeGrossAmounts.push(feeLine.gross_amount)
        }
        if (
            feeLine.net_amount != null &&
            feeLine.tax_amount != null &&
            feeLine.input_tax_rate != null
        ) {
            feeNetAmounts.push(feeLine.net_amount)
            feeTaxAmounts.push(feeLine.tax_amount)
        } else {
            allFeesHaveNet = false
        }
        extraPurchaseLines.push({
            id: feeLine.id,
            lineNo: feeLine.line_no,
            title: displayName(feeLine.title) || "采购费用",
            grossAmount: feeLine.gross_amount ?? null,
            netAmount: feeLine.net_amount ?? null,
            inputTaxRatePercent: inputRatePercent,
            isFee: true,
            ratioOfSalesPercent: feeLine.gross_amount
                ? safeRatioPercent(feeLine.gross_amount, revision.gross_amount)
                : null,
        })
    }

    for (const orphanLine of unmatchedPurchaseLines) {
        extraPurchaseLines.push({
            id: orphanLine.id,
            lineNo: orphanLine.line_no,
            title: displayName(orphanLine.title) || "未匹配商品",
            grossAmount: orphanLine.gross_amount ?? null,
            netAmount: orphanLine.net_amount ?? null,
            inputTaxRatePercent: formatRatePercent(orphanLine.input_tax_rate),
            isFee: false,
            ratioOfSalesPercent: orphanLine.gross_amount
                ? safeRatioPercent(
                      orphanLine.gross_amount,
                      revision.gross_amount,
                  )
                : null,
        })
    }

    const purchaseFeeGrossTotal = feeGrossAmounts.length
        ? sumFixed(feeGrossAmounts, { maxScale: 2, outputScale: 2 })
        : null
    const purchaseGoodsGrossTotal = orderPurchaseGoodsGross.length
        ? sumFixed(orderPurchaseGoodsGross, { maxScale: 2, outputScale: 2 })
        : null
    const purchaseGoodsNetTotal =
        orderPurchaseGoodsNet.length &&
        orderPurchaseGoodsNet.length === orderPurchaseGoodsGross.length
            ? sumFixed(orderPurchaseGoodsNet, { maxScale: 2, outputScale: 2 })
            : null

    const correspondingSalesGrossTotal = orderComparableSalesGross.length
        ? sumFixed(orderComparableSalesGross, { maxScale: 2, outputScale: 2 })
        : null
    const correspondingSalesNetTotal =
        allComparableHaveNet && orderComparableSalesNet.length
            ? sumFixed(orderComparableSalesNet, { maxScale: 2, outputScale: 2 })
            : null
    const correspondingSalesTaxTotal =
        allComparableHaveNet && orderComparableSalesTax.length
            ? sumFixed(orderComparableSalesTax, { maxScale: 2, outputScale: 2 })
            : null

    const totalComparableCostGross = orderComparableCostGross.length
        ? sumFixed(
              includeFeesInThisOrder
                  ? [...orderComparableCostGross, ...feeGrossAmounts]
                  : orderComparableCostGross,
              { maxScale: 2, outputScale: 2 },
          )
        : null
    const netScopeReady =
        allComparableHaveNet &&
        orderComparableCostNet.length > 0 &&
        (!includeFeesInThisOrder || allFeesHaveNet)
    const totalComparableCostNet = netScopeReady
        ? sumFixed(
              includeFeesInThisOrder
                  ? [...orderComparableCostNet, ...feeNetAmounts]
                  : orderComparableCostNet,
              { maxScale: 2, outputScale: 2 },
          )
        : null
    const totalComparableCostTax = netScopeReady
        ? sumFixed(
              includeFeesInThisOrder
                  ? [...orderComparableCostTax, ...feeTaxAmounts]
                  : orderComparableCostTax,
              { maxScale: 2, outputScale: 2 },
          )
        : null

    const grossMargin =
        correspondingSalesGrossTotal && totalComparableCostGross
            ? calculateMargin(
                  correspondingSalesGrossTotal,
                  totalComparableCostGross,
              )
            : null
    const netMargin =
        correspondingSalesNetTotal && totalComparableCostNet
            ? calculateMargin(
                  correspondingSalesNetTotal,
                  totalComparableCostNet,
              )
            : null
    const vatGap =
        correspondingSalesTaxTotal && totalComparableCostTax
            ? subtractFixed(
                  correspondingSalesTaxTotal,
                  totalComparableCostTax,
                  { maxScale: 2, outputScale: 2 },
              )
            : null
    const taxDragPoints =
        grossMargin && netMargin
            ? signedSubtract(grossMargin.percent, netMargin.percent, 1)
            : null
    const orderTaxInverted = Boolean(
        grossMargin && !grossMargin.negative && netMargin?.negative,
    )

    const salesCoveragePercent = correspondingSalesGrossTotal
        ? safeRatioPercent(correspondingSalesGrossTotal, revision.gross_amount)
        : null

    const comparableGoodsCostGross = orderComparableCostGross.length
        ? sumFixed(orderComparableCostGross, { maxScale: 2, outputScale: 2 })
        : null
    const costBarPercent =
        correspondingSalesGrossTotal && totalComparableCostGross
            ? safeClampedRatioPercent(
                  totalComparableCostGross,
                  correspondingSalesGrossTotal,
                  2,
              )
            : null
    const goodsCostBarPercent =
        correspondingSalesGrossTotal && comparableGoodsCostGross
            ? safeClampedRatioPercent(
                  comparableGoodsCostGross,
                  correspondingSalesGrossTotal,
                  2,
              )
            : null
    const feeCostBarPercent = (() => {
        if (
            !includeFeesInThisOrder ||
            !purchaseFeeGrossTotal ||
            !costBarPercent ||
            !goodsCostBarPercent
        ) {
            return null
        }
        try {
            if (compareDecimal(costBarPercent, goodsCostBarPercent, 1) <= 0) {
                return null
            }
            return clampPercent(
                subtractFixed(costBarPercent, goodsCostBarPercent, {
                    maxScale: 1,
                    outputScale: 1,
                }),
            )
        } catch {
            return null
        }
    })()
    const expenseFeeBarPercent =
        includeFeesInThisOrder && purchaseFeeGrossTotal
            ? safeClampedRatioPercent(
                  purchaseFeeGrossTotal,
                  revision.gross_amount,
                  2,
              )
            : null

    const giftPurchaseGrossTotal = giftPurchaseGrossAmounts.length
        ? sumFixed(giftPurchaseGrossAmounts, { maxScale: 2, outputScale: 2 })
        : null

    const resolvedExtraCost =
        extraCostSummary &&
        includeFeesInThisOrder &&
        (!snapshot?.document_id ||
            extraCostSummary.sales_order_id === snapshot.document_id)
            ? (() => {
                  const parts = [
                      extraCostSummary.occurred_gross_total,
                      extraCostSummary.pending_gross_total,
                      ...(purchaseFeeGrossTotal ? [purchaseFeeGrossTotal] : []),
                  ]
                  const combined = sumFixed(parts, {
                      maxScale: 2,
                      outputScale: 2,
                  })
                  return {
                      occurredGrossTotal: extraCostSummary.occurred_gross_total,
                      pendingGrossTotal: extraCostSummary.pending_gross_total,
                      combinedWithCurrentFeeTotal: combined,
                      combinedRatioPercent: safeRatioPercent(
                          combined,
                          revision.gross_amount,
                      ),
                  }
              })()
            : null

    // 4. 构造「商务、资金与票税对照」（问题 1）
    const paymentCheck = buildPaymentCheck(revision, purchasePaymentLabel)

    const invoiceTypeText = revision.invoice_type?.trim() || "未记录开票类型"
    const compactInvoiceType = shortInvoiceType(invoiceTypeText)
    const allSalesRates = salesLines
        .map((line) => formatRatePercent(line.sales_tax_rate))
        .filter((rate): rate is string => rate != null)
    const displaySalesRates = matchedSalesTaxRates.length
        ? matchedSalesTaxRates
        : allSalesRates
    const salesTaxDisplay = displaySalesRates.length
        ? `${invoiceTypeText} · 销项 ${formatRateList(displaySalesRates)}`
        : `${invoiceTypeText} · 税点 ${approvalSalesTaxPoint(revision.tax_point)}`
    const compactSalesTaxDisplay = displaySalesRates.length
        ? [compactInvoiceType, formatRateList(displaySalesRates)]
              .filter(Boolean)
              .join(" ")
        : [compactInvoiceType, approvalSalesTaxPoint(revision.tax_point)]
              .filter(Boolean)
              .join(" ")
    const purchaseTaxDisplay = matchedInputTaxRates.length
        ? `进项 ${formatRateList(matchedInputTaxRates)}`
        : "未记录进项税率"
    const compactPurchaseTaxDisplay = matchedInputTaxRates.length
        ? formatRateList(matchedInputTaxRates)
        : "未填"
    const requiresSpecialInvoice =
        invoiceTypeText.includes("专票") || invoiceTypeText.includes("专用")

    let taxCheck: CommercialComparisonItem
    const taxBase = {
        id: "tax" as const,
        title: "票税与对外毛利",
        shortTitle: "票税",
        salesLabel: "销售开票",
        salesValue: salesTaxDisplay,
        purchaseLabel: "采购进项",
        purchaseValue: purchaseTaxDisplay,
        compactSalesValue: compactSalesTaxDisplay || "未记录",
        compactPurchaseValue: compactPurchaseTaxDisplay,
        direction: "neutral" as const,
        metric: null as string | null,
    }
    if (orderTaxInverted && grossMargin && netMargin) {
        taxCheck = {
            ...taxBase,
            direction: "down",
            metric: `含税 ${grossMargin.percent}% → 税后 -${netMargin.absPercent}%（亏 ¥${netMargin.absAmount}）`,
            status: "severe",
            badge: "税后倒挂",
            detail: `对外含税毛利率 ${grossMargin.percent}%，扣除进销项税差后，对外不含税亏损 ¥${netMargin.absAmount}（-${netMargin.absPercent}%）。`,
        }
    } else if (invertedTaxLineCount > 0) {
        taxCheck = {
            ...taxBase,
            direction: "down",
            metric: `${invertedTaxLineCount} 项税后亏损`,
            status: "severe",
            badge: "明细税后倒挂",
            detail: `有 ${invertedTaxLineCount} 项商品含税看盈利，扣除进销项税差后不含税实际亏损，请重点核对下方明细。`,
        }
    } else if (lowerInputTaxLineCount > 0) {
        const dragHint =
            grossMargin && netMargin
                ? `；对外毛利率由含税 ${grossMargin.percent}% 降至不含税 ${netMargin.percent}%`
                : ""
        taxCheck = {
            ...taxBase,
            direction: "down",
            metric:
                grossMargin && netMargin
                    ? `${lowerInputTaxLineCount} 项偏低 · 毛利 ${grossMargin.percent}% → ${netMargin.percent}%`
                    : `${lowerInputTaxLineCount} 项进项偏低`,
            status: "warning",
            badge: "进项税率偏低",
            detail: `有 ${lowerInputTaxLineCount} 项采购进项税率低于销售销项税率${dragHint}。`,
        }
    } else if (
        requiresSpecialInvoice &&
        (zeroInputTaxCount > 0 || (missingInputTaxCount > 0 && !isExpenseOrder))
    ) {
        taxCheck = {
            ...taxBase,
            direction: "down",
            metric:
                zeroInputTaxCount > 0
                    ? `${zeroInputTaxCount} 项 0% 进项`
                    : `${missingInputTaxCount} 项未填税率`,
            status: "warning",
            badge: "核对专票抵扣",
            detail: "销售要求开具增值税专用发票，但采购存在 0% 或未填写进项税率明细，请核对能否取得可抵扣专票。",
        }
    } else if (missingInputTaxCount > 0 && !isExpenseOrder) {
        taxCheck = {
            ...taxBase,
            metric: `${missingInputTaxCount} 项未填税率`,
            status: "unknown",
            badge: "税率待核对",
            detail: `有 ${missingInputTaxCount} 项采购明细未记录进项税率，暂无法完整核算对外不含税毛利。`,
        }
    } else if (comparableTaxLineCount > 0) {
        taxCheck = {
            ...taxBase,
            metric:
                grossMargin && netMargin
                    ? `税后毛利 ${netMargin.percent}%`
                    : null,
            status: "ok",
            badge: "税率匹配",
            detail:
                grossMargin && netMargin
                    ? `采购进项不低于销项税率，对外不含税毛利率 ${netMargin.percent}%。`
                    : "采购进项税率不低于对应销售销项税率。",
        }
    } else {
        taxCheck = {
            ...taxBase,
            status: "info",
            badge: isExpenseOrder ? "费用进项" : "待核对",
            detail: isExpenseOrder
                ? "费用采购单请结合销售开票要求核对费用进项税抵扣。"
                : "暂无可比对的进销项商品明细。",
        }
    }

    const orderUniquePoints = countUniqueDeliveryPoints(
        (isExpenseOrder ? allPurchaseLines : goodsPurchaseLines).map(
            (line) => line.delivery_allocations,
        ),
    )
    const rawFulfillmentSummary = formatPurchaseFulfillmentSummary(
        purchaseFulfillment,
        orderUniquePoints,
    )
    const fulfillmentSummary = rawFulfillmentSummary ?? "未记录履约方式"

    const hasBulk = matchedBases.has("BULK")
    const hasDropship = matchedBases.has("DROPSHIP")
    const salesBasisText = isExpenseOrder
        ? "来源销售附加费用"
        : hasBulk && hasDropship
          ? "集采和一件代发"
          : hasBulk
            ? "按集采价取价"
            : hasDropship
              ? "按一件代发价取价"
              : hasSupplyWithoutBasis
                ? "已记录供给价，取价方式未记录"
                : "未记录采购供给价"
    const compactSalesBasisText = isExpenseOrder
        ? "附加费用"
        : hasBulk && hasDropship
          ? "集采 / 代发"
          : hasBulk
            ? "集采价"
            : hasDropship
              ? "一件代发"
              : hasSupplyWithoutBasis
                ? "未标取价"
                : "无供给价"

    const fulfillmentBase = {
        id: "fulfillment" as const,
        title: "履约与采购供给取价",
        shortTitle: "履约",
        salesLabel: "采购供给取价",
        salesValue: salesBasisText,
        purchaseLabel: "采购履约",
        purchaseValue: fulfillmentSummary,
        compactSalesValue: compactSalesBasisText,
        compactPurchaseValue: rawFulfillmentSummary ?? "未记录",
        direction: "neutral" as const,
        metric: null as string | null,
    }
    const supplyAdjSuffix =
        adjustedSupplyLineCount > 0
            ? `。另有 ${adjustedSupplyLineCount} 项采购供给价经过调整`
            : ""

    let fulfillmentCheck: CommercialComparisonItem
    if (hasBulk && purchaseFulfillment?.responsibility === "SUPPLIER_DIRECT") {
        fulfillmentCheck = {
            ...fulfillmentBase,
            direction: "down",
            metric: [
                orderUniquePoints > 1
                    ? `${orderUniquePoints} 处直发`
                    : "集采改直发",
                adjustedSupplyLineCount > 0
                    ? `${adjustedSupplyLineCount} 项调价`
                    : null,
            ]
                .filter(Boolean)
                .join(" · "),
            status: "warning",
            badge: "取价与履约不符",
            detail: `有商品按集采价确定采购供给价，实际由供应商直发${orderUniquePoints > 1 ? `（${orderUniquePoints} 处收货点）` : ""}，请核对直发成本和运费${supplyAdjSuffix}。`,
        }
    } else if (
        hasDropship &&
        purchaseFulfillment?.responsibility === "WAREHOUSE"
    ) {
        fulfillmentCheck = {
            ...fulfillmentBase,
            metric:
                adjustedSupplyLineCount > 0
                    ? `${adjustedSupplyLineCount} 项调价`
                    : null,
            status: "info",
            badge: "改走入仓",
            detail: `有商品按一件代发价确定采购供给价，实际采购入库，请核对仓配成本${supplyAdjSuffix}。`,
        }
    } else if (adjustedSupplyLineCount > 0) {
        fulfillmentCheck = {
            ...fulfillmentBase,
            direction: "down",
            metric: `${adjustedSupplyLineCount} 项供给价已调`,
            status: "warning",
            badge: "供给价已调整",
            detail: `有 ${adjustedSupplyLineCount} 项采购供给价与原参考价不同。采购毛利按销售单保存的采购供给价核算。`,
        }
    } else if (!purchaseFulfillment) {
        fulfillmentCheck = {
            ...fulfillmentBase,
            status: "unknown",
            badge: "待核对",
            detail: "本次提交未记录履约方式。",
        }
    } else {
        fulfillmentCheck = {
            ...fulfillmentBase,
            status: "ok",
            badge: "口径一致",
            detail: isExpenseOrder
                ? "本单为费用采购单，请结合整单销售金额评估费用占比。"
                : "采购履约方式与采购供给取价一致。",
        }
    }

    const commercialChecks: CommercialComparisonItem[] = [
        paymentCheck,
        taxCheck,
        fulfillmentCheck,
    ]

    const commercialAttentionCount = commercialChecks.filter(
        (item) => item.status === "severe" || item.status === "warning",
    ).length
    const lineAttentionCount = coverageRows.filter(
        (row) => row.severity === "severe" || row.severity === "warning",
    ).length
    const hasSevereAttention =
        commercialChecks.some((item) => item.status === "severe") ||
        coverageRows.some((row) => row.severity === "severe")

    return {
        revisionId,
        documentNo,
        revisionNo,
        customerName,
        projectName: displayName(revision.project_name) || null,
        settlementPartyName:
            displayName(revision.settlement_party_name) || null,
        contractNo: revision.contract_no?.trim() || null,
        contractNotices,
        businessRemark: revision.business_remark?.trim() || null,
        hasFullOrder: true,
        commercialChecks,
        ledger: {
            isExpenseOrder,
            salesGrossTotal: revision.gross_amount,
            salesNetTotal: revision.net_amount,
            salesTaxTotal: revision.tax_amount,
            totalSalesLineCount: salesLines.length,
            giftSalesLineCount,
            fullCount,
            partialCount,
            excessCount,
            mismatchCount,
            uncoveredCount,
            coveredLineCount: salesLines.length - uncoveredCount,
            purchaseGoodsGrossTotal,
            purchaseGoodsLineCount: goodsPurchaseLines.length,
            purchaseMargin: sumPurchaseMargins(orderPurchaseMargins),
            purchaseGoodsNetTotal,
            purchaseFeeGrossTotal: includeFeesInThisOrder
                ? purchaseFeeGrossTotal
                : null,
            purchaseFeeRatioPercent:
                includeFeesInThisOrder && purchaseFeeGrossTotal
                    ? safeRatioPercent(
                          purchaseFeeGrossTotal,
                          revision.gross_amount,
                      )
                    : null,
            unallocatedSharedFeeTotal: !includeFeesInThisOrder
                ? purchaseFeeGrossTotal
                : null,
            correspondingSalesGrossTotal,
            correspondingSalesNetTotal,
            salesCoveragePercent,
            costBarPercent,
            goodsCostBarPercent,
            feeCostBarPercent,
            expenseFeeBarPercent,
            grossMargin,
            netMargin,
            vatGap,
            taxDragPoints,
            taxInverted: orderTaxInverted,
            giftCoveredCount,
            giftPurchaseGrossTotal,
            isFullOrderCovered:
                !isExpenseOrder &&
                salesLines.length > 0 &&
                uncoveredCount === 0 &&
                partialCount === 0 &&
                mismatchCount === 0,
            extraCostSummary: resolvedExtraCost,
        },
        coverageRows,
        extraPurchaseLines,
        attentionCount: commercialAttentionCount + lineAttentionCount,
        hasSevereAttention,
        mergedOrderCount,
    }
}

/**
 * 构建采购审批「采销对照」模型（纯基于本次审批冻结数据）：
 * 1. 商务、资金（垫资/账期）、票税（进销项税差与税后不含税毛利）及履约口径对照；
 * 2. 整张来源销售单的全局覆盖进度、含税/不含税双口径毛利、赠品与未含明细清单。
 */
export function buildPurchaseSourceComparisonModel({
    orders,
    snapshots,
    purchaseLines,
    purchaseFulfillment,
    purchaseExtraSections,
    extraCostSummary,
    unitNames,
}: PurchaseSourceComparisonModelInput): PurchaseSourceComparisonModel {
    const lines = purchaseLines ?? []
    const purchasePaymentLabel =
        purchaseExtraSections
            ?.find((section) => section.label === "付款条件")
            ?.value.trim() || null

    const orderByRevisionId = new Map<string, SourceSalesOrderEntry>()
    for (const order of orders) {
        orderByRevisionId.set(order.revision.id, order)
    }

    const snapshotByRevisionId = new Map<string, SourceSalesSnapshotEntry>()
    for (const snapshot of snapshots ?? []) {
        snapshotByRevisionId.set(snapshot.revision_id, snapshot)
    }

    const revisionIds: string[] = []
    for (const snapshot of snapshots ?? []) {
        if (!revisionIds.includes(snapshot.revision_id)) {
            revisionIds.push(snapshot.revision_id)
        }
    }
    for (const order of orders) {
        if (!revisionIds.includes(order.revision.id)) {
            revisionIds.push(order.revision.id)
        }
    }

    const mergedOrderCount = revisionIds.length
    const builtOrders = revisionIds.map((revisionId) =>
        buildOrderComparison({
            order: orderByRevisionId.get(revisionId),
            snapshot: snapshotByRevisionId.get(revisionId),
            allPurchaseLines: lines,
            mergedOrderCount,
            purchaseFulfillment,
            purchasePaymentLabel,
            extraCostSummary,
            unitNames,
        }),
    )

    const totalAttentionCount = builtOrders.reduce(
        (sum, entry) => sum + entry.attentionCount,
        0,
    )
    const hasSevereAttention = builtOrders.some(
        (entry) => entry.hasSevereAttention,
    )

    return {
        orders: builtOrders,
        totalAttentionCount,
        hasSevereAttention,
    }
}
