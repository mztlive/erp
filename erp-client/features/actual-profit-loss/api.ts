/**
 * W16 session-mock API：queryFn / mutationFn 纯函数。
 * 正式金额/覆盖/利润率均来自服务端投影；前端不得浮点重算覆盖 totals。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CostEntryDetail,
  ProfitLossExportJob,
  ProfitLossPeriodBasisConfig,
  ProfitLossQuery,
  ProfitLossRow,
  ProfitLossView,
  ProjectionFreshnessState,
} from "@/features/actual-profit-loss/types"
import {
  W16_COST_COMPOSITION,
  W16_COST_ENTRIES,
  W16_EXCLUDED_NOTE,
  W16_FORMULA_TEXT,
  W16_FORMULA_VERSION,
  W16_PERIOD_BASIS_CONFIGURED,
  W16_PERIOD_BASIS_UNCONFIGURED,
  W16_PERMISSION_VERSION,
  W16_SALES_ORDER_ROWS,
  W16_SCOPE,
  W16_STAGE_REFERENCE,
  W16_TREND,
  createW16ExportJob,
  getW16ExportJob,
  setW16CorrectionPending,
  w16CorrectionPending,
} from "@/mock/actual-profit-loss"

export type PeriodBasisConfigQuery = {
  /** QA：basisConfig=missing 模拟未配置 */
  scenario?: "default" | "missing"
}

function basisLabel(
  code: string,
  config: ProfitLossPeriodBasisConfig
): string {
  return (
    config.allowedPeriodBases.find((b) => b.code === code)?.label ?? code
  )
}

function matchSearch(
  q: string | undefined,
  parts: readonly (string | undefined)[]
): boolean {
  if (!q?.trim()) return true
  const needle = q.trim().toLowerCase()
  return parts.some((p) => p?.toLowerCase().includes(needle))
}

function filterRows(
  rows: readonly ProfitLossRow[],
  query: ProfitLossQuery
): ProfitLossRow[] {
  let list = [...rows]

  if (query.coverage === "covered") {
    list = list.filter((r) => r.coverageState === "COVERED")
  } else if (query.coverage === "uncovered") {
    list = list.filter(
      (r) =>
        r.coverageState === "UNCOVERED" || r.coverageState === "PARTIAL"
    )
  }

  if (query.customerId) {
    list = list.filter((r) => r.customerId === query.customerId)
  }
  if (query.salesOrderId) {
    list = list.filter((r) => r.objectId === query.salesOrderId)
  }
  if (query.benefitScenario) {
    list = list.filter((r) =>
      r.benefitScenarios?.includes(query.benefitScenario!)
    )
  }
  if (query.fulfillmentModes && query.fulfillmentModes.length > 0) {
    list = list.filter((r) =>
      r.fulfillmentModes?.some((m) => query.fulfillmentModes!.includes(m))
    )
  }

  list = list.filter((r) =>
    matchSearch(query.q, [
      r.identityLabel,
      r.customerLabel,
      r.customerId,
      r.objectId,
    ])
  )

  // 默认：实际盈亏升序（亏损优先）；未覆盖无利润排最后
  const sort = query.sort || "actualProfitLossNet:asc"
  if (sort.startsWith("actualProfitLossNet")) {
    const desc = sort.endsWith(":desc")
    list.sort((a, b) => {
      const av = a.actualProfitLossNet
      const bv = b.actualProfitLossNet
      if (av == null && bv == null) return 0
      if (av == null) return 1
      if (bv == null) return -1
      const an = Number(av)
      const bn = Number(bv)
      return desc ? bn - an : an - bn
    })
  }

  return list
}

function applyFieldPermissions(
  row: ProfitLossRow,
  fieldHide: ProfitLossQuery["fieldHide"]
): ProfitLossRow {
  if (fieldHide === "cost") {
    return {
      ...row,
      actualProcurementCostNet: undefined,
      actualFulfillmentCostNet: undefined,
      reductionsNet: undefined,
      costEntryIds: [],
      allowedDrilldowns: row.allowedDrilldowns.filter((d) => d !== "cost_entry"),
    }
  }
  if (fieldHide === "profit") {
    return {
      ...row,
      actualProfitLossNet: undefined,
      marginRate: undefined,
      marginUnavailableReason: "无利润字段权限",
    }
  }
  return row
}

function resolveFreshness(
  demo?: ProjectionFreshnessState
): ProfitLossView["freshness"] {
  const projectedAt = "2026-08-01T09:28:00+08:00"
  const sourceWatermark = "2026-08-01T09:27:12+08:00"
  if (demo === "stale") {
    return {
      projectedAt: "2026-08-01T08:10:00+08:00",
      sourceWatermark: "2026-08-01T09:25:00+08:00",
      state: "stale",
    }
  }
  if (demo === "rebuilding") {
    return {
      projectedAt,
      sourceWatermark,
      state: "rebuilding",
    }
  }
  if (demo === "failed") {
    return {
      projectedAt: "2026-08-01T08:55:00+08:00",
      sourceWatermark: "2026-08-01T08:50:00+08:00",
      state: "failed",
    }
  }
  return { projectedAt, sourceWatermark, state: "fresh" }
}

/** 服务端投影 totals：由 seed 行汇总的**预计算**结果（模拟服务端），非前端 ad-hoc 重算入口。 */
function projectTotalsFromSeed(rows: readonly ProfitLossRow[]): {
  netSalesRevenue: string
  actualProcurementCostNet: string
  actualFulfillmentCostNet: string
  reductionsNet: string
  actualProfitLossNet?: string
  marginRate?: string
  marginUnavailableReason?: string
  coveredNetRevenue: string
  uncoveredNetRevenue: string
  coverageRate: string
  coverageState: "complete" | "partial" | "none"
  reliability: "reliable" | "partial" | "unavailable"
} {
  // Mock 服务端：使用十进制字符串加法（分）避免 JS 浮点展示问题
  const toCents = (s: string | undefined) => {
    if (s == null || s === "") return null
    const n = Math.round(Number(s) * 100)
    return Number.isFinite(n) ? n : null
  }
  const fromCents = (c: number) => (c / 100).toFixed(2)

  let rev = 0
  let proc = 0
  let ful = 0
  let red = 0
  let covered = 0
  let uncovered = 0
  let hasUncoveredProfit = false
  let reliableProfitCents = 0
  let reliableRevCents = 0

  for (const r of rows) {
    const rRev = toCents(r.netSalesRevenue) ?? 0
    rev += rRev
    if (r.coverageState === "COVERED") {
      covered += rRev
      const p = toCents(r.actualProcurementCostNet) ?? 0
      const f = toCents(r.actualFulfillmentCostNet) ?? 0
      const d = toCents(r.reductionsNet) ?? 0
      proc += p
      ful += f
      red += d
      const profit = toCents(r.actualProfitLossNet)
      if (profit != null) {
        reliableProfitCents += profit
        reliableRevCents += rRev
      }
    } else if (r.coverageState === "PARTIAL") {
      uncovered += rRev
      hasUncoveredProfit = true
      const p = toCents(r.actualProcurementCostNet) ?? 0
      const f = toCents(r.actualFulfillmentCostNet) ?? 0
      const d = toCents(r.reductionsNet) ?? 0
      proc += p
      ful += f
      red += d
      // partial 利润不并入默认可靠合计
    } else {
      uncovered += rRev
      hasUncoveredProfit = true
      // UNCOVERED：不把缺失成本当 0 并入利润
    }
  }

  const coverageState: "complete" | "partial" | "none" =
    uncovered === 0 && rev > 0
      ? "complete"
      : covered === 0 && rev > 0
        ? "none"
        : rev === 0
          ? "none"
          : "partial"

  const coverageRate =
    rev === 0 ? "0.00%" : `${((covered / rev) * 100).toFixed(2)}%`

  let actualProfitLossNet: string | undefined
  let marginRate: string | undefined
  let marginUnavailableReason: string | undefined

  if (coverageState === "none") {
    actualProfitLossNet = undefined
    marginUnavailableReason = "成本完全未覆盖，不生成零成本利润"
  } else if (hasUncoveredProfit && coverageState === "partial") {
    // 可靠子集利润（服务端 partial 投影）
    actualProfitLossNet = fromCents(reliableProfitCents)
    if (reliableRevCents === 0) {
      marginRate = undefined
      marginUnavailableReason = "适用不含税收入为 0，利润率不适用"
    } else {
      marginRate = `${((reliableProfitCents / reliableRevCents) * 100).toFixed(2)}%`
    }
  } else {
    actualProfitLossNet = fromCents(reliableProfitCents)
    if (reliableRevCents === 0) {
      marginRate = undefined
      marginUnavailableReason = "适用不含税收入为 0，利润率不适用"
    } else {
      marginRate = `${((reliableProfitCents / reliableRevCents) * 100).toFixed(2)}%`
    }
  }

  return {
    netSalesRevenue: fromCents(rev),
    actualProcurementCostNet: fromCents(proc),
    actualFulfillmentCostNet: fromCents(ful),
    reductionsNet: fromCents(red),
    actualProfitLossNet,
    marginRate,
    marginUnavailableReason,
    coveredNetRevenue: fromCents(covered),
    uncoveredNetRevenue: fromCents(uncovered),
    coverageRate,
    coverageState,
    reliability:
      coverageState === "complete"
        ? "reliable"
        : coverageState === "partial"
          ? "partial"
          : "unavailable",
  }
}

export async function fetchPeriodBasisConfig(
  query: PeriodBasisConfigQuery = {}
): Promise<ProfitLossPeriodBasisConfig> {
  await mockDelay(60)
  if (query.scenario === "missing") {
    return W16_PERIOD_BASIS_UNCONFIGURED
  }
  return W16_PERIOD_BASIS_CONFIGURED
}

export async function fetchProfitLossView(
  query: ProfitLossQuery
): Promise<ProfitLossView> {
  await mockDelay(120)

  const config =
    // 配置版本随 basis 配置一起读取；此处用固定配置做校验
    W16_PERIOD_BASIS_CONFIGURED
  const allowed = new Set(
    config.allowedPeriodBases.map((b) => b.code)
  )
  if (!query.periodBasis || !allowed.has(query.periodBasis)) {
    throw new Error(
      "periodBasis 未通过允许集合校验；请先选择服务端允许的期间归属口径。"
    )
  }

  const fieldHide = query.fieldHide ?? "none"
  const canViewCost = fieldHide !== "cost"
  const canViewProfit = fieldHide !== "profit"
  const canViewRevenue = true

  const filtered = filterRows(W16_SALES_ORDER_ROWS, query).map((r) =>
    applyFieldPermissions(r, fieldHide)
  )

  // 服务端在同一数据范围上投影 totals / 趋势 / 构成 / 明细
  const seedForTotals = filterRows(W16_SALES_ORDER_ROWS, query)
  const projected = projectTotalsFromSeed(seedForTotals)

  const freshness = resolveFreshness(query.freshnessDemo)

  const costComposition = canViewCost
    ? W16_COST_COMPOSITION
    : W16_COST_COMPOSITION.map((c) => ({
        costType: c.costType,
        label: c.label,
        // 无权：金额与占比均不返回，避免图表比例泄露
        netAmount: "—",
        share: undefined,
      }))

  const trend = canViewProfit
    ? W16_TREND
    : W16_TREND.map((t) => ({
        ...t,
        actualProfitLossNet: undefined,
        actualCostNet: canViewCost ? t.actualCostNet : "—",
      }))

  const filterParts = [
    `${query.from} ~ ${query.to}`,
    `归属口径=${basisLabel(query.periodBasis, config)}`,
    `覆盖=${query.coverage}`,
    `维度=${query.dimension}`,
  ]
  if (query.customerId) filterParts.push(`客户=${query.customerId}`)
  if (query.salesOrderId) filterParts.push(`销售单=${query.salesOrderId}`)
  if (query.q) filterParts.push(`搜索=${query.q}`)

  return {
    scope: {
      id: W16_SCOPE.id,
      label: W16_SCOPE.label,
      permissionVersion: W16_PERMISSION_VERSION,
    },
    period: {
      from: query.from,
      to: query.to,
      basis: query.periodBasis,
      basisLabel: basisLabel(query.periodBasis, config),
      timezone: "Asia/Shanghai",
    },
    businessType: "GOODS_SERVICE",
    amountBasis: "NET",
    amountBasisLabel: "不含税",
    businessTypeLabel: "非卡券",
    formulaVersion: W16_FORMULA_VERSION,
    formulaText: W16_FORMULA_TEXT,
    freshness,
    coverage: {
      coveredNetRevenue: projected.coveredNetRevenue,
      uncoveredNetRevenue: projected.uncoveredNetRevenue,
      coverageRate: projected.coverageRate,
      reliability: projected.reliability,
      coverageState: projected.coverageState,
    },
    totals: {
      netSalesRevenue: canViewRevenue ? projected.netSalesRevenue : "—",
      actualProcurementCostNet: canViewCost
        ? projected.actualProcurementCostNet
        : undefined,
      actualFulfillmentCostNet: canViewCost
        ? projected.actualFulfillmentCostNet
        : undefined,
      reductionsNet: canViewCost ? projected.reductionsNet : undefined,
      actualProfitLossNet: canViewProfit
        ? projected.actualProfitLossNet
        : undefined,
      marginRate: canViewProfit ? projected.marginRate : undefined,
      marginUnavailableReason: canViewProfit
        ? projected.marginUnavailableReason
        : "无利润字段权限",
    },
    fieldPermissions: {
      canViewRevenue,
      canViewCost,
      canViewProfit,
      canExport: true,
    },
    trend,
    costComposition,
    stageReference: W16_STAGE_REFERENCE,
    rows: {
      dimension: query.dimension,
      items: filtered,
      total: filtered.length,
    },
    filterSummary: filterParts.join(" · "),
    excludedNote: W16_EXCLUDED_NOTE,
    correctionPendingNotice: w16CorrectionPending
      ? "正式记录已更新，经营汇总将在数据追平后刷新。本页不会本地覆盖金额。"
      : undefined,
  }
}

export async function fetchCostEntryDetail(
  costEntryId: string
): Promise<CostEntryDetail | null> {
  await mockDelay(80)
  return W16_COST_ENTRIES.find((e) => e.costEntryId === costEntryId) ?? null
}

export async function fetchCostEntriesForRow(
  costEntryIds: readonly string[]
): Promise<CostEntryDetail[]> {
  await mockDelay(80)
  const set = new Set(costEntryIds)
  return W16_COST_ENTRIES.filter((e) => set.has(e.costEntryId))
}

export async function startProfitLossExport(input: {
  query: ProfitLossQuery
  view: Pick<
    ProfitLossView,
    | "period"
    | "scope"
    | "formulaVersion"
    | "freshness"
    | "rows"
    | "fieldPermissions"
  >
  coverage: ProfitLossQuery["coverage"]
}): Promise<ProfitLossExportJob> {
  await mockDelay(100)
  if (!input.view.fieldPermissions.canExport) {
    throw new Error("当前权限不允许导出")
  }
  if (!input.query.periodBasis) {
    throw new Error("periodBasis 未明确，已阻断导出")
  }
  const job = createW16ExportJob({
    periodFrom: input.view.period.from,
    periodTo: input.view.period.to,
    periodBasis: input.view.period.basis,
    formulaVersion: input.view.formulaVersion,
    coverage: input.coverage,
    scopeId: input.view.scope.id,
    scopeLabel: input.view.scope.label,
    permissionVersion: input.view.scope.permissionVersion,
    projectedAt: input.view.freshness.projectedAt,
    sourceWatermark: input.view.freshness.sourceWatermark,
    amountBasis: "NET",
    businessType: "GOODS_SERVICE",
    rowCount: input.view.rows.total,
  })
  return job
}

export async function fetchExportJob(
  jobId: string
): Promise<ProfitLossExportJob | null> {
  await mockDelay(40)
  return getW16ExportJob(jobId)
}

/** 演示：标记来源纠错后等待投影（不改本地金额） */
export async function markSourceCorrectionPending(): Promise<{
  notice: string
}> {
  await mockDelay(50)
  setW16CorrectionPending(true)
  return {
    notice:
      "正式记录已更新，经营汇总将在数据追平后刷新。本页不会本地覆盖金额。",
  }
}

export async function clearSourceCorrectionPending(): Promise<void> {
  await mockDelay(20)
  setW16CorrectionPending(false)
}
