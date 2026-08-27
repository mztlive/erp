/** W28 卡券消费台账与经营分析 · 客户端契约 */

import type { StatusTone } from "@/components/ui/status-badge"

export type DateBasis = "consumption" | "sales" | "expiry"

export type CardBusinessDimension =
    | "customer"
    | "sales_order"
    | "voucher_category"
    | "card_instance"

export type CostBasisCode = "ACTUAL" | "STANDARD" | "NONE"

export type CoverageFilter = "below_threshold" | "none" | "all"

export type ExpiryState = "active" | "expired" | "all"

type ProjectionSlaState = "WITHIN_SLA" | "BREACHED" | "REBUILDING" | "FAILED"

export type ProjectionFreshnessState =
    | "fresh"
    | "stale"
    | "rebuilding"
    | "failed"

export type TaxBasis = "GROSS" | "NET"

export type CoverageStatus =
    | "complete"
    | "acceptable"
    | "warning"
    | "insufficient"

type DateBasisOption = Readonly<{
    code: DateBasis
    label: string
    explanation: string
}>

export type DateBasisConfig = Readonly<{
    /** 服务端已固化默认；未配置时为 undefined（Q2） */
    configuredDateBasis?: DateBasis
    allowedDateBases: readonly DateBasisOption[]
    configurationVersion: string
}>

export type CardBusinessAnalyticsQuery = Readonly<{
    from: string
    to: string
    dateBasis: DateBasis
    dimension: CardBusinessDimension
    customerId?: string
    salesOrderId?: string
    voucherCategoryId?: string
    /** 多选成本口径；空表示全部 */
    costBasis?: readonly CostBasisCode[]
    expiryState?: ExpiryState
    coverage?: CoverageFilter
    compare?: "previous_period"
    sort: string
    page: number
    pageSize: number
}>

type AuthorizedCardMetric = Readonly<{
    key: string
    label: string
    value: string | null
    taxBasis: TaxBasis
    currency: "CNY"
    valueState: "available" | "unavailable" | "masked"
    reasonCode?: string
    detail?: string
}>

type CostBasisSlice = Readonly<{
    basis: CostBasisCode
    consumptionGross: string
    /** NONE 不返回成本金额，禁止 0 */
    costNet?: string
    share: string
    shareLabel: string
}>

type CardBusinessCoverage = Readonly<{
    coveredConsumptionGross: string
    totalConsumptionGross: string
    rate: string | null
    ratePercent: number
    threshold: string
    status: CoverageStatus
    byBasis: readonly CostBasisSlice[]
    /** 主导成本口径（用于 CostCoverageNotice.basis） */
    dominantBasis: CostBasisCode
    notice: string
    profitReferenceOnly: boolean
}>

type CardBusinessFreshness = Readonly<{
    projectionUpdatedAt: string
    consumedOutboxWatermark: string
    sourceFactWatermark: string
    balanceSnapshotAt?: string
    lagSeconds: number
    maxLagSeconds: 60
    slaState: ProjectionSlaState
    state: ProjectionFreshnessState
}>

type CardBusinessTrendPoint = Readonly<{
    period: string
    salesGross: string
    consumptionGross: string
    refundGross: string
    balanceGross: string
}>

type ContributionTrendPoint = Readonly<{
    period: string
    marginNet: string
    contributionNet: string
    coverageRate: string
    coveragePercent: number
}>

type CardBusinessBreakdownItem = Readonly<{
    id: string
    label: string
    consumptionGross: string
    share: string
}>

export type CardBusinessRow = Readonly<{
    rowId: string
    customerId?: string
    customerLabel: string
    salesOrderId?: string
    salesOrderNo?: string
    voucherCategoryLabel: string
    /** 不可逆稳定卡实例引用摘要；禁止卡号/卡密/手机 */
    cardInstanceRef?: string
    consumptionGross: string
    refundGross: string
    costBasis: CostBasisCode
    costNet?: string
    coverageStatus: "covered" | "partial" | "none"
    unconsumedBalanceGross: string
    unfulfilledBalanceGross: string
    riskLabel?: string
    /** W25 稳定消费/商城订单 ID */
    consumptionOrderId?: string
    consumptionOrderHref?: string
    /** 成本来源下钻 */
    supplierOrderHref?: string
    /** 维度聚合行的分组键（客户/销售单/类目）；单行视角为 undefined */
    groupKey?: string
    rowCount?: number
}>

export type CardBusinessAnalyticsView = Readonly<{
    scope: {
        timezone: string
        currency: "CNY"
        filterDigest: string
        permissionVersion: string
        scopeLabel: string
    }
    period: {
        from: string
        to: string
        dateBasis: DateBasis
        dateBasisLabel: string
    }
    freshness: CardBusinessFreshness
    coverage: CardBusinessCoverage
    metrics: readonly AuthorizedCardMetric[]
    /** 履约期限是否全部到期；未到期不展示最终利润 */
    scopeFullyExpired: boolean
    finalProfitNet?: string | null
    finalProfitUnavailableReason?: string
    trends: {
        consumption: readonly CardBusinessTrendPoint[]
        contribution: readonly ContributionTrendPoint[]
    }
    breakdowns: {
        byCategory: readonly CardBusinessBreakdownItem[]
        byCustomer: readonly CardBusinessBreakdownItem[]
    }
    rows: {
        items: readonly CardBusinessRow[]
        total: number
    }
    filterSummary: string
    wechatExcludedNote: string
    wechatExcluded: {
        consumptionGross: string
        costNet: string
    }
    fieldPermissions: {
        canViewCost: boolean
        canViewProfit: boolean
        canExport: boolean
    }
    governanceLinks: {
        noneCoverageHref: string
        backfillHref: string
        integrationErrorsHref: string
    }
}>

export type CardBusinessExportJob = Readonly<{
    jobId: string
    status: "queued" | "running" | "succeeded" | "failed"
    total: number
    completed: number
    createdAt: string
    downloadLabel?: string
    watermark: {
        periodFrom: string
        periodTo: string
        dateBasis: DateBasis
        filterSummary: string
        coverageRate: string | null
        projectionUpdatedAt: string
        consumedOutboxWatermark: string
        balanceSnapshotAt?: string
        lagSeconds: number
        permissionVersion: string
        taxDisclaimer: string
        wechatExcludedNote: string
        rowCount: number
    }
}>

export type PeriodPreset = "month-to-date" | "last-month" | "quarter-to-date"

export const DATE_BASIS_LABEL: Record<DateBasis, string> = {
    consumption: "消费发生日",
    sales: "销售发生日",
    expiry: "履约到期日",
}

export const DIMENSION_LABEL: Record<CardBusinessDimension, string> = {
    customer: "客户",
    sales_order: "销售单",
    voucher_category: "卡券类目",
    card_instance: "卡实例摘要",
}

export const COST_BASIS_LABEL: Record<CostBasisCode, string> = {
    ACTUAL: "实际成本",
    STANDARD: "标准成本",
    NONE: "无可用成本",
}

export const COVERAGE_STATUS_UI: Record<
    CoverageStatus,
    {
        label: string
        tone: StatusTone
        noticeState: "complete" | "partial" | "none"
    }
> = {
    complete: { label: "完整覆盖", tone: "success", noticeState: "complete" },
    acceptable: { label: "可接受", tone: "success", noticeState: "complete" },
    warning: { label: "覆盖不足", tone: "warning", noticeState: "partial" },
    insufficient: {
        label: "覆盖严重不足",
        tone: "destructive",
        noticeState: "none",
    },
}

export const COST_BASIS_ROW_UI: Record<
    CostBasisCode,
    { label: string; tone: StatusTone }
> = {
    ACTUAL: { label: "实际成本", tone: "success" },
    STANDARD: { label: "标准成本", tone: "info" },
    NONE: { label: "无可用成本", tone: "warning" },
}

export const COVERAGE_FILTER_LABEL: Record<
    Exclude<CoverageFilter, "all">,
    string
> = {
    below_threshold: "覆盖不足",
    none: "未覆盖",
}
