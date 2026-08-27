/**
 * W15 客户经营质量 — 只读分析数据契约。
 * 前端仅格式化展示；金额/标签/覆盖率一律服务端投影字段。
 */

export type FundsReviewFilter = "all" | "reviewed_only"
export type BusinessTypeFilter = "VOUCHER" | "GOODS_SERVICE"
export type PeriodSelectionSource =
    | "SERVER_DEFAULT"
    | "CONFIGURED_PRESET"
    | "EXPLICIT"

type ProjectionFreshnessState = "fresh" | "stale" | "rebuilding" | "failed"

type MetricReliability = "reliable" | "partial" | "unavailable"
type AmountBasis = "GROSS" | "NET"

export type CustomerQualityScenario =
    | "default"
    | "no_period_default"
    | "empty"
    | "no_scope"
    | "forbidden"
    | "field_denied"
    | "stale"
    | "rebuilding"
    | "failed"
    | "refresh_failed"

type TagType = "scale" | "profit" | "risk"

export type CustomerQualityPeriodPolicy = Readonly<{
    hasDefault: boolean
    from?: string
    to?: string
    periodBasis?: string
    timezone: string
    customerQualityPeriodPolicyId?: string
    customerQualityPeriodPolicyVersion?: number
    selectionSource?: Exclude<PeriodSelectionSource, "EXPLICIT">
    presets?: readonly {
        id: string
        label: string
        from: string
        to: string
    }[]
}>

export type CustomerQualityQuery = Readonly<{
    from: string
    to: string
    periodBasis: string
    periodSelectionSource: PeriodSelectionSource
    customerQualityPeriodPolicyId?: string
    customerQualityPeriodPolicyVersion?: number
    scopeId: string
    fundsReview: FundsReviewFilter
    businessType?: BusinessTypeFilter
    benefitScenario?: string
    scaleTag?: string
    profitTag?: string
    riskTag?: string
    q?: string
    sort: string
    /** 服务端分页：page 从 1 开始 */
    page: number
    pageSize: number
    /** 图表选中维度（规模分层 / 利润贡献等） */
    chartDimension?: string
    chartCode?: string
    customerId?: string
    scenario?: CustomerQualityScenario
}>

type CustomerQualityMetric = Readonly<{
    key: string
    label: string
    value: string
    amountBasis?: AmountBasis
    visible: boolean
    reliability: MetricReliability
    explanation?: string
    /** 字段无权时位置保留 */
    fieldDenied?: boolean
}>

type DimensionItem = Readonly<{
    code: string
    label: string
    value: string
    share?: string
    count?: number
}>

type CustomerQualityDimension = Readonly<{
    key: string
    title: string
    ruleVersion?: string
    ruleExplanation?: string
    items: readonly DimensionItem[]
}>

export type BusinessTag = Readonly<{
    type: TagType
    code: string
    label: string
    tone: "success" | "info" | "warning" | "destructive" | "neutral"
    ruleVersion: string
    explanation: string
}>

export type CustomerQualityRow = Readonly<{
    customerId: string
    customerNo: string
    customerName: string
    ownerLabels: readonly string[]
    tags: readonly BusinessTag[]
    salesGrossAmount: string
    salesOrderCount: number
    voucherShare: string
    nonVoucherShare: string
    costCoveredNetRevenue: string | null
    costUncoveredNetRevenue: string | null
    costCoverageRate: string | null
    actualProfitLossNet: string | null
    marginRate: string | null
    receivableOpenGross: string | null
    overdueGross: string | null
    averageCollectionDays: string | null
    exceptionCounts: Readonly<Record<string, number>>
    firstBusinessAt?: string
    latestBusinessAt?: string
    scaleTierCode: string
    profitTierCode: string
    riskTierCode: string
    cardFundsReviewInsufficient: boolean
    allowedDrilldowns: readonly ("W03" | "W11" | "W16" | "W05")[]
}>

export type CustomerQualityView = Readonly<{
    scope: {
        id: string
        label: string
        permissionVersion: string
    }
    period: {
        from: string
        to: string
        basis: string
        timezone: string
        selectionSource: PeriodSelectionSource
        customerQualityPeriodPolicyId?: string
        customerQualityPeriodPolicyVersion?: number
    }
    freshness: {
        projectedAt: string
        sourceWatermark: string
        state: ProjectionFreshnessState
        refreshFailed?: boolean
    }
    coverage: {
        cardFundsReviewRate: string
        cardFundsReviewPercent: number
        reviewedVoucherOrderCount: number
        requiredVoucherOrderCount: number
        cardFundsState: "complete" | "partial" | "none"
        costCoveredNetRevenue: string
        costUncoveredNetRevenue: string
        costCoverageRate: string
        costCoveragePercent: number
        costCoverageState: "complete" | "partial" | "none"
        costBasis: "ACTUAL" | "STANDARD" | "NONE"
    }
    metrics: readonly CustomerQualityMetric[]
    dimensions: readonly CustomerQualityDimension[]
    customers: {
        items: readonly CustomerQualityRow[]
        total: number
        filteredTotal: number
    }
    filterSummary: string
    canExport: boolean
    emptyKind?: "no-data" | "filter" | "no-scope" | "forbidden"
    tagRuleCatalog: Readonly<
        Record<
            TagType,
            {
                ruleVersion: string
                explanation: string
                labels: Readonly<Record<string, string>>
            }
        >
    >
}>

export type CustomerQualityExportJob = Readonly<{
    jobId: string
    status: "queued" | "running" | "succeeded" | "failed"
    total: number
    completed: number
    filterSummary: string
    period: { from: string; to: string }
    permissionVersion: string
    projectionWatermark: string
    amountBasisNote: string
    downloadLabel?: string
    expiresAt?: string
}>
