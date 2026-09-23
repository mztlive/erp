/**
 * S3-05 M10 客户经营质量双口径 — 前端数据契约。
 * 后端：`GET /admin/customer-quality/current|history`（snake_case 入参），
 * 导出：`POST /admin/customer-quality/current|history/exports`。
 * 两口径永不合并排名或合计；金额一律服务端十进制字符串，前端只做展示。
 */

/** 双口径：当前负责客户 vs 历史负责订单贡献。 */
export type QualityCaliber = "current" | "history"

export type CurrentQualityDimension = "customer" | "owner_user" | "owner_org"
export type HistoryQualityDimension = "attribution_user" | "attribution_org"

/** 稳定身份候选：值是 ID，标签自带 ID 以区分同名。 */
export type QualityFilterOption = Readonly<{
    value: string
    label: string
}>

export type CurrentQualityQuery = Readonly<{
    from: string
    to: string
    ownerUserIds?: readonly string[]
    orgUnitIds?: readonly string[]
    includeDescendants?: boolean
    customerId?: string
    ownerGroup?: string
    q?: string
    dimension: CurrentQualityDimension
    sort: string
    scopeVersion?: string
    page: number
    pageSize: number
}>

export type HistoryQualityQuery = Readonly<{
    from: string
    to: string
    attributionUserIds?: readonly string[]
    attributionOrgUnitIds?: readonly string[]
    attributionGroup?: string
    customerId?: string
    q?: string
    dimension: HistoryQualityDimension
    sort: string
    scopeVersion?: string
    page: number
    pageSize: number
}>

export type QualityTotals = Readonly<{
    objectCount: number
    orderCount: number
    grossTotal: string
    unpricedCount: number
}>

export type CurrentQualityRow = Readonly<{
    rowId: string
    kind: string
    customerId?: string
    customerNo?: string
    customerName?: string
    groupId?: string
    label?: string
    ownerUserId?: string
    ownerUserName?: string
    ownerOrgUnitId?: string
    ownerOrgUnitName?: string
    customerCount?: number
    orderCount: number
    grossTotal: string
    unpricedCount: number
    firstEffectiveAt?: string
    latestEffectiveAt?: string
}>

export type HistoryQualityRow = Readonly<{
    rowId: string
    kind: string
    groupId?: string
    label?: string
    attributionUserId?: string
    attributionUserName?: string
    attributionOrgUnitId?: string
    attributionOrgUnitName?: string
    attributionPath?: readonly string[]
    orderId?: string
    orderNo?: string
    customerId?: string
    customerName?: string
    effectiveAt?: string
    orderCount?: number
    grossTotal: string
    unpricedCount: number
}>

type QualityScope = Readonly<{
    id: string
    label: string
    permissionVersion: string
}>

type QualityPeriod = Readonly<{
    from: string
    to: string
    basis: string
    basisLabel: string
    timezone: string
}>

/** 空态三分：无范围 / 筛选为空或期间无数据 / 请求失败（失败由 Query 错误态表达）。 */
export type DualEmptyReason = "no-scope" | "filtered-empty" | "no-data"

type DualQualityViewBase = Readonly<{
    emptyReason?: string
    scopeSummary: string
    asOf: string
    policyVersion: number
    organizationVersion: number
    scopeVersion: string
    scope: QualityScope
    period: QualityPeriod
    ownershipBasis: string
    totals: QualityTotals
    filterSummary: string
    canExport: boolean
}>

export type CurrentQualityView = DualQualityViewBase &
    Readonly<{
        rows: Readonly<{
            dimension: string
            items: readonly CurrentQualityRow[]
            total: number
        }>
    }>

export type HistoryQualityView = DualQualityViewBase &
    Readonly<{
        rows: Readonly<{
            dimension: string
            items: readonly HistoryQualityRow[]
            total: number
        }>
    }>

export type QualityExport = Readonly<{
    csvContent: string
    fileName: string
    rowCount: number
    generatedAt: string
}>

export function toDualEmptyReason(raw?: string): DualEmptyReason | null {
    if (raw === "no_scope") return "no-scope"
    if (raw === "filtered_empty") return "filtered-empty"
    if (raw === "no_data") return "no-data"
    return null
}
