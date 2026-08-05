/**
 * W28 卡券消费台账与经营分析 · 只读投影 seed。
 * - 企业卡券消费/成本不含微信支付（WECHAT）
 * - ACTUAL+STANDARD+NONE 消费金额合计 = 累计卡券消费
 * - NONE 不贡献成本 0；不进入利润
 */

import type {
  CardBusinessBreakdownItem,
  CardBusinessExportJob,
  CardBusinessRow,
  CardBusinessTrendPoint,
  ContributionTrendPoint,
  CostBasisSlice,
  DateBasisConfig,
} from "@/features/card-business-analytics/types"

export const W28_PERMISSION_VERSION = "pv-w28-demo-1"
export const W28_SCOPE_LABEL = "总部财务授权 · 卡券经营"
export const W28_THRESHOLD = "95.00%"

/** 累计卡券消费（含税）= ACTUAL + STANDARD + NONE 消费额 */
export const W28_TOTAL_CONSUMPTION_GROSS = "5240000.00"
export const W28_ACTUAL_CONSUMPTION_GROSS = "3563200.00" // 68.0%
export const W28_STANDARD_CONSUMPTION_GROSS = "1016560.00" // 19.4%
export const W28_NONE_CONSUMPTION_GROSS = "660240.00" // 12.6%
// 3563200 + 1016560 + 660240 = 5240000 ✓

export const W28_ACTUAL_COST_NET = "2890400.00"
export const W28_STANDARD_COST_NET = "812448.00"
/** 微信消费与成本（仅说明，不进入企业卡券指标） */
export const W28_WECHAT_CONSUMPTION_GROSS = "186400.00"
export const W28_WECHAT_COST_NET = "142200.00"

export const W28_WECHAT_EXCLUDED_NOTE =
  "微信支付消费与对应成本不进入本页企业卡券消费、覆盖率与利润指标；微信成本仍由供应商结算链路处理。本数据已排除微信支付消费 ¥186,400.00（含税）与成本 ¥142,200.00（不含税）。"

export const W28_DATE_BASIS_CONFIGURED: DateBasisConfig = {
  configuredDateBasis: "consumption",
  configurationVersion: "dbc-2026-07-01",
  allowedDateBases: [
    {
      code: "consumption",
      label: "消费发生日",
      explanation: "按卡券支付消费/退款发生日归属期间。",
    },
    {
      code: "sales",
      label: "销售发生日",
      explanation: "按企业卡券销售成交日归属期间。",
    },
    {
      code: "expiry",
      label: "履约到期日",
      explanation: "按销售单履约期限到期日归属，用于最终盈亏视角。",
    },
  ],
}

/** QA：?basisConfig=missing 模拟 Q2 未配置默认日期口径 */
export const W28_DATE_BASIS_UNCONFIGURED: DateBasisConfig = {
  configuredDateBasis: undefined,
  configurationVersion: "dbc-none",
  allowedDateBases: W28_DATE_BASIS_CONFIGURED.allowedDateBases,
}

export const W28_BY_BASIS: readonly CostBasisSlice[] = [
  {
    basis: "ACTUAL",
    consumptionGross: W28_ACTUAL_CONSUMPTION_GROSS,
    costNet: W28_ACTUAL_COST_NET,
    share: "0.6800",
    shareLabel: "68.0%",
  },
  {
    basis: "STANDARD",
    consumptionGross: W28_STANDARD_CONSUMPTION_GROSS,
    costNet: W28_STANDARD_COST_NET,
    share: "0.1940",
    shareLabel: "19.4%",
  },
  {
    basis: "NONE",
    consumptionGross: W28_NONE_CONSUMPTION_GROSS,
    // 故意不设 costNet：NONE 不得显示为 0 成本
    share: "0.1260",
    shareLabel: "12.6%",
  },
]

export const W28_CONSUMPTION_TREND: readonly CardBusinessTrendPoint[] = [
  {
    period: "W23",
    salesGross: "420000.00",
    consumptionGross: "380000.00",
    refundGross: "12000.00",
    balanceGross: "4100000.00",
  },
  {
    period: "W24",
    salesGross: "510000.00",
    consumptionGross: "450000.00",
    refundGross: "15000.00",
    balanceGross: "3950000.00",
  },
  {
    period: "W25",
    salesGross: "480000.00",
    consumptionGross: "510000.00",
    refundGross: "18000.00",
    balanceGross: "3800000.00",
  },
  {
    period: "W26",
    salesGross: "620000.00",
    consumptionGross: "550000.00",
    refundGross: "14000.00",
    balanceGross: "3680000.00",
  },
  {
    period: "W27",
    salesGross: "590000.00",
    consumptionGross: "600000.00",
    refundGross: "16000.00",
    balanceGross: "3520000.00",
  },
  {
    period: "W28",
    salesGross: "540000.00",
    consumptionGross: "580000.00",
    refundGross: "11000.00",
    balanceGross: "3360000.00",
  },
]

export const W28_CONTRIBUTION_TREND: readonly ContributionTrendPoint[] = [
  {
    period: "W23",
    marginNet: "72000.00",
    contributionNet: "98000.00",
    coverageRate: "86.2%",
    coveragePercent: 86.2,
  },
  {
    period: "W24",
    marginNet: "81000.00",
    contributionNet: "105000.00",
    coverageRate: "87.0%",
    coveragePercent: 87.0,
  },
  {
    period: "W25",
    marginNet: "76000.00",
    contributionNet: "99000.00",
    coverageRate: "85.5%",
    coveragePercent: 85.5,
  },
  {
    period: "W26",
    marginNet: "88000.00",
    contributionNet: "112000.00",
    coverageRate: "88.1%",
    coveragePercent: 88.1,
  },
  {
    period: "W27",
    marginNet: "91000.00",
    contributionNet: "118000.00",
    coverageRate: "87.8%",
    coveragePercent: 87.8,
  },
  {
    period: "W28",
    marginNet: "85000.00",
    contributionNet: "110000.00",
    coverageRate: "87.4%",
    coveragePercent: 87.4,
  },
]

export const W28_BY_CATEGORY: readonly CardBusinessBreakdownItem[] = [
  {
    id: "cat-meal",
    label: "餐饮卡",
    consumptionGross: "2148400.00",
    share: "41.0%",
  },
  {
    id: "cat-retail",
    label: "商超卡",
    consumptionGross: "1624400.00",
    share: "31.0%",
  },
  {
    id: "cat-digital",
    label: "数码权益",
    consumptionGross: "891000.00",
    share: "17.0%",
  },
  {
    id: "cat-other",
    label: "其他",
    consumptionGross: "576200.00",
    share: "11.0%",
  },
]

export const W28_BY_CUSTOMER: readonly CardBusinessBreakdownItem[] = [
  {
    id: "cust-lanwan",
    label: "蓝湾集团",
    consumptionGross: "1286000.00",
    share: "24.5%",
  },
  {
    id: "cust-qingyun",
    label: "青云科技",
    consumptionGross: "942000.00",
    share: "18.0%",
  },
  {
    id: "cust-haimo",
    label: "海墨实业",
    consumptionGross: "786000.00",
    share: "15.0%",
  },
  {
    id: "cust-other",
    label: "其他授权客户",
    consumptionGross: "2226000.00",
    share: "42.5%",
  },
]

/**
 * 明细行：成本口径按消费记录（非整张卡）；
 * 仅稳定卡实例引用摘要，无卡号/卡密/手机。
 */
export const W28_ROWS: readonly CardBusinessRow[] = [
  {
    rowId: "row-cb-001",
    customerId: "cust-lanwan",
    customerLabel: "蓝湾集团",
    salesOrderId: "so-card-008",
    salesOrderNo: "XS20260325008",
    voucherCategoryLabel: "餐饮卡",
    cardInstanceRef: "ci·a7f3…9c2e",
    consumptionGross: "286000.00",
    refundGross: "0.00",
    costBasis: "ACTUAL",
    costNet: "231800.00",
    coverageStatus: "covered",
    unconsumedBalanceGross: "214000.00",
    unfulfilledBalanceGross: "214000.00",
    consumptionOrderId: "mo-90881",
    consumptionOrderHref: "/commerce/consumption-orders/mo-90881?section=overview",
    supplierOrderHref: "/supplier-api/orders/sfo-fulfilling-01",
  },
  {
    rowId: "row-cb-002",
    customerId: "cust-qingyun",
    customerLabel: "青云科技",
    salesOrderId: "so-card-015",
    salesOrderNo: "XS20260412015",
    voucherCategoryLabel: "商超卡",
    cardInstanceRef: "ci·b1e8…4d71",
    consumptionGross: "158400.00",
    refundGross: "6200.00",
    costBasis: "STANDARD",
    costNet: "126720.00",
    coverageStatus: "covered",
    unconsumedBalanceGross: "98000.00",
    unfulfilledBalanceGross: "98000.00",
    riskLabel: "按历史有效供给价估算",
    consumptionOrderId: "mo-90012",
    consumptionOrderHref: "/commerce/consumption-orders/mo-90012?section=overview",
  },
  {
    rowId: "row-cb-003",
    customerId: "cust-haimo",
    customerLabel: "海墨实业",
    salesOrderId: "so-card-021",
    salesOrderNo: "XS20260508021",
    voucherCategoryLabel: "数码权益",
    cardInstanceRef: "ci·c9a2…1f55",
    consumptionGross: "96400.00",
    refundGross: "0.00",
    costBasis: "NONE",
    // costNet 故意缺省：NONE 不得显示 0
    coverageStatus: "none",
    unconsumedBalanceGross: "152000.00",
    unfulfilledBalanceGross: "152000.00",
    riskLabel: "无可用成本 · 利润不可靠",
    consumptionOrderId: "mo-77120",
    consumptionOrderHref: "/commerce/consumption-orders/mo-77120?section=overview",
  },
  {
    rowId: "row-cb-004",
    customerId: "cust-lanwan",
    customerLabel: "蓝湾集团",
    salesOrderId: "so-card-008",
    salesOrderNo: "XS20260325008",
    voucherCategoryLabel: "商超卡",
    cardInstanceRef: "ci·d4b6…8a03",
    consumptionGross: "210500.00",
    refundGross: "8500.00",
    costBasis: "ACTUAL",
    costNet: "168400.00",
    coverageStatus: "covered",
    unconsumedBalanceGross: "89000.00",
    unfulfilledBalanceGross: "89000.00",
    consumptionOrderId: "mo-88901",
    consumptionOrderHref: "/commerce/consumption-orders/mo-88901?section=overview",
    supplierOrderHref: "/supplier-api/orders/sfo-completed-partial-refund",
  },
  {
    rowId: "row-cb-005",
    customerId: "cust-qingyun",
    customerLabel: "青云科技",
    salesOrderId: "so-card-019",
    salesOrderNo: "XS20260601019",
    voucherCategoryLabel: "餐饮卡",
    cardInstanceRef: "ci·e2c7…6b19",
    consumptionGross: "72400.00",
    refundGross: "0.00",
    costBasis: "NONE",
    coverageStatus: "none",
    unconsumedBalanceGross: "128000.00",
    unfulfilledBalanceGross: "128000.00",
    riskLabel: "税口径不足 · 进入 NONE",
    consumptionOrderId: "mo-diff-8120",
    consumptionOrderHref: "/commerce/consumption-orders/mo-diff-8120?section=overview",
  },
  {
    rowId: "row-cb-006",
    customerId: "cust-haimo",
    customerLabel: "海墨实业",
    salesOrderId: "so-card-030",
    salesOrderNo: "XS20260628030",
    voucherCategoryLabel: "其他",
    cardInstanceRef: "ci·f8d1…0e44",
    consumptionGross: "134200.00",
    refundGross: "2100.00",
    costBasis: "STANDARD",
    costNet: "107360.00",
    coverageStatus: "covered",
    unconsumedBalanceGross: "45600.00",
    unfulfilledBalanceGross: "45600.00",
    riskLabel: "按历史有效供给价估算",
    consumptionOrderId: "mo-legacy-5520",
    consumptionOrderHref: "/commerce/consumption-orders/mo-legacy-5520?section=overview",
  },
]

// —— 导出任务（同步完成：创建即成功） ——
let exportSeq = 1

export function createW28ExportJob(input: {
  periodFrom: string
  periodTo: string
  dateBasis: CardBusinessExportJob["watermark"]["dateBasis"]
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
}): CardBusinessExportJob {
  const jobId = `w28-export-${exportSeq++}`
  const createdAt = "2026-08-01T09:40:00+08:00"
  return {
    jobId,
    status: "succeeded",
    total: 100,
    completed: 100,
    createdAt,
    downloadLabel: `卡券经营分析_${input.periodFrom}_${input.periodTo}.csv`,
    watermark: {
      periodFrom: input.periodFrom,
      periodTo: input.periodTo,
      dateBasis: input.dateBasis,
      filterSummary: input.filterSummary,
      coverageRate: input.coverageRate,
      projectionUpdatedAt: input.projectionUpdatedAt,
      consumedOutboxWatermark: input.consumedOutboxWatermark,
      balanceSnapshotAt: input.balanceSnapshotAt,
      lagSeconds: input.lagSeconds,
      permissionVersion: input.permissionVersion,
      taxDisclaimer: input.taxDisclaimer,
      wechatExcludedNote: input.wechatExcludedNote,
      rowCount: input.rowCount,
    },
  }
}
