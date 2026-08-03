/**
 * W23 执行投影演示种子。
 * 仅含服务端投影修订白名单字段；不含成交金额、配赠、税率、开票、应收、玩法规则。
 */

import type {
  DeliveryStatus,
  ExecutionProjectionRow,
  ProjectionSource,
  ProjectionWhitelistContent,
  ReconciliationStatus,
  LatencyBand,
} from "@/features/execution-projections/types"
import {
  DELIVERY_STATUS_LABEL,
  DELIVERY_STATUS_TONE,
} from "@/features/execution-projections/types"
import type { StatusTone } from "@/components/ui/status-badge"

export type ProjectionSeed = {
  projectionId: string
  projectionNo: string
  projectionRevisionId: string
  projectionRevisionNo: number
  projectionSource: ProjectionSource
  salesOrderId: string
  salesOrderNo: string
  salesOrderRevisionId: string
  salesOrderRevisionNo: number
  /** W05 当前销售版本（历史投影不得被此覆盖） */
  w05CurrentSalesRevisionNo: number
  salesOrderStatus: string
  salesOrderStatusTone: StatusTone
  customerLabel: string
  targetMallId: string
  targetMallName: string
  currentAckedRevisionNo?: number
  deliveryStatus: DeliveryStatus
  attemptCount: number
  lastAttemptAt?: string
  nextAttemptAt?: string
  mallAckAt?: string
  mallExecutionBaseline?: string
  errorCode?: string
  errorSummary?: string
  workItemId?: string
  errorTaskId?: string
  latencyBand: LatencyBand
  reconciliationStatus: ReconciliationStatus
  pendingDurationLabel: string
  ownerLabel: string
  content: ProjectionWhitelistContent
  /** 历史修订（含来源销售版本） */
  history?: Array<{
    projectionRevisionId: string
    projectionRevisionNo: number
    salesOrderRevisionId: string
    salesOrderRevisionNo: number
    deliveryStatus: DeliveryStatus
    mallAckAt?: string
    content: ProjectionWhitelistContent
  }>
  allowedActions: string[]
  actionBlockers: Array<{ action: string; code: string; message: string }>
}

function content(
  partial: Partial<ProjectionWhitelistContent> &
    Pick<
      ProjectionWhitelistContent,
      | "customerExternalIdentity"
      | "voucherCategoryExternalIdentity"
      | "voucherCategoryErpName"
      | "faceValue"
      | "cardCount"
    >
): ProjectionWhitelistContent {
  return {
    customerExternalIdentityCopyable: false,
    voucherExpiryAt: "2028-06-01 23:59:59",
    cardForm: "电子卡",
    effectiveAt: "2026-07-28 10:12:00",
    contentHash: "sha256:a1b2c3d4e5f67890abcdef",
    ...partial,
  }
}

export const EXECUTION_PROJECTION_SEEDS: readonly ProjectionSeed[] = [
  {
    projectionId: "xp_110",
    projectionNo: "XP-20260801-110",
    projectionRevisionId: "xpr_110_v3",
    projectionRevisionNo: 3,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1013",
    salesOrderNo: "XS20260329001",
    salesOrderRevisionId: "sor_1013_v3",
    salesOrderRevisionNo: 3,
    w05CurrentSalesRevisionNo: 3,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "星河制造股份有限公司",
    targetMallId: "mall_hd",
    targetMallName: "华东商城",
    currentAckedRevisionNo: 3,
    deliveryStatus: "ACKED",
    attemptCount: 1,
    lastAttemptAt: "2026-07-28 10:14:22",
    mallAckAt: "2026-07-28 10:14:41",
    mallExecutionBaseline: "mall-exec·a91c…e2",
    latencyBand: "normal",
    reconciliationStatus: "MATCHED",
    pendingDurationLabel: "—",
    ownerLabel: "自动重试",
    content: content({
      customerExternalIdentity: "ext·星河·7f2a",
      voucherCategoryExternalIdentity: "cat·中国通·m9",
      voucherCategoryErpName: "中国通",
      faceValue: "1000.00",
      cardCount: "800",
      contentHash: "sha256:c0de11aa…",
    }),
    history: [
      {
        projectionRevisionId: "xpr_110_v2",
        projectionRevisionNo: 2,
        salesOrderRevisionId: "sor_1013_v2",
        salesOrderRevisionNo: 2,
        deliveryStatus: "ACKED",
        mallAckAt: "2026-07-20 09:01:00",
        content: content({
          customerExternalIdentity: "ext·星河·7f2a",
          voucherCategoryExternalIdentity: "cat·中国通·m9",
          voucherCategoryErpName: "中国通",
          faceValue: "1000.00",
          cardCount: "500",
          effectiveAt: "2026-07-15 11:00:00",
          contentHash: "sha256:bb22…v2",
        }),
      },
      {
        projectionRevisionId: "xpr_110_v1",
        projectionRevisionNo: 1,
        salesOrderRevisionId: "sor_1013_v1",
        salesOrderRevisionNo: 1,
        deliveryStatus: "ACKED",
        mallAckAt: "2026-07-10 16:22:00",
        content: content({
          customerExternalIdentity: "ext·星河·7f2a",
          voucherCategoryExternalIdentity: "cat·中国通·m9",
          voucherCategoryErpName: "中国通",
          faceValue: "1000.00",
          cardCount: "300",
          effectiveAt: "2026-07-10 15:00:00",
          contentHash: "sha256:aa11…v1",
        }),
      },
    ],
    allowedActions: [],
    actionBlockers: [
      {
        action: "RETRY",
        code: "ALREADY_ACKED",
        message: "商城已确认，无需重试。",
      },
      {
        action: "QUERY_RESULT",
        code: "ALREADY_ACKED",
        message: "已有明确确认结果。",
      },
    ],
  },
  {
    projectionId: "xp_118",
    projectionNo: "XP-20260801-118",
    projectionRevisionId: "xpr_118_v1",
    projectionRevisionNo: 1,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1003",
    salesOrderNo: "XS20260326007",
    salesOrderRevisionId: "sor_1001_v2",
    salesOrderRevisionNo: 2,
    w05CurrentSalesRevisionNo: 2,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "星河制造股份有限公司",
    targetMallId: "mall_hd",
    targetMallName: "华东商城",
    deliveryStatus: "SENDING",
    attemptCount: 1,
    lastAttemptAt: "2026-08-01 09:30:00",
    nextAttemptAt: "2026-08-01 09:45:00",
    latencyBand: "normal",
    reconciliationStatus: "NONE",
    pendingDurationLabel: "12 分钟",
    ownerLabel: "自动重试",
    content: content({
      customerExternalIdentity: "ext·星河·7f2a",
      voucherCategoryExternalIdentity: "cat·福利卡·k2",
      voucherCategoryErpName: "企业福利卡",
      faceValue: "500.00",
      cardCount: "200",
      cardForm: "实体卡",
    }),
    allowedActions: ["QUERY_RESULT"],
    actionBlockers: [
      {
        action: "RETRY",
        code: "IN_FLIGHT",
        message: "正在发送中，请勿重复操作。",
      },
    ],
  },
  {
    projectionId: "xp_121",
    projectionNo: "XP-20260801-121",
    projectionRevisionId: "xpr_121_v1",
    projectionRevisionNo: 1,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1006",
    salesOrderNo: "XS20260323009",
    salesOrderRevisionId: "sor_1020_v1",
    salesOrderRevisionNo: 1,
    w05CurrentSalesRevisionNo: 1,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "云帆贸易有限公司",
    targetMallId: "mall_hn",
    targetMallName: "华南商城",
    deliveryStatus: "UNKNOWN",
    attemptCount: 2,
    lastAttemptAt: "2026-08-01 08:10:00",
    latencyBand: "over_sla",
    reconciliationStatus: "NONE",
    pendingDurationLabel: "1 小时 26 分",
    ownerLabel: "运营协同",
    content: content({
      customerExternalIdentity: "ext·云帆·3c1e",
      voucherCategoryExternalIdentity: "cat·油卡·p1",
      voucherCategoryErpName: "中石化油卡",
      faceValue: "200.00",
      cardCount: "1000",
    }),
    allowedActions: ["QUERY_RESULT", "ESCALATE"],
    actionBlockers: [
      {
        action: "RETRY",
        code: "RESULT_UNKNOWN",
        message: "结果未知须先查询最终结果，未明确前不得重试或标为成功。",
      },
    ],
  },
  {
    projectionId: "xp_130",
    projectionNo: "XP-20260801-130",
    projectionRevisionId: "xpr_130_v2",
    projectionRevisionNo: 2,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1009",
    salesOrderNo: "XS20260318022",
    salesOrderRevisionId: "sor_1021_v2",
    salesOrderRevisionNo: 2,
    w05CurrentSalesRevisionNo: 2,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "启明教育集团",
    targetMallId: "mall_hd",
    targetMallName: "华东商城",
    currentAckedRevisionNo: 1,
    deliveryStatus: "FAILED",
    attemptCount: 3,
    lastAttemptAt: "2026-08-01 07:55:12",
    nextAttemptAt: undefined,
    errorCode: "MALL_MAPPING_REJECTED",
    errorSummary: "商城拒绝：类目映射失效（脱敏摘要）",
    latencyBand: "over_sla",
    reconciliationStatus: "VERSION_MISMATCH",
    pendingDurationLabel: "2 小时 5 分",
    ownerLabel: "运营协同",
    content: content({
      customerExternalIdentity: "ext·启明·91ab",
      voucherCategoryExternalIdentity: "cat·图书卡·t4",
      voucherCategoryErpName: "图书提货卡",
      faceValue: "100.00",
      cardCount: "500",
      effectiveAt: "2026-07-30 14:20:00",
    }),
    history: [
      {
        projectionRevisionId: "xpr_130_v1",
        projectionRevisionNo: 1,
        salesOrderRevisionId: "sor_1021_v1",
        salesOrderRevisionNo: 1,
        deliveryStatus: "ACKED",
        mallAckAt: "2026-07-25 11:00:00",
        content: content({
          customerExternalIdentity: "ext·启明·91ab",
          voucherCategoryExternalIdentity: "cat·图书卡·t4",
          voucherCategoryErpName: "图书提货卡",
          faceValue: "100.00",
          cardCount: "300",
          effectiveAt: "2026-07-25 10:00:00",
          contentHash: "sha256:hist130v1",
        }),
      },
    ],
    allowedActions: ["RETRY", "ESCALATE", "QUERY_RESULT"],
    actionBlockers: [],
  },
  {
    projectionId: "xp_141",
    projectionNo: "XP-20260801-141",
    projectionRevisionId: "xpr_141_v1",
    projectionRevisionNo: 1,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1012",
    salesOrderNo: "XS20260308003",
    salesOrderRevisionId: "sor_1022_v1",
    salesOrderRevisionNo: 1,
    w05CurrentSalesRevisionNo: 1,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "蓝海科技股份",
    targetMallId: "mall_hb",
    targetMallName: "华北商城",
    deliveryStatus: "ESCALATED_MANUAL",
    attemptCount: 5,
    lastAttemptAt: "2026-07-31 22:10:00",
    errorCode: "AUTH_GATEWAY_TIMEOUT",
    errorSummary: "鉴权网关超时，已超过自动重试上限",
    workItemId: "wi_err_xp141",
    errorTaskId: "err_task_xp141",
    latencyBand: "over_sla",
    reconciliationStatus: "NONE",
    pendingDurationLabel: "11 小时",
    ownerLabel: "人工错误责任队列",
    content: content({
      customerExternalIdentity: "ext·蓝海·55d0",
      voucherCategoryExternalIdentity: "cat·通兑卡·z8",
      voucherCategoryErpName: "通兑电子卡",
      faceValue: "2000.00",
      cardCount: "50",
    }),
    allowedActions: ["ESCALATE"],
    actionBlockers: [
      {
        action: "RETRY",
        code: "ESCALATED",
        message: "已转人工处理，按单据重试请到接口错误中心按原任务号处理。",
      },
      {
        action: "QUERY_RESULT",
        code: "ESCALATED",
        message: "已有错误记录，请在接口错误中心处理。",
      },
    ],
  },
  {
    projectionId: "xp_150",
    projectionNo: "XP-20260801-150",
    projectionRevisionId: "xpr_150_v1",
    projectionRevisionNo: 1,
    projectionSource: "MIGRATION_BASELINE",
    salesOrderId: "so_mig_01",
    salesOrderNo: "XS-MIG-0008",
    salesOrderRevisionId: "sor_mig_01_v0",
    salesOrderRevisionNo: 0,
    w05CurrentSalesRevisionNo: 0,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "迁移客户·东海集团",
    targetMallId: "mall_hd",
    targetMallName: "华东商城",
    deliveryStatus: "PENDING",
    attemptCount: 0,
    nextAttemptAt: "2026-08-01 10:00:00",
    latencyBand: "normal",
    reconciliationStatus: "NONE",
    pendingDurationLabel: "排队中",
    ownerLabel: "自动重试",
    content: content({
      customerExternalIdentity: "ext·东海·mig",
      voucherCategoryExternalIdentity: "cat·基线·b0",
      voucherCategoryErpName: "迁移基线卡券",
      faceValue: "100.00",
      cardCount: "10",
      effectiveAt: "2026-06-01 00:00:00",
    }),
    allowedActions: [],
    actionBlockers: [
      {
        action: "RETRY",
        code: "NOT_YET_SENT",
        message: "尚未首次发送，将由后台按计划执行。",
      },
      {
        action: "QUERY_RESULT",
        code: "NO_REQUEST",
        message: "尚无可查询的原请求。",
      },
    ],
  },
  {
    projectionId: "xp_160",
    projectionNo: "XP-20260801-160",
    projectionRevisionId: "xpr_160_v1",
    projectionRevisionNo: 1,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1014",
    salesOrderNo: "XS20260215020",
    salesOrderRevisionId: "sor_1023_v1",
    salesOrderRevisionNo: 1,
    w05CurrentSalesRevisionNo: 1,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "中原商贸联社",
    targetMallId: "mall_hn",
    targetMallName: "华南商城",
    deliveryStatus: "RETRYING",
    attemptCount: 2,
    lastAttemptAt: "2026-08-01 09:05:00",
    nextAttemptAt: "2026-08-01 09:50:00",
    errorCode: "TRANSIENT_NETWORK",
    errorSummary: "商城瞬时网络错误，将自动重试",
    latencyBand: "near_sla",
    reconciliationStatus: "NONE",
    pendingDurationLabel: "48 分钟",
    ownerLabel: "自动重试",
    content: content({
      customerExternalIdentity: "ext·中原·c8f1",
      voucherCategoryExternalIdentity: "cat·粮油卡·g3",
      voucherCategoryErpName: "粮油提货卡",
      faceValue: "300.00",
      cardCount: "120",
      cardForm: "实体卡",
    }),
    allowedActions: ["QUERY_RESULT"],
    actionBlockers: [
      {
        action: "RETRY",
        code: "AUTO_RETRY_SCHEDULED",
        message: "自动重试已安排，无需人工重试。",
      },
    ],
  },
  {
    projectionId: "xp_171",
    projectionNo: "XP-20260801-171",
    projectionRevisionId: "xpr_171_v4",
    projectionRevisionNo: 4,
    projectionSource: "ERP_SALES_REVISION",
    salesOrderId: "so_1024",
    salesOrderNo: "XS20260403015",
    salesOrderRevisionId: "sor_1024_v4",
    salesOrderRevisionNo: 4,
    w05CurrentSalesRevisionNo: 5,
    salesOrderStatus: "已生效",
    salesOrderStatusTone: "success",
    customerLabel: "远景控股",
    targetMallId: "mall_hb",
    targetMallName: "华北商城",
    currentAckedRevisionNo: 2,
    deliveryStatus: "FAILED",
    attemptCount: 2,
    lastAttemptAt: "2026-08-01 06:40:00",
    errorCode: "SCHEMA_MISMATCH",
    errorSummary: "商城字段校验失败：履约期限格式（脱敏）",
    latencyBand: "over_sla",
    reconciliationStatus: "VERSION_MISMATCH",
    pendingDurationLabel: "3 小时 20 分",
    ownerLabel: "运营协同",
    content: content({
      customerExternalIdentity: "ext·远景·e0e0",
      voucherCategoryExternalIdentity: "cat·综合卡·y1",
      voucherCategoryErpName: "综合福利卡",
      faceValue: "500.00",
      cardCount: "400",
      effectiveAt: "2026-07-29 18:00:00",
      contentHash: "sha256:proj171v4",
    }),
    history: [
      {
        projectionRevisionId: "xpr_171_v2",
        projectionRevisionNo: 2,
        salesOrderRevisionId: "sor_1024_v2",
        salesOrderRevisionNo: 2,
        deliveryStatus: "ACKED",
        mallAckAt: "2026-07-18 12:00:00",
        content: content({
          customerExternalIdentity: "ext·远景·e0e0",
          voucherCategoryExternalIdentity: "cat·综合卡·y1",
          voucherCategoryErpName: "综合福利卡",
          faceValue: "500.00",
          cardCount: "200",
          effectiveAt: "2026-07-18 10:00:00",
          contentHash: "sha256:proj171v2",
        }),
      },
    ],
    allowedActions: ["RETRY", "ESCALATE", "QUERY_RESULT"],
    actionBlockers: [],
  },
]

export const MALL_OPTIONS = [
  { id: "mall_hd", name: "华东商城" },
  { id: "mall_hn", name: "华南商城" },
  { id: "mall_hb", name: "华北商城" },
] as const

export function seedToListRow(seed: ProjectionSeed): ExecutionProjectionRow {
  return {
    projectionId: seed.projectionId,
    projectionNo: seed.projectionNo,
    projectionRevisionId: seed.projectionRevisionId,
    projectionRevisionNo: seed.projectionRevisionNo,
    projectionSource: seed.projectionSource,
    salesOrderId: seed.salesOrderId,
    salesOrderNo: seed.salesOrderNo,
    salesOrderRevisionId: seed.salesOrderRevisionId,
    salesOrderRevisionNo: seed.salesOrderRevisionNo,
    salesOrderStatus: seed.salesOrderStatus,
    salesOrderStatusTone: seed.salesOrderStatusTone,
    customerLabel: seed.customerLabel,
    targetMallId: seed.targetMallId,
    targetMallName: seed.targetMallName,
    currentAckedRevisionNo: seed.currentAckedRevisionNo,
    delivery: {
      deliveryId: `dlv_${seed.projectionId}`,
      status: seed.deliveryStatus,
      statusLabel: DELIVERY_STATUS_LABEL[seed.deliveryStatus],
      statusTone: DELIVERY_STATUS_TONE[seed.deliveryStatus],
      attemptCount: seed.attemptCount,
      lastAttemptAt: seed.lastAttemptAt,
      nextAttemptAt: seed.nextAttemptAt,
      mallAckAt: seed.mallAckAt,
      mallExecutionBaseline: seed.mallExecutionBaseline,
      errorCode: seed.errorCode,
      errorSummary: seed.errorSummary,
      workItemId: seed.workItemId,
      errorTaskId: seed.errorTaskId,
    },
    latencyBand: seed.latencyBand,
    reconciliationStatus: seed.reconciliationStatus,
    pendingDurationLabel: seed.pendingDurationLabel,
    ownerLabel: seed.ownerLabel,
    allowedActions: [...seed.allowedActions],
    actionBlockers: seed.actionBlockers.map((b) => ({ ...b })),
    objectVersion: `ov-${seed.projectionId}-v1`,
    whitelistPreview: {
      voucherCategoryErpName: seed.content.voucherCategoryErpName,
      faceValue: seed.content.faceValue,
      cardCount: seed.content.cardCount,
      cardForm: seed.content.cardForm,
      voucherExpiryAt: seed.content.voucherExpiryAt,
    },
  }
}
