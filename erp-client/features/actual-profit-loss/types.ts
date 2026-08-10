/** W16 实际经营盈亏 · 客户端契约类型（与 docs/ui-workspaces/w16 §8 对齐） */

import type { StatusTone } from "@/components/ui/status-badge"

export type ProfitLossCoverage = "covered" | "uncovered" | "all"

export type ProfitLossDimension =
    | "sales_order"
    | "customer"
    | "scenario"
    | "fulfillment"
    | "cost_type"

export type CostStage = "EXPECTED" | "CONFIRMED" | "ACTUAL" | "REDUCTION"
type CostScope = "NON_VOUCHER_FULFILLMENT"

export type CoverageState = "COVERED" | "PARTIAL" | "UNCOVERED"

export type ProjectionFreshnessState =
    | "fresh"
    | "stale"
    | "rebuilding"
    | "failed"

type Reliability = "reliable" | "partial" | "unavailable"

type ProfitLossPeriodBasisOption = Readonly<{
    code: string
    label: string
    explanation: string
}>

export type ProfitLossPeriodBasisConfig = Readonly<{
    /** 服务端已固化的正式口径；未配置时为 undefined */
    configuredPeriodBasis?: string
    allowedPeriodBases: readonly ProfitLossPeriodBasisOption[]
    configurationVersion: string
}>

export type ProfitLossQuery = Readonly<{
    from: string
    to: string
    periodBasis: string
    scopeId: string
    coverage: ProfitLossCoverage
    customerId?: string
    salesOrderId?: string
    benefitScenario?: string
    fulfillmentModes?: readonly string[]
    costTypes?: readonly string[]
    dimension: ProfitLossDimension
    q?: string
    sort: string
    pageSize: number
}>

type CoverageBlocker = Readonly<{
    code: string
    message: string
}>

export type ProfitLossRow = Readonly<{
    rowId: string
    objectType: string
    objectId?: string
    identityLabel: string
    customerId?: string
    customerLabel?: string
    benefitScenarios?: readonly string[]
    fulfillmentModes?: readonly string[]
    netSalesRevenue: string
    actualProcurementCostNet?: string
    actualFulfillmentCostNet?: string
    reductionsNet?: string
    actualProfitLossNet?: string
    marginRate?: string
    marginUnavailableReason?: string
    coverageState: CoverageState
    coverageBlockers: readonly CoverageBlocker[]
    latestCostOccurredAt?: string
    allowedDrilldowns: readonly string[]
    /** 关联成本记录 id，供 detail 下钻 */
    costEntryIds: readonly string[]
}>

type ProfitLossTrendPoint = Readonly<{
    period: string
    netSalesRevenue: string
    actualCostNet: string
    actualProfitLossNet?: string
    reliability: Reliability
}>

type ProfitLossCostComposition = Readonly<{
    costType: string
    label: string
    netAmount: string
    /** 仅当有成本字段权限时返回；无权时不返回以免图表比例泄露 */
    share?: string
}>

type StageReferenceLine = Readonly<{
    stage: "EXPECTED" | "CONFIRMED"
    label: string
    procurementCostNet: string
    fulfillmentCostNet: string
    totalNet: string
    note: string
}>

export type ProfitLossView = Readonly<{
    scope: {
        id: string
        label: string
        permissionVersion: string
    }
    period: {
        from: string
        to: string
        basis: string
        basisLabel: string
        timezone: string
    }
    businessType: "GOODS_SERVICE"
    amountBasis: "NET"
    amountBasisLabel: string
    businessTypeLabel: string
    formulaVersion: string
    formulaText: string
    freshness: {
        projectedAt: string
        sourceWatermark: string
        state: ProjectionFreshnessState
    }
    coverage: {
        coveredNetRevenue: string
        uncoveredNetRevenue: string
        coverageRate: string
        reliability: Reliability
        coverageState: "complete" | "partial" | "none"
    }
    totals: {
        netSalesRevenue: string
        actualProcurementCostNet?: string
        actualFulfillmentCostNet?: string
        reductionsNet?: string
        actualProfitLossNet?: string
        marginRate?: string
        marginUnavailableReason?: string
    }
    fieldPermissions: {
        canViewRevenue: boolean
        canViewCost: boolean
        canViewProfit: boolean
        canExport: boolean
    }
    trend: readonly ProfitLossTrendPoint[]
    costComposition: readonly ProfitLossCostComposition[]
    stageReference: readonly StageReferenceLine[]
    rows: {
        dimension: ProfitLossDimension
        items: readonly ProfitLossRow[]
        total: number
    }
    filterSummary: string
    excludedNote: string
    correctionPendingNotice?: string
}>

export type CostEntryDetail = Readonly<{
    costEntryId: string
    costType: string
    costTypeLabel: string
    stage: CostStage
    stageLabel: string
    costScope: CostScope
    costScopeLabel: string
    supplierId?: string
    supplierName?: string
    amountGross: string
    taxRate: string
    taxAmount: string
    amountNet: string
    occurredAt: string
    sourceType: string
    sourceTypeLabel: string
    sourceDocumentId: string
    sourceDocumentNo: string
    sourceLineId?: string
    sourceLineLabel?: string
    sourceVersion: string
    salesOrderId: string
    salesOrderNo: string
    salesOrderLineId?: string
    salesOrderLineLabel?: string
    originalCostEntryId?: string
    originalCostEntryLabel?: string
    voucherSummary?: string
    /** 纠错来源路由（只读跳转，W16 不执行变更） */
    correctionHref?: string
    correctionLabel?: string
}>

export type ProfitLossExportJob = Readonly<{
    jobId: string
    status: "queued" | "running" | "succeeded" | "failed"
    total: number
    completed: number
    createdAt: string
    downloadLabel?: string
    /** 导出水印/冻结元数据 */
    watermark: {
        periodFrom: string
        periodTo: string
        periodBasis: string
        formulaVersion: string
        coverage: ProfitLossCoverage
        scopeId: string
        scopeLabel: string
        permissionVersion: string
        projectedAt: string
        sourceWatermark: string
        amountBasis: "NET"
        businessType: "GOODS_SERVICE"
        rowCount: number
    }
}>

export type PeriodPreset = "month-to-date" | "last-month" | "quarter-to-date"

export const DIMENSION_LABEL: Record<ProfitLossDimension, string> = {
    sales_order: "销售单",
    customer: "客户",
    scenario: "福利场景",
    fulfillment: "履约方式",
    cost_type: "成本类型",
}

export const COVERAGE_FILTER_LABEL: Record<ProfitLossCoverage, string> = {
    covered: "成本完整",
    uncovered: "未覆盖",
    all: "全部覆盖状态",
}

export const COVERAGE_STATE_UI: Record<
    CoverageState,
    { label: string; tone: StatusTone }
> = {
    COVERED: { label: "完整", tone: "success" },
    PARTIAL: { label: "部分", tone: "warning" },
    UNCOVERED: { label: "未覆盖", tone: "destructive" },
}
