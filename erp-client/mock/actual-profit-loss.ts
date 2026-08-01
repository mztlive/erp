/**
 * W16 实际经营盈亏 · 只读投影 seed。
 * 仅含 NON_VOUCHER_FULFILLMENT 的 ACTUAL/REDUCTION 计入实际值；
 * EXPECTED/CONFIRMED 仅作对照；不含卡券收入与 WECHAT/MALL 消费成本。
 */

import type {
  CostEntryDetail,
  ProfitLossPeriodBasisConfig,
  ProfitLossRow,
  ProfitLossTrendPoint,
  ProfitLossCostComposition,
  StageReferenceLine,
} from "@/features/actual-profit-loss/types"

export const W16_FORMULA_VERSION = "pl-formula-v1.2.0"
export const W16_PERMISSION_VERSION = "pv-w16-demo-1"
export const W16_SCOPE = {
  id: "org-hq-finance",
  label: "总部财务授权范围",
} as const

export const W16_EXCLUDED_NOTE =
  "本页仅汇总非卡券业务（GOODS_SERVICE）不含税口径。卡券销售收入、卡券直接履约费用、商城消费成本（MALL_CONSUMPTION）与微信成本（WECHAT_COST）均不进入 W16；卡券消费经营结果见 W28。"

export const W16_FORMULA_TEXT =
  "实际经营盈亏（不含税）= 非卡券不含税销售收入 − 非卡券不含税实际采购成本 − 非卡券不含税实际履约费用（仅 cost_scope=NON_VOUCHER_FULFILLMENT 且 stage∈{ACTUAL,REDUCTION}）"

/** 默认：服务端已配置期间归属口径 */
export const W16_PERIOD_BASIS_CONFIGURED: ProfitLossPeriodBasisConfig = {
  configuredPeriodBasis: "sales_revenue_recognition_date",
  configurationVersion: "pbc-2026-07-01",
  allowedPeriodBases: [
    {
      code: "sales_revenue_recognition_date",
      label: "销售收入确认日",
      explanation: "订单维度按销售收入确认口径归属期间；成本按销售单归属并展示实际发生日。",
    },
    {
      code: "sales_order_effective_date",
      label: "销售单生效日",
      explanation: "按销售单正式生效日归属期间。",
    },
    {
      code: "fulfillment_complete_date",
      label: "履约完成日",
      explanation: "按履约终态完成日归属期间。",
    },
    {
      code: "cost_occurred_date",
      label: "成本发生日",
      explanation: "按实际成本发生日归属期间。",
    },
  ],
}

/** QA：未配置正式口径时阻断查询 */
export const W16_PERIOD_BASIS_UNCONFIGURED: ProfitLossPeriodBasisConfig = {
  configuredPeriodBasis: undefined,
  configurationVersion: "pbc-none",
  allowedPeriodBases: W16_PERIOD_BASIS_CONFIGURED.allowedPeriodBases,
}

export const W16_COST_ENTRIES: readonly CostEntryDetail[] = [
  {
    costEntryId: "ce-act-001",
    costType: "GOODS",
    costTypeLabel: "商品",
    stage: "ACTUAL",
    stageLabel: "实际成本",
    costScope: "NON_VOUCHER_FULFILLMENT",
    costScopeLabel: "非卡券履约",
    supplierId: "sup-01",
    supplierName: "华东供应链有限公司",
    amountGross: "452000.00",
    taxRate: "13%",
    taxAmount: "52000.00",
    amountNet: "400000.00",
    occurredAt: "2026-07-18T14:20:00+08:00",
    sourceType: "PURCHASE_RECEIPT",
    sourceTypeLabel: "采购入库",
    sourceDocumentId: "pr-20260718-01",
    sourceDocumentNo: "RK20260718001",
    sourceLineId: "prl-01",
    sourceLineLabel: "行 1 · 定制礼盒 A",
    sourceVersion: "v3",
    salesOrderId: "so-nv-001",
    salesOrderNo: "XS20260715001",
    salesOrderLineId: "sol-001-1",
    salesOrderLineLabel: "定制礼盒 A × 200",
    voucherSummary: "凭证摘要授权可见 · 借：库存商品 400,000.00",
    correctionHref: "/procurement/orders/po-20260710-01",
    correctionLabel: "打开采购单（正式变更/冲减）",
  },
  {
    costEntryId: "ce-act-002",
    costType: "LOGISTICS",
    costTypeLabel: "物流",
    stage: "ACTUAL",
    stageLabel: "实际成本",
    costScope: "NON_VOUCHER_FULFILLMENT",
    costScopeLabel: "非卡券履约",
    supplierId: "sup-log-02",
    supplierName: "顺捷物流",
    amountGross: "22600.00",
    taxRate: "6%",
    taxAmount: "1280.00",
    amountNet: "21320.00",
    occurredAt: "2026-07-22T09:05:00+08:00",
    sourceType: "FULFILLMENT_SHIP",
    sourceTypeLabel: "履约发运",
    sourceDocumentId: "ff-ship-778",
    sourceDocumentNo: "FY20260722012",
    sourceVersion: "v1",
    salesOrderId: "so-nv-001",
    salesOrderNo: "XS20260715001",
    salesOrderLineId: "sol-001-1",
    salesOrderLineLabel: "定制礼盒 A × 200",
    voucherSummary: "物流费不含税 21,320.00",
    correctionHref: "/fulfillment?salesOrderId=so-nv-001",
    correctionLabel: "打开履约作业",
  },
  {
    costEntryId: "ce-red-001",
    costType: "REBATE",
    costTypeLabel: "返点",
    stage: "REDUCTION",
    stageLabel: "成本冲减",
    costScope: "NON_VOUCHER_FULFILLMENT",
    costScopeLabel: "非卡券履约",
    supplierId: "sup-01",
    supplierName: "华东供应链有限公司",
    amountGross: "-11300.00",
    taxRate: "13%",
    taxAmount: "-1300.00",
    amountNet: "-10000.00",
    occurredAt: "2026-07-28T16:40:00+08:00",
    sourceType: "PURCHASE_REBATE",
    sourceTypeLabel: "采购返点",
    sourceDocumentId: "rebate-09",
    sourceDocumentNo: "FD20260728003",
    sourceVersion: "v1",
    salesOrderId: "so-nv-001",
    salesOrderNo: "XS20260715001",
    originalCostEntryId: "ce-act-001",
    originalCostEntryLabel: "ce-act-001 · 商品实际成本",
    voucherSummary: "返点冲减不含税 10,000.00（反向贡献）",
  },
  {
    costEntryId: "ce-act-003",
    costType: "PRINT",
    costTypeLabel: "印刷",
    stage: "ACTUAL",
    stageLabel: "实际成本",
    costScope: "NON_VOUCHER_FULFILLMENT",
    costScopeLabel: "非卡券履约",
    supplierId: "sup-print-03",
    supplierName: "明华印刷",
    amountGross: "31800.00",
    taxRate: "13%",
    taxAmount: "3660.00",
    amountNet: "28140.00",
    occurredAt: "2026-07-25T11:10:00+08:00",
    sourceType: "FULFILLMENT_SERVICE",
    sourceTypeLabel: "履约服务",
    sourceDocumentId: "ff-svc-441",
    sourceDocumentNo: "FW20260725008",
    sourceVersion: "v2",
    salesOrderId: "so-nv-002",
    salesOrderNo: "XS20260718002",
    salesOrderLineId: "sol-002-1",
    salesOrderLineLabel: "员工节日礼包印刷件",
    voucherSummary: "印刷费不含税 28,140.00",
    correctionHref: "/fulfillment?salesOrderId=so-nv-002",
    correctionLabel: "打开履约作业",
  },
  {
    costEntryId: "ce-act-004",
    costType: "WAREHOUSE",
    costTypeLabel: "仓储",
    stage: "ACTUAL",
    stageLabel: "实际成本",
    costScope: "NON_VOUCHER_FULFILLMENT",
    costScopeLabel: "非卡券履约",
    amountGross: "5600.00",
    taxRate: "6%",
    taxAmount: "317.00",
    amountNet: "5283.00",
    occurredAt: "2026-07-30T08:00:00+08:00",
    sourceType: "WAREHOUSE_FEE",
    sourceTypeLabel: "仓储费用",
    sourceDocumentId: "wh-fee-12",
    sourceDocumentNo: "CC20260730001",
    sourceVersion: "v1",
    salesOrderId: "so-nv-002",
    salesOrderNo: "XS20260718002",
  },
  {
    costEntryId: "ce-act-005",
    costType: "DELIVERY",
    costTypeLabel: "配送",
    stage: "ACTUAL",
    stageLabel: "实际成本",
    costScope: "NON_VOUCHER_FULFILLMENT",
    costScopeLabel: "非卡券履约",
    supplierId: "sup-del-05",
    supplierName: "城配快达",
    amountGross: "8900.00",
    taxRate: "6%",
    taxAmount: "504.00",
    amountNet: "8396.00",
    occurredAt: "2026-08-01T07:30:00+08:00",
    sourceType: "FULFILLMENT_DELIVERY",
    sourceTypeLabel: "终端配送",
    sourceDocumentId: "del-90",
    sourceDocumentNo: "PS20260801004",
    sourceVersion: "v1",
    salesOrderId: "so-nv-003",
    salesOrderNo: "XS20260722003",
  },
]

/** 销售单维度明细（服务端已投影；金额均为不含税字符串） */
export const W16_SALES_ORDER_ROWS: readonly ProfitLossRow[] = [
  {
    rowId: "row-so-001",
    objectType: "sales_order",
    objectId: "so-nv-001",
    identityLabel: "XS20260715001",
    customerId: "cust-01",
    customerLabel: "星河制造股份有限公司",
    benefitScenarios: ["员工福利", "节日礼赠"],
    fulfillmentModes: ["公司仓发"],
    netSalesRevenue: "680000.00",
    actualProcurementCostNet: "400000.00",
    actualFulfillmentCostNet: "21320.00",
    reductionsNet: "-10000.00",
    actualProfitLossNet: "268680.00",
    marginRate: "39.51%",
    coverageState: "COVERED",
    coverageBlockers: [],
    latestCostOccurredAt: "2026-07-28T16:40:00+08:00",
    allowedDrilldowns: ["sales_order", "cost_entry"],
    costEntryIds: ["ce-act-001", "ce-act-002", "ce-red-001"],
  },
  {
    rowId: "row-so-002",
    objectType: "sales_order",
    objectId: "so-nv-002",
    identityLabel: "XS20260718002",
    customerId: "cust-02",
    customerLabel: "远航科技集团",
    benefitScenarios: ["客户答谢"],
    fulfillmentModes: ["供应商直发", "服务"],
    netSalesRevenue: "320000.00",
    actualProcurementCostNet: "0.00",
    actualFulfillmentCostNet: "33423.00",
    reductionsNet: "0.00",
    actualProfitLossNet: "286577.00",
    marginRate: "89.56%",
    coverageState: "PARTIAL",
    coverageBlockers: [
      {
        code: "MISSING_GOODS_COST",
        message: "商品采购实际成本尚未形成（待入库/待复核）",
      },
      {
        code: "PENDING_COST_TYPE",
        message: "待补记录类型：商品 ACTUAL",
      },
    ],
    latestCostOccurredAt: "2026-07-30T08:00:00+08:00",
    allowedDrilldowns: ["sales_order", "cost_entry"],
    costEntryIds: ["ce-act-003", "ce-act-004"],
  },
  {
    rowId: "row-so-003",
    objectType: "sales_order",
    objectId: "so-nv-003",
    identityLabel: "XS20260722003",
    customerId: "cust-03",
    customerLabel: "青云贸易有限公司",
    benefitScenarios: ["渠道激励"],
    fulfillmentModes: ["公司仓发", "配送"],
    netSalesRevenue: "150000.00",
    // 完全未覆盖：不返回零成本利润
    actualProcurementCostNet: undefined,
    actualFulfillmentCostNet: undefined,
    reductionsNet: undefined,
    actualProfitLossNet: undefined,
    marginRate: undefined,
    marginUnavailableReason: "成本完全未覆盖，不生成零成本利润",
    coverageState: "UNCOVERED",
    coverageBlockers: [
      {
        code: "NO_ACTUAL_COST",
        message: "尚无 ACTUAL/REDUCTION 成本记录归属本销售单",
      },
      {
        code: "PENDING_FULFILLMENT",
        message: "待补记录类型：采购入库、履约发运",
      },
    ],
    latestCostOccurredAt: undefined,
    allowedDrilldowns: ["sales_order"],
    costEntryIds: [],
  },
  {
    rowId: "row-so-004",
    objectType: "sales_order",
    objectId: "so-nv-004",
    identityLabel: "XS20260725004",
    customerId: "cust-01",
    customerLabel: "星河制造股份有限公司",
    benefitScenarios: ["内部活动"],
    fulfillmentModes: ["电子"],
    // 净收入为 0：分母为零 → 利润率不适用
    netSalesRevenue: "0.00",
    actualProcurementCostNet: "1200.00",
    actualFulfillmentCostNet: "0.00",
    reductionsNet: "0.00",
    actualProfitLossNet: "-1200.00",
    marginRate: undefined,
    marginUnavailableReason: "适用不含税收入为 0，利润率不适用",
    coverageState: "COVERED",
    coverageBlockers: [],
    latestCostOccurredAt: "2026-07-26T10:00:00+08:00",
    allowedDrilldowns: ["sales_order", "cost_entry"],
    costEntryIds: [],
  },
  {
    rowId: "row-so-005",
    objectType: "sales_order",
    objectId: "so-nv-005",
    identityLabel: "XS20260728005",
    customerId: "cust-04",
    customerLabel: "瀚海实业",
    benefitScenarios: ["员工福利"],
    fulfillmentModes: ["供应商直发"],
    netSalesRevenue: "210000.00",
    actualProcurementCostNet: "168000.00",
    actualFulfillmentCostNet: "8400.00",
    reductionsNet: "0.00",
    actualProfitLossNet: "33600.00",
    marginRate: "16.00%",
    coverageState: "COVERED",
    coverageBlockers: [],
    latestCostOccurredAt: "2026-07-29T15:20:00+08:00",
    allowedDrilldowns: ["sales_order", "cost_entry"],
    costEntryIds: [],
  },
]

export const W16_TREND: readonly ProfitLossTrendPoint[] = [
  {
    period: "2026-03",
    netSalesRevenue: "980000.00",
    actualCostNet: "720000.00",
    actualProfitLossNet: "260000.00",
    reliability: "reliable",
  },
  {
    period: "2026-04",
    netSalesRevenue: "1050000.00",
    actualCostNet: "780000.00",
    actualProfitLossNet: "270000.00",
    reliability: "reliable",
  },
  {
    period: "2026-05",
    netSalesRevenue: "1120000.00",
    actualCostNet: "810000.00",
    actualProfitLossNet: "310000.00",
    reliability: "reliable",
  },
  {
    period: "2026-06",
    netSalesRevenue: "1180000.00",
    actualCostNet: "850000.00",
    actualProfitLossNet: "330000.00",
    reliability: "reliable",
  },
  {
    period: "2026-07",
    netSalesRevenue: "1360000.00",
    actualCostNet: "632343.00",
    actualProfitLossNet: "727657.00",
    reliability: "partial",
  },
  {
    period: "2026-08",
    netSalesRevenue: "150000.00",
    actualCostNet: "8396.00",
    actualProfitLossNet: undefined,
    reliability: "partial",
  },
]

export const W16_COST_COMPOSITION: readonly ProfitLossCostComposition[] = [
  { costType: "GOODS", label: "商品", netAmount: "568000.00", share: "88.7%" },
  { costType: "LOGISTICS", label: "物流", netAmount: "21320.00", share: "3.3%" },
  { costType: "PRINT", label: "印刷", netAmount: "28140.00", share: "4.4%" },
  { costType: "WAREHOUSE", label: "仓储", netAmount: "5283.00", share: "0.8%" },
  { costType: "DELIVERY", label: "配送", netAmount: "8396.00", share: "1.3%" },
  { costType: "PLATFORM", label: "平台/技术服务", netAmount: "0.00", share: "0.0%" },
  { costType: "OFFLINE_SERVICE", label: "线下服务", netAmount: "0.00", share: "0.0%" },
  {
    costType: "REBATE",
    label: "返点（冲减）",
    netAmount: "-10000.00",
    share: "-1.6%",
  },
  { costType: "OTHER", label: "其他", netAmount: "8400.00", share: "1.3%" },
]

/** 预计/已确认仅作对照，不进入实际盈亏 */
export const W16_STAGE_REFERENCE: readonly StageReferenceLine[] = [
  {
    stage: "EXPECTED",
    label: "预计成本（EXPECTED）",
    procurementCostNet: "590000.00",
    fulfillmentCostNet: "72000.00",
    totalNet: "662000.00",
    note: "执行期对照，不参与实际经营盈亏或利润率。",
  },
  {
    stage: "CONFIRMED",
    label: "已确认成本（CONFIRMED）",
    procurementCostNet: "575000.00",
    fulfillmentCostNet: "68000.00",
    totalNet: "643000.00",
    note: "执行期对照，不参与实际经营盈亏或利润率。",
  },
]

/** 演示：来源纠错后投影待追平（不本地改金额） */
export let w16CorrectionPending = false

export function setW16CorrectionPending(value: boolean) {
  w16CorrectionPending = value
}

/** 导出任务 session 态 */
export type W16ExportJobRecord = {
  jobId: string
  status: "queued" | "running" | "succeeded" | "failed"
  total: number
  completed: number
  createdAt: string
  downloadLabel?: string
  watermark: {
    periodFrom: string
    periodTo: string
    periodBasis: string
    formulaVersion: string
    coverage: "covered" | "uncovered" | "all"
    scopeId: string
    scopeLabel: string
    permissionVersion: string
    projectedAt: string
    sourceWatermark: string
    amountBasis: "NET"
    businessType: "GOODS_SERVICE"
    rowCount: number
  }
}

const exportJobs = new Map<string, W16ExportJobRecord>()
let exportSeq = 0

export function createW16ExportJob(
  watermark: W16ExportJobRecord["watermark"]
): W16ExportJobRecord {
  const jobId = `exp-w16-${++exportSeq}`
  const job: W16ExportJobRecord = {
    jobId,
    status: "queued",
    total: watermark.rowCount,
    completed: 0,
    createdAt: new Date().toISOString(),
    watermark,
  }
  exportJobs.set(jobId, job)
  globalThis.setTimeout(() => {
    const current = exportJobs.get(jobId)
    if (!current) return
    exportJobs.set(jobId, {
      ...current,
      status: "running",
      completed: Math.max(1, Math.ceil(current.total / 2)),
    })
  }, 400)
  globalThis.setTimeout(() => {
    const current = exportJobs.get(jobId)
    if (!current) return
    exportJobs.set(jobId, {
      ...current,
      status: "succeeded",
      completed: current.total,
      downloadLabel: `实际盈亏导出-非卡券不含税-${jobId}.csv`,
    })
  }, 1200)
  return job
}

export function getW16ExportJob(jobId: string): W16ExportJobRecord | null {
  return exportJobs.get(jobId) ?? null
}
