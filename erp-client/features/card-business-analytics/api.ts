/**
 * W28 session-mock API：queryFn / mutationFn 纯函数。
 * 指标、覆盖率、三分法与利润均来自服务端投影；前端不重算 totals。
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  AuthorizedCardMetric,
  CardBusinessAnalyticsQuery,
  CardBusinessAnalyticsView,
  CardBusinessExportJob,
  CardBusinessRow,
  DateBasis,
  DateBasisConfig,
  ProjectionFreshnessState,
} from "@/features/card-business-analytics/types"
import { DATE_BASIS_LABEL } from "@/features/card-business-analytics/types"
import {
  W28_ACTUAL_CONSUMPTION_GROSS,
  W28_BY_BASIS,
  W28_BY_CATEGORY,
  W28_BY_CUSTOMER,
  W28_CONSUMPTION_TREND,
  W28_CONTRIBUTION_TREND,
  W28_DATE_BASIS_CONFIGURED,
  W28_DATE_BASIS_UNCONFIGURED,
  W28_NONE_CONSUMPTION_GROSS,
  W28_PERMISSION_VERSION,
  W28_ROWS,
  W28_SCOPE_LABEL,
  W28_STANDARD_CONSUMPTION_GROSS,
  W28_THRESHOLD,
  W28_TOTAL_CONSUMPTION_GROSS,
  W28_WECHAT_CONSUMPTION_GROSS,
  W28_WECHAT_COST_NET,
  W28_WECHAT_EXCLUDED_NOTE,
  createW28ExportJob,
  getW28ExportJob,
} from "@/mock/card-business-analytics"

export type DateBasisConfigQuery = {
  /** QA：basisConfig=missing 模拟未配置默认日期口径 */
  scenario?: "default" | "missing"
}

function dateBasisLabel(code: DateBasis, config: DateBasisConfig): string {
  return (
    config.allowedDateBases.find((b) => b.code === code)?.label ??
    DATE_BASIS_LABEL[code]
  )
}

function filterRows(
  rows: readonly CardBusinessRow[],
  query: CardBusinessAnalyticsQuery
): CardBusinessRow[] {
  let list = [...rows]

  if (query.customerId) {
    list = list.filter((r) => r.customerId === query.customerId)
  }
  if (query.salesOrderId) {
    list = list.filter(
      (r) =>
        r.salesOrderId === query.salesOrderId ||
        r.salesOrderNo === query.salesOrderId
    )
  }
  if (query.voucherCategoryId) {
    list = list.filter((r) =>
      r.voucherCategoryLabel.includes(query.voucherCategoryId!)
    )
  }
  if (query.costBasis && query.costBasis.length > 0) {
    const set = new Set(query.costBasis)
    list = list.filter((r) => set.has(r.costBasis))
  }
  if (query.coverage === "none") {
    list = list.filter((r) => r.coverageStatus === "none")
  } else if (query.coverage === "below_threshold") {
    list = list.filter(
      (r) => r.coverageStatus === "none" || r.coverageStatus === "partial"
    )
  }

  const sort = query.sort || "consumptionGross:desc"
  if (sort.startsWith("consumptionGross")) {
    const desc = !sort.endsWith(":asc")
    list.sort((a, b) => {
      const an = Number(a.consumptionGross)
      const bn = Number(b.consumptionGross)
      return desc ? bn - an : an - bn
    })
  }

  return list
}

function applyFieldPermissions(
  row: CardBusinessRow,
  fieldHide: CardBusinessAnalyticsQuery["fieldHide"]
): CardBusinessRow {
  if (fieldHide === "cost" || fieldHide === "profit") {
    return {
      ...row,
      costNet: undefined,
      supplierOrderHref: undefined,
    }
  }
  return row
}

function resolveFreshness(
  demo?: ProjectionFreshnessState
): CardBusinessAnalyticsView["freshness"] {
  const projectionUpdatedAt = "2026-08-01T09:28:40+08:00"
  const consumedOutboxWatermark = "2026-08-01T09:28:10+08:00"
  const sourceFactWatermark = "2026-08-01T09:28:05+08:00"
  const balanceSnapshotAt = "2026-08-01T08:00:00+08:00"

  if (demo === "stale") {
    return {
      projectionUpdatedAt: "2026-08-01T08:10:00+08:00",
      consumedOutboxWatermark: "2026-08-01T08:09:00+08:00",
      sourceFactWatermark: "2026-08-01T09:25:00+08:00",
      balanceSnapshotAt,
      lagSeconds: 95,
      maxLagSeconds: 60,
      slaState: "BREACHED",
      state: "stale",
    }
  }
  if (demo === "rebuilding") {
    return {
      projectionUpdatedAt,
      consumedOutboxWatermark,
      sourceFactWatermark,
      balanceSnapshotAt,
      lagSeconds: 12,
      maxLagSeconds: 60,
      slaState: "REBUILDING",
      state: "rebuilding",
    }
  }
  if (demo === "failed") {
    return {
      projectionUpdatedAt: "2026-08-01T08:55:00+08:00",
      consumedOutboxWatermark: "2026-08-01T08:50:00+08:00",
      sourceFactWatermark: "2026-08-01T08:50:00+08:00",
      balanceSnapshotAt,
      lagSeconds: 0,
      maxLagSeconds: 60,
      slaState: "FAILED",
      state: "failed",
    }
  }
  return {
    projectionUpdatedAt,
    consumedOutboxWatermark,
    sourceFactWatermark,
    balanceSnapshotAt,
    lagSeconds: 18,
    maxLagSeconds: 60,
    slaState: "WITHIN_SLA",
    state: "fresh",
  }
}

function buildMetrics(options: {
  canViewCost: boolean
  canViewProfit: boolean
  scopeFullyExpired: boolean
}): AuthorizedCardMetric[] {
  const sales: AuthorizedCardMetric = {
    key: "cardSalesAmountGross",
    label: "卡券销售金额",
    value: "8600000.00",
    taxBasis: "GROSS",
    currency: "CNY",
    valueState: "available",
    detail: "企业客户成交",
  }
  const face: AuthorizedCardMetric = {
    key: "cardFaceValueGross",
    label: "可消费总额度",
    value: "8600000.00",
    taxBasis: "GROSS",
    currency: "CNY",
    valueState: "available",
    detail: "面值 + 配赠",
  }
  const consumption: AuthorizedCardMetric = {
    key: "cumulativeCardConsumptionGross",
    label: "累计卡券消费",
    value: W28_TOTAL_CONSUMPTION_GROSS,
    taxBasis: "GROSS",
    currency: "CNY",
    valueState: "available",
    detail: "卡券支付 − 退款 · 不含微信",
  }
  const balance: AuthorizedCardMetric = {
    key: "unconsumedBalanceGross",
    label: "未消费余额",
    value: "3360000.00",
    taxBasis: "GROSS",
    currency: "CNY",
    valueState: "available",
    detail: "商城余额记录",
  }
  const rate: AuthorizedCardMetric = {
    key: "consumptionRate",
    label: "消费率",
    value: "60.93%",
    taxBasis: "GROSS",
    currency: "CNY",
    valueState: "available",
    detail: "累计消费 ÷ 可消费额度",
  }
  const cost: AuthorizedCardMetric = {
    key: "consumptionCostNet",
    label: "实际消费成本",
    value: options.canViewCost ? "3702848.00" : null,
    taxBasis: "NET",
    currency: "CNY",
    valueState: options.canViewCost ? "available" : "masked",
    reasonCode: options.canViewCost ? undefined : "FIELD_PERMISSION",
    detail: "ACTUAL+STANDARD · 不含 NONE",
  }
  const margin: AuthorizedCardMetric = {
    key: "consumptionMarginNet",
    label: "消费毛差",
    value: options.canViewProfit ? "934320.00" : null,
    taxBasis: "NET",
    currency: "CNY",
    valueState: options.canViewProfit ? "available" : "masked",
    reasonCode: options.canViewProfit ? undefined : "FIELD_PERMISSION",
    detail: "不含税消费 − 不含税成本",
  }
  const contribution: AuthorizedCardMetric = {
    key: "currentContributionNet",
    label: "当前经营贡献",
    value: options.canViewProfit ? "1120000.00" : null,
    taxBasis: "NET",
    currency: "CNY",
    valueState: options.canViewProfit ? "available" : "masked",
    reasonCode: options.canViewProfit ? undefined : "FIELD_PERMISSION",
    detail: "非最终利润 · 须同屏未履约余额",
  }
  const unfulfilled: AuthorizedCardMetric = {
    key: "unfulfilledBalanceGross",
    label: "未履约余额",
    value: "3360000.00",
    taxBasis: "GROSS",
    currency: "CNY",
    valueState: "available",
    detail: "仍承担履约责任的额度",
  }

  const list = [
    sales,
    face,
    consumption,
    balance,
    rate,
    cost,
    margin,
    contribution,
    unfulfilled,
  ]

  if (options.scopeFullyExpired && options.canViewProfit) {
    list.push({
      key: "finalProfitNet",
      label: "最终经营盈亏",
      value: "980500.00",
      taxBasis: "NET",
      currency: "CNY",
      valueState: "available",
      detail: "履约期限已到期范围",
    })
  }

  return list
}

export async function fetchDateBasisConfig(
  query: DateBasisConfigQuery = {}
): Promise<DateBasisConfig> {
  await mockDelay(60)
  if (query.scenario === "missing") {
    return W28_DATE_BASIS_UNCONFIGURED
  }
  return W28_DATE_BASIS_CONFIGURED
}

export async function fetchCardBusinessAnalytics(
  query: CardBusinessAnalyticsQuery
): Promise<CardBusinessAnalyticsView> {
  await mockDelay(120)

  const config = W28_DATE_BASIS_CONFIGURED
  const allowed = new Set(config.allowedDateBases.map((b) => b.code))
  if (!query.dateBasis || !allowed.has(query.dateBasis)) {
    throw new Error(
      "日期口径未通过校验；请先选择系统允许的日期口径。"
    )
  }

  const fieldHide = query.fieldHide ?? "none"
  const canViewCost = fieldHide !== "cost"
  const canViewProfit = fieldHide !== "profit"

  // 履约到期视角 + expiryState=expired 时展示最终利润
  const scopeFullyExpired =
    query.dateBasis === "expiry" && query.expiryState === "expired"

  const filtered = filterRows(W28_ROWS, query).map((r) =>
    applyFieldPermissions(r, fieldHide)
  )

  const coveredGross =
    Number(W28_ACTUAL_CONSUMPTION_GROSS) + Number(W28_STANDARD_CONSUMPTION_GROSS)
  const totalGross = Number(W28_TOTAL_CONSUMPTION_GROSS)
  const ratePercent = totalGross === 0 ? 0 : (coveredGross / totalGross) * 100
  const rateLabel = `${ratePercent.toFixed(1)}%`
  const thresholdNum = 95
  const profitReferenceOnly = ratePercent < thresholdNum
  const status =
    ratePercent >= 95
      ? ("complete" as const)
      : ratePercent >= 80
        ? ("warning" as const)
        : ratePercent > 0
          ? ("insufficient" as const)
          : ("insufficient" as const)

  const byBasis = canViewCost
    ? W28_BY_BASIS
    : W28_BY_BASIS.map((s) => ({
        ...s,
        costNet: undefined,
      }))

  const freshness = resolveFreshness(query.freshnessDemo)

  const filterParts = [
    `${query.from} ~ ${query.to}`,
    `日期口径=${dateBasisLabel(query.dateBasis, config)}`,
    `维度=${query.dimension}`,
  ]
  if (query.customerId) filterParts.push(`客户=${query.customerId}`)
  if (query.salesOrderId) filterParts.push(`销售单=${query.salesOrderId}`)
  if (query.costBasis?.length)
    filterParts.push(`成本口径=${query.costBasis.join("+")}`)
  if (query.expiryState && query.expiryState !== "all")
    filterParts.push(`履约=${query.expiryState}`)
  if (query.coverage && query.coverage !== "all")
    filterParts.push(`覆盖=${query.coverage}`)

  const filterDigest = filterParts.join("|")
  const filterSummary = filterParts.join(" · ")

  const noneAmount = W28_NONE_CONSUMPTION_GROSS
  const coverageNotice = profitReferenceOnly
    ? `成本覆盖率 ${rateLabel} 低于阈值 ${W28_THRESHOLD}。NONE 消费 ¥${Number(noneAmount).toLocaleString("zh-CN")} 无可用成本，不按零成本计入利润。成本不完整，结果仅供参考。`
    : `成本覆盖率 ${rateLabel}。ACTUAL / STANDARD / NONE 三分法同屏；NONE 不进入成本与利润。`

  const metrics = buildMetrics({
    canViewCost,
    canViewProfit,
    scopeFullyExpired,
  })

  const from = Math.max(0, (query.page - 1) * query.pageSize)
  const pageItems = filtered.slice(from, from + query.pageSize)

  return {
    scope: {
      timezone: "Asia/Shanghai",
      currency: "CNY",
      filterDigest,
      permissionVersion: W28_PERMISSION_VERSION,
      scopeLabel: W28_SCOPE_LABEL,
    },
    period: {
      from: query.from,
      to: query.to,
      dateBasis: query.dateBasis,
      dateBasisLabel: dateBasisLabel(query.dateBasis, config),
    },
    freshness,
    coverage: {
      coveredConsumptionGross: coveredGross.toFixed(2),
      totalConsumptionGross: W28_TOTAL_CONSUMPTION_GROSS,
      rate: rateLabel,
      ratePercent,
      threshold: W28_THRESHOLD,
      status,
      byBasis,
      dominantBasis: "ACTUAL",
      notice: coverageNotice,
      profitReferenceOnly,
    },
    metrics,
    scopeFullyExpired,
    finalProfitNet: scopeFullyExpired && canViewProfit ? "980500.00" : null,
    finalProfitUnavailableReason: scopeFullyExpired
      ? undefined
      : "当前范围履约期限未全部到期，不展示最终经营盈亏；请查看当前经营贡献与未履约余额。",
    trends: {
      consumption: W28_CONSUMPTION_TREND,
      contribution: canViewProfit
        ? W28_CONTRIBUTION_TREND
        : W28_CONTRIBUTION_TREND.map((t) => ({
            ...t,
            marginNet: "—",
            contributionNet: "—",
          })),
    },
    breakdowns: {
      byCategory: W28_BY_CATEGORY,
      byCustomer: W28_BY_CUSTOMER,
    },
    rows: {
      items: pageItems,
      total: filtered.length,
    },
    filterSummary,
    wechatExcludedNote: W28_WECHAT_EXCLUDED_NOTE,
    wechatExcluded: {
      consumptionGross: W28_WECHAT_CONSUMPTION_GROSS,
      costNet: W28_WECHAT_COST_NET,
    },
    fieldPermissions: {
      canViewCost,
      canViewProfit,
      canExport: true,
    },
    governanceLinks: {
      noneCoverageHref: `/governance/integration-errors?view=mine`,
      backfillHref: `/governance/history-backfill?from=${encodeURIComponent(query.from)}&to=${encodeURIComponent(query.to)}&costBasis=NONE`,
      integrationErrorsHref: `/governance/integration-errors?view=mine`,
    },
  }
}

export async function startCardBusinessExport(input: {
  query: CardBusinessAnalyticsQuery
  view: Pick<
    CardBusinessAnalyticsView,
    | "period"
    | "scope"
    | "freshness"
    | "coverage"
    | "filterSummary"
    | "wechatExcludedNote"
    | "fieldPermissions"
    | "rows"
  >
}): Promise<CardBusinessExportJob> {
  await mockDelay(100)
  if (!input.view.fieldPermissions.canExport) {
    throw new Error("当前权限不允许导出")
  }
  if (!input.query.dateBasis || !input.query.from || !input.query.to) {
    throw new Error("from/to/dateBasis 未完整，已阻断导出")
  }
  return createW28ExportJob({
    periodFrom: input.view.period.from,
    periodTo: input.view.period.to,
    dateBasis: input.view.period.dateBasis,
    filterSummary: input.view.filterSummary,
    coverageRate: input.view.coverage.rate,
    projectionUpdatedAt: input.view.freshness.projectionUpdatedAt,
    consumedOutboxWatermark: input.view.freshness.consumedOutboxWatermark,
    balanceSnapshotAt: input.view.freshness.balanceSnapshotAt,
    lagSeconds: input.view.freshness.lagSeconds,
    permissionVersion: input.view.scope.permissionVersion,
    taxDisclaimer:
      "销售/面值/消费/余额为含税（GROSS）；成本/毛差/经营贡献为不含税（NET）。NONE 不按零成本计入利润。",
    wechatExcludedNote: input.view.wechatExcludedNote,
    rowCount: input.view.rows.total,
  })
}

export async function fetchCardBusinessExportJob(
  jobId: string
): Promise<CardBusinessExportJob | null> {
  await mockDelay(40)
  return getW28ExportJob(jobId)
}
