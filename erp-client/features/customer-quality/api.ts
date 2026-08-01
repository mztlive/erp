/**
 * W15 session-mock API：queryFn / mutationFn 纯函数。
 * 投影字段由服务端返回；前端不重算盈亏/覆盖率/标签。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CustomerQualityExportJob,
  CustomerQualityMetric,
  CustomerQualityPeriodPolicy,
  CustomerQualityQuery,
  CustomerQualityRow,
  CustomerQualityScenario,
  CustomerQualityView,
} from "@/features/customer-quality/types"
import {
  BASE_METRICS,
  buildBaseView,
  CUSTOMER_QUALITY_ROWS,
  TAG_RULE_CATALOG,
} from "@/mock/customer-quality"

const PERMISSION_VERSION = "pv-w15-demo-1"
const DEFAULT_SCOPE = {
  id: "scope:team:sales-east",
  label: "华东销售团队（授权）",
  permissionVersion: PERMISSION_VERSION,
}

/** In-memory export jobs for this SPA session. */
const exportJobs = new Map<string, CustomerQualityExportJob>()

export async function fetchCustomerQualityPeriodPolicy(input?: {
  scenario?: CustomerQualityScenario
}): Promise<CustomerQualityPeriodPolicy> {
  await mockDelay(80)
  if (input?.scenario === "no_period_default") {
    return {
      hasDefault: false,
      timezone: "Asia/Shanghai",
      presets: [
        {
          id: "ytd",
          label: "本年至今",
          from: "2026-01-01",
          to: "2026-08-01",
        },
        {
          id: "q2",
          label: "2026 Q2",
          from: "2026-04-01",
          to: "2026-06-30",
        },
      ],
    }
  }
  return {
    hasDefault: true,
    from: "2026-01-01",
    to: "2026-08-01",
    periodBasis: "BUSINESS_DATE",
    timezone: "Asia/Shanghai",
    customerQualityPeriodPolicyId: "cq-period-policy-2026",
    customerQualityPeriodPolicyVersion: 3,
    selectionSource: "SERVER_DEFAULT",
    presets: [
      {
        id: "ytd",
        label: "本年至今（默认）",
        from: "2026-01-01",
        to: "2026-08-01",
      },
      {
        id: "q2",
        label: "2026 Q2",
        from: "2026-04-01",
        to: "2026-06-30",
      },
      {
        id: "rolling12",
        label: "近 12 个月",
        from: "2025-08-01",
        to: "2026-08-01",
      },
    ],
  }
}

function applyFieldDenial(
  metrics: readonly CustomerQualityMetric[]
): CustomerQualityMetric[] {
  return metrics.map((m) => {
    if (
      m.key === "actualProfitLossNet" ||
      m.key === "coveredNonVoucherNetRevenue"
    ) {
      return {
        ...m,
        value: "—",
        fieldDenied: true,
        reliability: "unavailable",
        explanation: "当前角色不可查看利润相关字段。",
      }
    }
    return m
  })
}

function filterRows(
  rows: readonly CustomerQualityRow[],
  query: CustomerQualityQuery
): CustomerQualityRow[] {
  let next = [...rows]

  if (query.customerId) {
    next = next.filter((r) => r.customerId === query.customerId)
  }
  if (query.q) {
    const q = query.q.trim().toLowerCase()
    next = next.filter(
      (r) =>
        r.customerNo.toLowerCase().includes(q) ||
        r.customerName.toLowerCase().includes(q)
    )
  }
  if (query.scaleTag) {
    next = next.filter((r) => r.scaleTierCode === query.scaleTag)
  }
  if (query.profitTag) {
    next = next.filter((r) => r.profitTierCode === query.profitTag)
  }
  if (query.riskTag) {
    next = next.filter((r) => r.riskTierCode === query.riskTag)
  }
  if (query.chartDimension === "scale" && query.chartCode) {
    next = next.filter((r) => r.scaleTierCode === query.chartCode)
  }
  if (query.chartDimension === "profit" && query.chartCode) {
    next = next.filter((r) => r.profitTierCode === query.chartCode)
  }
  if (query.chartDimension === "risk" && query.chartCode) {
    next = next.filter((r) => r.riskTierCode === query.chartCode)
  }
  if (query.businessType === "VOUCHER") {
    // 卡券筛选：仍展示规模/回款；利润字段在行上标不可用（服务端口径）
    next = next.map((r) => ({
      ...r,
      actualProfitLossNet: null,
      marginRate: null,
      costCoveredNetRevenue: null,
      costUncoveredNetRevenue: null,
      costCoverageRate: null,
    }))
  }
  if (query.fundsReview === "reviewed_only") {
    next = next.filter((r) => !r.cardFundsReviewInsufficient)
  }

  const sort = query.sort || "salesGrossAmount:desc"
  const [sortKey, sortDir] = sort.split(":")
  const dir = sortDir === "asc" ? 1 : -1
  next.sort((a, b) => {
    const av =
      sortKey === "actualProfitLossNet"
        ? Number(a.actualProfitLossNet ?? Number.NEGATIVE_INFINITY)
        : sortKey === "overdueGross"
          ? Number(a.overdueGross ?? 0)
          : sortKey === "costCoverageRate"
            ? Number(String(a.costCoverageRate ?? "0").replace("%", ""))
            : sortKey === "latestBusinessAt"
              ? Date.parse(a.latestBusinessAt ?? "") || 0
              : Number(a.salesGrossAmount)
    const bv =
      sortKey === "actualProfitLossNet"
        ? Number(b.actualProfitLossNet ?? Number.NEGATIVE_INFINITY)
        : sortKey === "overdueGross"
          ? Number(b.overdueGross ?? 0)
          : sortKey === "costCoverageRate"
            ? Number(String(b.costCoverageRate ?? "0").replace("%", ""))
            : sortKey === "latestBusinessAt"
              ? Date.parse(b.latestBusinessAt ?? "") || 0
              : Number(b.salesGrossAmount)
    return (av - bv) * dir
  })

  return next
}

function buildFilterSummary(query: CustomerQualityQuery): string {
  const parts = [
    `期间 ${query.from} ~ ${query.to}`,
    `范围 ${DEFAULT_SCOPE.label}`,
    query.fundsReview === "reviewed_only" ? "仅已复核票款" : "票款全部授权记录",
    query.businessType === "VOUCHER"
      ? "业务性质=卡券"
      : query.businessType === "GOODS_SERVICE"
        ? "业务性质=非卡券"
        : null,
    query.scaleTag ? `规模=${query.scaleTag}` : null,
    query.profitTag ? `利润=${query.profitTag}` : null,
    query.riskTag ? `风险=${query.riskTag}` : null,
    query.chartDimension && query.chartCode
      ? `图表=${query.chartDimension}:${query.chartCode}`
      : null,
    query.q ? `搜索=${query.q}` : null,
    query.customerId ? `聚焦客户=${query.customerId}` : null,
  ].filter(Boolean)
  return parts.join(" · ")
}

export async function fetchCustomerQuality(
  query: CustomerQualityQuery
): Promise<CustomerQualityView> {
  await mockDelay(160)

  const scenario = query.scenario ?? "default"
  const base = buildBaseView()

  if (scenario === "forbidden") {
    return {
      ...base,
      scope: DEFAULT_SCOPE,
      period: {
        from: query.from,
        to: query.to,
        basis: query.periodBasis,
        timezone: "Asia/Shanghai",
        selectionSource: query.periodSelectionSource,
        customerQualityPeriodPolicyId: query.customerQualityPeriodPolicyId,
        customerQualityPeriodPolicyVersion:
          query.customerQualityPeriodPolicyVersion,
      },
      metrics: [],
      customers: { items: [], total: 0, filteredTotal: 0 },
      filterSummary: buildFilterSummary(query),
      canExport: false,
      emptyKind: "forbidden",
      tagRuleCatalog: TAG_RULE_CATALOG,
    }
  }

  if (scenario === "no_scope") {
    return {
      ...base,
      scope: {
        id: "scope:none",
        label: "无授权数据范围",
        permissionVersion: PERMISSION_VERSION,
      },
      period: {
        from: query.from,
        to: query.to,
        basis: query.periodBasis,
        timezone: "Asia/Shanghai",
        selectionSource: query.periodSelectionSource,
        customerQualityPeriodPolicyId: query.customerQualityPeriodPolicyId,
        customerQualityPeriodPolicyVersion:
          query.customerQualityPeriodPolicyVersion,
      },
      metrics: BASE_METRICS.map((m) => ({
        ...m,
        value: "—",
        reliability: "unavailable" as const,
        explanation: "无数据范围，不展示公司级 0 指标。",
      })),
      customers: { items: [], total: 0, filteredTotal: 0 },
      filterSummary: buildFilterSummary(query),
      canExport: false,
      emptyKind: "no-scope",
      tagRuleCatalog: TAG_RULE_CATALOG,
    }
  }

  if (scenario === "empty") {
    return {
      ...base,
      scope: DEFAULT_SCOPE,
      period: {
        from: query.from,
        to: query.to,
        basis: query.periodBasis,
        timezone: "Asia/Shanghai",
        selectionSource: query.periodSelectionSource,
        customerQualityPeriodPolicyId: query.customerQualityPeriodPolicyId,
        customerQualityPeriodPolicyVersion:
          query.customerQualityPeriodPolicyVersion,
      },
      metrics: BASE_METRICS.map((m) => ({
        ...m,
        value:
          m.key === "salesOrderCount" || m.key === "exceptionCustomerCount"
            ? "0"
            : m.amountBasis
              ? "¥0.00"
              : "0",
        reliability: "reliable" as const,
        explanation: "期间内无授权经营记录。",
      })),
      customers: { items: [], total: 0, filteredTotal: 0 },
      filterSummary: buildFilterSummary(query),
      canExport: false,
      emptyKind: "no-data",
      tagRuleCatalog: TAG_RULE_CATALOG,
    }
  }

  let metrics = [...BASE_METRICS]
  if (query.businessType === "VOUCHER") {
    metrics = metrics.map((m) => {
      if (
        m.key === "actualProfitLossNet" ||
        m.key === "coveredNonVoucherNetRevenue"
      ) {
        return {
          ...m,
          value: "—",
          reliability: "unavailable" as const,
          explanation: "卡券实际经营结果请前往卡券经营分析；本页不显示 ¥0 或无穷大利润率。",
        }
      }
      return m
    })
  }
  if (scenario === "field_denied") {
    metrics = applyFieldDenial(metrics)
  }

  const allRows = CUSTOMER_QUALITY_ROWS
  const filtered = filterRows(allRows, query)

  let freshness = base.freshness
  if (scenario === "stale") {
    freshness = {
      ...freshness,
      state: "stale",
      projectedAt: "2026-08-01T08:50:00+08:00",
    }
  } else if (scenario === "rebuilding") {
    freshness = { ...freshness, state: "rebuilding" }
  } else if (scenario === "failed") {
    freshness = { ...freshness, state: "failed" }
  } else if (scenario === "refresh_failed") {
    freshness = { ...freshness, state: "stale", refreshFailed: true }
  }

  const emptyKind =
    allRows.length === 0
      ? ("no-data" as const)
      : filtered.length === 0
        ? ("filter" as const)
        : undefined

  return {
    ...base,
    freshness,
    scope: DEFAULT_SCOPE,
    period: {
      from: query.from,
      to: query.to,
      basis: query.periodBasis,
      timezone: "Asia/Shanghai",
      selectionSource: query.periodSelectionSource,
      customerQualityPeriodPolicyId: query.customerQualityPeriodPolicyId,
      customerQualityPeriodPolicyVersion:
        query.customerQualityPeriodPolicyVersion,
    },
    metrics,
    customers: {
      items: filtered,
      total: allRows.length,
      filteredTotal: filtered.length,
    },
    filterSummary: buildFilterSummary(query),
    canExport: true,
    emptyKind,
    tagRuleCatalog: TAG_RULE_CATALOG,
  }
}

export async function startCustomerQualityExport(input: {
  query: CustomerQualityQuery
  filterSummary: string
  projectionWatermark: string
  permissionVersion: string
  rowCount: number
}): Promise<CustomerQualityExportJob> {
  await mockDelay(120)
  const jobId = `EXP-W15-${Date.now().toString(36)}`
  const job: CustomerQualityExportJob = {
    jobId,
    status: "running",
    total: Math.max(1, input.rowCount),
    completed: 0,
    filterSummary: input.filterSummary,
    period: { from: input.query.from, to: input.query.to },
    permissionVersion: input.permissionVersion,
    projectionWatermark: input.projectionWatermark,
    amountBasisNote:
      "成交金额=含税(GROSS)；实际盈亏=不含税(NET)；卡券收入不进入实际盈亏列；缺失成本不写作 0。",
  }
  exportJobs.set(jobId, job)

  // Mock progress → succeeded with 7-day download
  queueMicrotask(() => {
    window.setTimeout(() => {
      const current = exportJobs.get(jobId)
      if (!current) return
      exportJobs.set(jobId, {
        ...current,
        status: "succeeded",
        completed: current.total,
        downloadLabel: `客户经营质量_${input.query.from}_${input.query.to}.csv`,
        expiresAt: new Date(Date.now() + 7 * 24 * 60 * 60 * 1000).toISOString(),
      })
    }, 600)
  })

  return job
}

export async function fetchCustomerQualityExportJob(
  jobId: string
): Promise<CustomerQualityExportJob | null> {
  await mockDelay(40)
  return exportJobs.get(jobId) ?? null
}
