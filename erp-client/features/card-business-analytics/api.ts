/**
 * W28 session-mock API：queryFn / mutationFn 纯函数。
 * 指标、覆盖率、三分法与利润均来自服务端投影；前端不重算 totals。
 */

import { mockDelay } from "@/lib/mock-delay"
import { applyFieldPermissions } from "@/lib/field-permissions"
import { resolveFreshness } from "@/lib/freshness"
import type {
  AuthorizedCardMetric,
  CardBusinessAnalyticsQuery,
  CardBusinessAnalyticsView,
  CardBusinessDimension,
  CardBusinessExportJob,
  CardBusinessFreshness,
  CardBusinessRow,
  CostBasisCode,
  DateBasis,
  DateBasisConfig,
} from "@/features/card-business-analytics/types"
import { DATE_BASIS_LABEL } from "@/features/card-business-analytics/types"
import {
  COST_BASIS_LABEL,
  COVERAGE_FILTER_LABEL,
  DIMENSION_LABEL,
  EXPIRY_STATE_LABEL,
} from "@/features/card-business-analytics/types"
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

const COST_BASIS_RANK: Record<CostBasisCode, number> = {
  ACTUAL: 0,
  STANDARD: 1,
  NONE: 2,
}

const COVERAGE_RANK: Record<CardBusinessRow["coverageStatus"], number> = {
  covered: 0,
  partial: 1,
  none: 2,
}

function sortValue(row: CardBusinessRow, key: string): string | number {
  switch (key) {
    case "customer":
      return row.customerLabel
    case "salesOrder":
      return row.salesOrderNo ?? ""
    case "category":
      return row.voucherCategoryLabel
    case "cardRef":
      return row.cardInstanceRef ?? ""
    case "consumption":
    case "consumptionGross":
      return Number(row.consumptionGross)
    case "refund":
      return Number(row.refundGross)
    case "costBasis":
      return COST_BASIS_RANK[row.costBasis]
    case "cost":
      return row.costNet == null ? Number.NEGATIVE_INFINITY : Number(row.costNet)
    case "coverage":
      return COVERAGE_RANK[row.coverageStatus]
    case "balance":
      return Number(row.unfulfilledBalanceGross)
    default:
      return Number(row.consumptionGross)
  }
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

  return list
}

function sortRows(
  rows: readonly CardBusinessRow[],
  sort: string
): CardBusinessRow[] {
  const [sortKey, sortDir] = sort.split(":")
  const desc = sortDir === "desc"
  const list = [...rows]
  list.sort((a, b) => {
    const av = sortValue(a, sortKey)
    const bv = sortValue(b, sortKey)
    if (typeof av === "string" || typeof bv === "string") {
      const cmp = String(av).localeCompare(String(bv), "zh-CN")
      return desc ? -cmp : cmp
    }
    return desc ? (bv as number) - (av as number) : (av as number) - (bv as number)
  })
  return list
}

/**
 * 分析视角（dimension）服务端分组投影：按客户/销售单/类目聚合金额，
 * 卡实例摘要保持逐行。聚合行不提供单对象下钻链接。
 */
function groupByDimension(
  rows: readonly CardBusinessRow[],
  dimension: CardBusinessDimension
): CardBusinessRow[] {
  if (dimension === "card_instance") {
    return rows.map((r) => ({ ...r, groupKey: r.rowId, rowCount: 1 }))
  }

  const keyOf = (r: CardBusinessRow): string => {
    if (dimension === "customer") return r.customerId ?? ""
    if (dimension === "sales_order") return r.salesOrderId ?? ""
    return r.voucherCategoryLabel
  }

  const groups = new Map<string, CardBusinessRow[]>()
  for (const r of rows) {
    const key = keyOf(r)
    const bucket = groups.get(key) ?? []
    bucket.push(r)
    groups.set(key, bucket)
  }

  const toCents = (s?: string): number => {
    const n = Number(s)
    return Number.isFinite(n) ? Math.round(n * 100) : 0
  }
  const fromCents = (c: number): string => (c / 100).toFixed(2)

  return Array.from(groups.entries()).map(([key, bucket]) => {
    const first = bucket[0]
    const sum = (f: (r: CardBusinessRow) => string | undefined) =>
      fromCents(bucket.reduce((acc, r) => acc + toCents(f(r)), 0))

    const basisCounts = new Map<CostBasisCode, number>()
    for (const r of bucket) {
      basisCounts.set(
        r.costBasis,
        (basisCounts.get(r.costBasis) ?? 0) + toCents(r.consumptionGross)
      )
    }
    const dominantBasis =
      Array.from(basisCounts.entries()).sort((a, b) => b[1] - a[1])[0]?.[0] ??
      first.costBasis

    const coverageSet = new Set(bucket.map((r) => r.coverageStatus))
    const coverageStatus =
      coverageSet.size === 1 ? first.coverageStatus : ("partial" as const)

    const hasNonNoneCost = bucket.some(
      (r) => r.costBasis !== "NONE" && r.costNet != null
    )

    return {
      rowId: `group:${dimension}:${key}`,
      customerId: dimension === "customer" ? first.customerId : undefined,
      customerLabel:
        dimension === "customer"
          ? first.customerLabel
          : `${first.customerLabel} 等`,
      salesOrderId: dimension === "sales_order" ? first.salesOrderId : undefined,
      salesOrderNo: dimension === "sales_order" ? first.salesOrderNo : undefined,
      voucherCategoryLabel:
        dimension === "voucher_category"
          ? first.voucherCategoryLabel
          : bucket.length > 1 &&
              new Set(bucket.map((r) => r.voucherCategoryLabel)).size > 1
            ? "多类目"
            : first.voucherCategoryLabel,
      consumptionGross: sum((r) => r.consumptionGross),
      refundGross: sum((r) => r.refundGross),
      costBasis: dominantBasis,
      costNet: hasNonNoneCost
        ? sum((r) => (r.costBasis === "NONE" ? undefined : r.costNet))
        : undefined,
      coverageStatus,
      unconsumedBalanceGross: sum((r) => r.unconsumedBalanceGross),
      unfulfilledBalanceGross: sum((r) => r.unfulfilledBalanceGross),
      groupKey: key,
      rowCount: bucket.length,
    }
  })
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
    detail: "实际 + 标准成本 · 不含无可用成本",
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

  // 先按筛选条件过滤原始明细，再按分析视角分组，最后服务端排序（作用于全量分组行）
  const filteredRaw = filterRows(W28_ROWS, query)
  const groupedRows = groupByDimension(filteredRaw, query.dimension)
  const sortedGrouped = sortRows(
    groupedRows,
    query.sort || "consumption:desc"
  )

  const filteredForTable = sortedGrouped.map((r) =>
    applyFieldPermissions(r, fieldHide, {
      cost: (row) => ({
        ...row,
        costNet: undefined,
        supplierOrderHref: undefined,
      }),
      profit: (row) => ({
        ...row,
        costNet: undefined,
        supplierOrderHref: undefined,
      }),
    })
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

  const projectionUpdatedAt = "2026-08-01T09:28:40+08:00"
  const consumedOutboxWatermark = "2026-08-01T09:28:10+08:00"
  const sourceFactWatermark = "2026-08-01T09:28:05+08:00"
  const balanceSnapshotAt = "2026-08-01T08:00:00+08:00"
  const freshness: CardBusinessFreshness = resolveFreshness(query.freshnessDemo, {
    fresh: {
      projectionUpdatedAt,
      consumedOutboxWatermark,
      sourceFactWatermark,
      balanceSnapshotAt,
      lagSeconds: 18,
      maxLagSeconds: 60,
      slaState: "WITHIN_SLA",
      state: "fresh",
    },
    stale: {
      projectionUpdatedAt: "2026-08-01T08:10:00+08:00",
      consumedOutboxWatermark: "2026-08-01T08:09:00+08:00",
      sourceFactWatermark: "2026-08-01T09:25:00+08:00",
      balanceSnapshotAt,
      lagSeconds: 95,
      maxLagSeconds: 60,
      slaState: "BREACHED",
      state: "stale",
    },
    rebuilding: {
      projectionUpdatedAt,
      consumedOutboxWatermark,
      sourceFactWatermark,
      balanceSnapshotAt,
      lagSeconds: 12,
      maxLagSeconds: 60,
      slaState: "REBUILDING",
      state: "rebuilding",
    },
    failed: {
      projectionUpdatedAt: "2026-08-01T08:55:00+08:00",
      consumedOutboxWatermark: "2026-08-01T08:50:00+08:00",
      sourceFactWatermark: "2026-08-01T08:50:00+08:00",
      balanceSnapshotAt,
      lagSeconds: 0,
      maxLagSeconds: 60,
      slaState: "FAILED",
      state: "failed",
    },
  })

  const filterParts = [
    `${query.from} ~ ${query.to}`,
    `日期口径=${dateBasisLabel(query.dateBasis, config)}`,
    `视角=${DIMENSION_LABEL[query.dimension]}`,
  ]
  if (query.customerId) {
    const label = W28_ROWS.find((r) => r.customerId === query.customerId)
      ?.customerLabel
    filterParts.push(`客户=${label ?? query.customerId}`)
  }
  if (query.salesOrderId) {
    const label = W28_ROWS.find(
      (r) => r.salesOrderId === query.salesOrderId
    )?.salesOrderNo
    filterParts.push(`销售单=${label ?? query.salesOrderId}`)
  }
  if (query.costBasis?.length)
    filterParts.push(
      `成本口径=${query.costBasis.map((b) => COST_BASIS_LABEL[b]).join("、")}`
    )
  if (query.expiryState && query.expiryState !== "all")
    filterParts.push(`履约期限=${EXPIRY_STATE_LABEL[query.expiryState]}`)
  if (query.coverage && query.coverage !== "all")
    filterParts.push(`覆盖=${COVERAGE_FILTER_LABEL[query.coverage]}`)

  const filterDigest = filterParts.join("|")
  const filterSummary = filterParts.join(" · ")

  const noneAmount = W28_NONE_CONSUMPTION_GROSS
  const coverageNotice = profitReferenceOnly
    ? `成本覆盖率 ${rateLabel} 低于阈值 ${W28_THRESHOLD}。无可用成本口径的消费额 ¥${Number(noneAmount).toLocaleString("zh-CN")} 无可用成本，不按零成本计入利润。成本不完整，结果仅供参考。`
    : `成本覆盖率 ${rateLabel}。实际成本 / 标准成本 / 无可用成本三分法同屏；无可用成本不进入成本与利润。`

  const metrics = buildMetrics({
    canViewCost,
    canViewProfit,
    scopeFullyExpired,
  })

  const from = Math.max(0, (query.page - 1) * query.pageSize)
  const pageItems = filteredForTable.slice(from, from + query.pageSize)

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
      total: filteredForTable.length,
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
      "销售/面值/消费/余额为含税；成本/毛差/经营贡献为不含税。无可用成本不按零成本计入利润。",
    wechatExcludedNote: input.view.wechatExcludedNote,
    rowCount: input.view.rows.total,
  })
}
