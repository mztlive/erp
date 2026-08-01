/**
 * W25 商城消费订单 · session-mock API（queryFn / mutationFn）
 * 只读事实追溯 + 导出任务；无修改商城订单写操作。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  EmptyReason,
  ExportCommand,
  ExportJobResult,
  MallConsumptionOrderListQuery,
  MallConsumptionOrderListResult,
  MallConsumptionOrderMetric,
  MallConsumptionOrderRow,
  MallConsumptionOrderView,
  SalesOrderConsumptionSummary,
} from "@/features/mall-consumption-orders/types"
import {
  ATTRIBUTION_STATUS_LABEL,
  COST_BASIS_LABEL,
  FULFILLMENT_CHAIN_LABEL,
} from "@/features/mall-consumption-orders/types"
import {
  CONSUMPTION_ORDER_SEEDS,
  DATA_SCOPE_VERSION,
  FACT_WATERMARK,
  MALLS,
  PERMISSION_VERSION,
  SALES_ORDER_CONSUMPTION_SUMMARIES,
  getSeedById,
} from "@/mock/mall-consumption-orders"

const BOUNDARY_NOTICE =
  "W25 是由不可变关键事实形成的追溯视图，不是商城可变员工订单的实时副本，也不是第二个商城订单写入口。仅展示支付成功、取消、退款、完成、余额恢复五类结果事实。"

function nowIso() {
  return new Date().toISOString()
}

export async function fetchSalesOrderConsumptionSummary(
  salesOrderId: string
): Promise<SalesOrderConsumptionSummary> {
  await mockDelay()
  return (
    SALES_ORDER_CONSUMPTION_SUMMARIES[salesOrderId] ?? {
      salesOrderId,
      orderCount: 0,
      paidAmount: "0.00",
      refundedAmount: "0.00",
      restoredBalanceAmount: "0.00",
    }
  )
}

function matchSearch(q: string | undefined, parts: readonly string[]): boolean {
  if (!q?.trim()) return true
  const needle = q.trim().toLowerCase()
  return parts.some((p) => p.toLowerCase().includes(needle))
}

function hasAutoException(row: MallConsumptionOrderRow): boolean {
  return (
    row.fulfillmentChain === "ERP_AUTOMATED" &&
    (row.supplierOrderSummary.hasException ||
      row.attributionStatus === "DIFFERENCE")
  )
}

function computeMetrics(
  rows: readonly MallConsumptionOrderRow[]
): MallConsumptionOrderMetric[] {
  return [
    {
      key: "paid",
      label: "支付成功",
      value: rows.filter((r) =>
        r.factSummary.some((f) => f.factType === "PAYMENT_SUCCEEDED")
      ).length,
      detail: "有支付事实",
    },
    {
      key: "pending_attr",
      label: "待归集",
      value: rows.filter((r) => r.attributionStatus === "PENDING").length,
    },
    {
      key: "fact_diff",
      label: "事实差异",
      value: rows.filter((r) => r.attributionStatus === "DIFFERENCE").length,
    },
    {
      key: "auto_exception",
      label: "自动履约异常",
      value: rows.filter(hasAutoException).length,
    },
    {
      key: "cost_none",
      label: "成本未覆盖",
      value: rows.filter((r) =>
        r.costBasisBreakdown.some((b) => b.basis === "NONE")
      ).length,
    },
  ]
}

function applyFilters(
  rows: MallConsumptionOrderRow[],
  query: MallConsumptionOrderListQuery
): MallConsumptionOrderRow[] {
  let next = rows

  if (query.q?.trim()) {
    next = next.filter((r) =>
      matchSearch(query.q, [
        r.externalOrderNo,
        r.mallOrderId,
        r.customerLabel,
        r.mallName,
        r.customerId ?? "",
      ])
    )
  }

  if (query.mallIds?.length) {
    const set = new Set(query.mallIds)
    next = next.filter((r) => set.has(r.mallId))
  }

  if (query.fulfillmentChains?.length) {
    const set = new Set(query.fulfillmentChains)
    next = next.filter((r) => set.has(r.fulfillmentChain))
  }

  if (query.attributionStatuses?.length) {
    const set = new Set(query.attributionStatuses)
    next = next.filter((r) => set.has(r.attributionStatus))
  }

  if (query.paymentSources?.length) {
    next = next.filter((r) => {
      const card = Number(r.paymentComposition.cardAmount) > 0
      const wx = Number(r.paymentComposition.wechatAmount) > 0
      const kind =
        card && wx ? "MIXED" : card ? "CARD" : wx ? "WECHAT" : undefined
      return kind != null && query.paymentSources!.includes(kind)
    })
  }

  if (query.costBases?.length) {
    const set = new Set(query.costBases)
    next = next.filter((r) =>
      r.costBasisBreakdown.some((b) => set.has(b.basis))
    )
  }

  if (query.dataSources?.length) {
    const set = new Set(query.dataSources)
    next = next.filter((r) => {
      if (r.dataSource === "MIXED") {
        return set.has("REALTIME") || set.has("BACKFILL")
      }
      return set.has(r.dataSource as "REALTIME" | "BACKFILL")
    })
  }

  if (query.factTypes?.length) {
    const set = new Set(query.factTypes)
    next = next.filter((r) =>
      r.factSummary.some((f) => set.has(f.factType))
    )
  }

  if (query.supplierStatuses?.length) {
    const set = new Set(query.supplierStatuses)
    next = next.filter((r) =>
      r.supplierOrderSummary.statuses.some((s) => set.has(s))
    )
  }

  if (query.metric && query.metric !== "all") {
    switch (query.metric) {
      case "paid":
        next = next.filter((r) =>
          r.factSummary.some((f) => f.factType === "PAYMENT_SUCCEEDED")
        )
        break
      case "pending_attr":
        next = next.filter((r) => r.attributionStatus === "PENDING")
        break
      case "fact_diff":
        next = next.filter((r) => r.attributionStatus === "DIFFERENCE")
        break
      case "auto_exception":
        next = next.filter(hasAutoException)
        break
      case "cost_none":
        next = next.filter((r) =>
          r.costBasisBreakdown.some((b) => b.basis === "NONE")
        )
        break
      default:
        break
    }
  }

  // sort by paidAt desc default
  next = [...next].sort((a, b) => {
    const da = new Date(a.paidAt).getTime()
    const db = new Date(b.paidAt).getTime()
    return db - da
  })

  return next
}

function filterSummary(
  query: MallConsumptionOrderListQuery,
  total: number
): string {
  const parts: string[] = []
  if (query.metric && query.metric !== "all") {
    const labels: Record<string, string> = {
      paid: "支付成功",
      pending_attr: "待归集",
      fact_diff: "事实差异",
      auto_exception: "自动履约异常",
      cost_none: "成本未覆盖",
    }
    parts.push(labels[query.metric] ?? query.metric)
  }
  if (query.mallIds?.length) {
    parts.push(
      query.mallIds
        .map((id) => MALLS.find((m) => m.id === id)?.name ?? id)
        .join("/")
    )
  }
  if (query.fulfillmentChains?.length) {
    parts.push(
      query.fulfillmentChains.map((c) => FULFILLMENT_CHAIN_LABEL[c]).join("/")
    )
  }
  if (query.attributionStatuses?.length) {
    parts.push(
      query.attributionStatuses
        .map((s) => ATTRIBUTION_STATUS_LABEL[s])
        .join("/")
    )
  }
  if (query.costBases?.length) {
    parts.push(query.costBases.map((b) => COST_BASIS_LABEL[b]).join("/"))
  }
  if (query.paymentSources?.length) {
    const labels: Record<string, string> = {
      CARD: "卡券",
      WECHAT: "微信",
      MIXED: "组合支付",
    }
    parts.push(query.paymentSources.map((p) => labels[p] ?? p).join("/"))
  }
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  parts.push(`${total} 条`)
  return parts.join(" · ")
}

export async function fetchConsumptionOrderList(
  query: MallConsumptionOrderListQuery
): Promise<MallConsumptionOrderListResult> {
  await mockDelay()

  if (query.demoFlag === "no-permission") {
    return {
      rows: [],
      pageInfo: { page: 1, pageSize: query.pageSize ?? 8, total: 0 },
      metrics: [
        { key: "paid", label: "支付成功", value: 0 },
        { key: "pending_attr", label: "待归集", value: 0 },
        { key: "fact_diff", label: "事实差异", value: 0 },
        { key: "auto_exception", label: "自动履约异常", value: 0 },
        { key: "cost_none", label: "成本未覆盖", value: 0 },
      ],
      malls: [...MALLS],
      filterSummary: "无模块权限",
      emptyReason: "NO_PERMISSION",
      hasModulePermission: false,
      hasDataScope: false,
      permissionVersion: PERMISSION_VERSION,
      dataScopeVersion: DATA_SCOPE_VERSION,
      factWatermark: FACT_WATERMARK,
      queriedAt: nowIso(),
      boundaryNotice: BOUNDARY_NOTICE,
    }
  }

  if (query.demoFlag === "no-scope") {
    return {
      rows: [],
      pageInfo: { page: 1, pageSize: query.pageSize ?? 8, total: 0 },
      metrics: [
        { key: "paid", label: "支付成功", value: 0 },
        { key: "pending_attr", label: "待归集", value: 0 },
        { key: "fact_diff", label: "事实差异", value: 0 },
        { key: "auto_exception", label: "自动履约异常", value: 0 },
        { key: "cost_none", label: "成本未覆盖", value: 0 },
      ],
      malls: [...MALLS],
      filterSummary: "无数据范围",
      emptyReason: "NO_SCOPE",
      hasModulePermission: true,
      hasDataScope: false,
      permissionVersion: PERMISSION_VERSION,
      dataScopeVersion: DATA_SCOPE_VERSION,
      factWatermark: FACT_WATERMARK,
      queriedAt: nowIso(),
      boundaryNotice: BOUNDARY_NOTICE,
    }
  }

  if (query.demoFlag === "empty") {
    return {
      rows: [],
      pageInfo: { page: 1, pageSize: query.pageSize ?? 8, total: 0 },
      metrics: [
        { key: "paid", label: "支付成功", value: 0 },
        { key: "pending_attr", label: "待归集", value: 0 },
        { key: "fact_diff", label: "事实差异", value: 0 },
        { key: "auto_exception", label: "自动履约异常", value: 0 },
        { key: "cost_none", label: "成本未覆盖", value: 0 },
      ],
      malls: [...MALLS],
      filterSummary: "当前范围 0 条",
      emptyReason: "NO_DATA",
      hasModulePermission: true,
      hasDataScope: true,
      permissionVersion: PERMISSION_VERSION,
      dataScopeVersion: DATA_SCOPE_VERSION,
      factWatermark: FACT_WATERMARK,
      queriedAt: nowIso(),
      boundaryNotice: BOUNDARY_NOTICE,
    }
  }

  const allRows = CONSUMPTION_ORDER_SEEDS.map((s) => s.row)
  const metrics = computeMetrics(allRows)
  const filtered = applyFilters(allRows, query)

  const page = Math.max(1, query.page ?? 1)
  const pageSize = Math.max(1, query.pageSize ?? 8)
  const start = (page - 1) * pageSize
  const pageRows = filtered.slice(start, start + pageSize)

  let emptyReason: EmptyReason | undefined
  if (filtered.length === 0) {
    const hasFilter =
      Boolean(query.q?.trim()) ||
      Boolean(query.mallIds?.length) ||
      Boolean(query.fulfillmentChains?.length) ||
      Boolean(query.attributionStatuses?.length) ||
      Boolean(query.paymentSources?.length) ||
      Boolean(query.costBases?.length) ||
      Boolean(query.metric && query.metric !== "all") ||
      Boolean(query.factTypes?.length) ||
      Boolean(query.supplierStatuses?.length) ||
      Boolean(query.dataSources?.length)
    emptyReason = hasFilter ? "FILTER_EMPTY" : "NO_DATA"
  }

  return {
    rows: pageRows,
    pageInfo: { page, pageSize, total: filtered.length },
    metrics,
    malls: [...MALLS],
    filterSummary: filterSummary(query, filtered.length),
    emptyReason,
    hasModulePermission: true,
    hasDataScope: true,
    permissionVersion: PERMISSION_VERSION,
    dataScopeVersion: DATA_SCOPE_VERSION,
    factWatermark: FACT_WATERMARK,
    queriedAt: nowIso(),
    boundaryNotice: BOUNDARY_NOTICE,
  }
}

export async function fetchConsumptionOrderDetail(
  mallOrderId: string
): Promise<MallConsumptionOrderView | null> {
  await mockDelay()
  const seed = getSeedById(mallOrderId)
  if (!seed) return null
  return {
    ...seed.view,
    freshness: {
      ...seed.view.freshness,
      queriedAt: nowIso(),
    },
  }
}

export async function createConsumptionOrderExportJob(
  command: ExportCommand
): Promise<ExportJobResult> {
  await mockDelay(120)
  const jobId = `exp-w25-${command.requestId.slice(-8)}`
  const expiresAt = new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString()
  return {
    jobId,
    requestId: command.requestId,
    rowCount: command.rowCount,
    permissionVersion: PERMISSION_VERSION,
    fieldSetId: command.fieldSetId,
    maskDisclaimer:
      "导出使用服务端选择快照与字段权限掩码：地址、手机号、完整支付引用、卡号/卡密、未授权成本金额不会以明文写入文件。下载时重新鉴权。",
    expiresAt,
    downloadLabel: `商城消费订单_${jobId}.csv`,
    status: "succeeded",
  }
}
