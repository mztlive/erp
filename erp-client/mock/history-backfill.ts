/**
 * W30 历史消费回填 · seed 数据（session-mock）
 * 不含卡号/卡密/手机/完整地址/原始消息内容。
 */

import type {
  CreateBackfillContext,
  HistoryBackfillDetailView,
  HistoryBackfillItemView,
  HistoryBackfillJobCore,
  HistoryBackfillReportView,
} from "@/features/history-backfill/types"

const SCOPE_NOTE =
  "生效范围半开区间 [rangeStart, T)。occurredAt = T 的记录不进入历史回填，按实时/补投契约处理。"

const LEGACY_NOTE =
  "T 前支付只补台账，履约链固定 LEGACY_MANUAL，不创建供应商订单、取消或退款动作。"

const SENSITIVE_NOTE =
  "报告与明细已脱敏：不含卡号、卡密、绑定手机号、完整履约地址或原始商城消息内容。"

function baseJob(
  partial: Omit<
    HistoryBackfillJobCore,
    | "fulfillmentNote"
    | "scopeNote"
    | "legacyManualNote"
    | "formalDownstreamUnlocked"
  > &
    Partial<
      Pick<
        HistoryBackfillJobCore,
        "fulfillmentNote" | "scopeNote" | "legacyManualNote" | "formalDownstreamUnlocked"
      >
    >
): HistoryBackfillJobCore {
  const formalDownstreamUnlocked =
    partial.formalDownstreamUnlocked ??
    (partial.reportReviewStatus === "CONFIRMED" &&
      partial.coverageComplete &&
      partial.processingStatus === "COMPLETED")
  return {
    fulfillmentNote: "历史记录追加写入，不覆盖实时记录",
    scopeNote: SCOPE_NOTE,
    legacyManualNote: LEGACY_NOTE,
    formalDownstreamUnlocked,
    ...partial,
  }
}

export const CREATE_CONTEXT_SEED: CreateBackfillContext = {
  cutoverId: "cutover_east_20260801",
  mallId: "mall_east",
  mallName: "华东商城",
  environment: "production",
  requiredHistoryStart: "2024-01-01T00:00:00+08:00",
  rangeEnd: "2026-08-01T00:00:00+08:00",
  cutoverAt: "2026-08-01T00:00:00+08:00",
  sourceCoverageStart: "2024-01-01T00:00:00+08:00",
  coverageComplete: true,
  coverageGaps: [],
  estimatedFactCount: 128_400,
  hasOverlappingFormalJob: false,
  canCreateDraft: true,
  blockReasons: [],
}

/** 演示：来源覆盖不足时的创建上下文（session 可切换） */
export const CREATE_CONTEXT_GAP: CreateBackfillContext = {
  ...CREATE_CONTEXT_SEED,
  sourceCoverageStart: "2024-06-01T00:00:00+08:00",
  coverageComplete: false,
  coverageGaps: [
    {
      from: "2024-01-01T00:00:00+08:00",
      to: "2024-06-01T00:00:00+08:00",
      reasonCode: "SOURCE_START_LATE",
      reasonLabel: "来源可提供起点晚于 requiredHistoryStart",
    },
  ],
  canCreateDraft: false,
  blockReasons: [
    "来源覆盖起点 2024-06-01 晚于必须覆盖起点 2024-01-01，禁止缩晚 rangeStart 后宣称全历史完成",
    "存在区间缺口，START 前必须修复来源并重新校验",
  ],
}

export const JOB_SEEDS: HistoryBackfillJobCore[] = [
  baseJob({
    id: "hb_job_running_01",
    jobNo: "HB-20260801-01",
    mallId: "mall_east",
    mallName: "华东商城",
    environment: "production",
    cutoverId: "cutover_east_20260801",
    requiredHistoryStart: "2024-01-01T00:00:00+08:00",
    rangeStart: "2024-01-01T00:00:00+08:00",
    rangeEnd: "2026-08-01T00:00:00+08:00",
    cutoverAt: "2026-08-01T00:00:00+08:00",
    sourceCoverageStart: "2024-01-01T00:00:00+08:00",
    coverageComplete: true,
    coverageGaps: [],
    processingStatus: "RUNNING",
    reportReviewStatus: "NOT_READY",
    pipelineStage: "INGEST",
    lockVersion: 3,
    requestedBy: "系统管理员 · 陈浩",
    requestedAt: "2026-08-01T08:10:00+08:00",
    sourceAsOf: "2026-08-01T10:42:00+08:00",
    idempotencyNamespace: "mall-backfill:HB-20260801-01",
    progress: {
      totalCount: 128_400,
      processedCount: 42_180,
      insertedCount: 38_902,
      deduplicatedCount: 2_640,
      unattributedCount: 412,
      failedCount: 226,
      lastProgressAt: "2026-08-01T10:42:00+08:00",
      heartbeatAt: "2026-08-01T10:42:12+08:00",
    },
    costBasis: [
      {
        basis: "ACTUAL",
        count: 28_100,
        consumptionAmountGross: "¥4,812,300.00",
        costAmountNet: "¥4,210,088.40",
      },
      {
        basis: "STANDARD",
        count: 9_420,
        consumptionAmountGross: "¥1,204,550.00",
        costAmountNet: "¥1,088,210.00",
      },
      {
        basis: "NONE",
        count: 1_382,
        consumptionAmountGross: "¥186,420.00",
        costAmountNet: null,
      },
    ],
    coverageRate: "96.92%",
    coveragePercent: 96.92,
    allowedActions: [],
    actionBlockers: [
      {
        action: "START",
        code: "ALREADY_RUNNING",
        message: "任务运行中，不可再启动；失败后可续跑原任务。",
      },
      {
        action: "RESUME",
        code: "NOT_RESUMABLE",
        message: "仅部分完成或失败任务可续跑。",
      },
    ],
  }),
  baseJob({
    id: "hb_job_partial_02",
    jobNo: "HB-20260728-03",
    mallId: "mall_north",
    mallName: "华北商城",
    environment: "production",
    cutoverId: "cutover_north_20260720",
    requiredHistoryStart: "2023-07-01T00:00:00+08:00",
    rangeStart: "2023-07-01T00:00:00+08:00",
    rangeEnd: "2026-07-20T00:00:00+08:00",
    cutoverAt: "2026-07-20T00:00:00+08:00",
    sourceCoverageStart: "2023-07-01T00:00:00+08:00",
    coverageComplete: true,
    coverageGaps: [],
    processingStatus: "PARTIAL",
    reportReviewStatus: "NOT_READY",
    pipelineStage: "ATTRIBUTE",
    lockVersion: 7,
    requestedBy: "系统管理员 · 陈浩",
    requestedAt: "2026-07-28T14:00:00+08:00",
    sourceAsOf: "2026-07-29T18:20:00+08:00",
    idempotencyNamespace: "mall-backfill:HB-20260728-03",
    progress: {
      totalCount: 86_200,
      processedCount: 79_540,
      insertedCount: 72_100,
      deduplicatedCount: 5_880,
      unattributedCount: 920,
      failedCount: 640,
      lastProgressAt: "2026-07-29T18:20:00+08:00",
      heartbeatAt: "2026-07-29T18:20:00+08:00",
    },
    costBasis: [
      {
        basis: "ACTUAL",
        count: 50_200,
        consumptionAmountGross: "¥6,120,000.00",
        costAmountNet: "¥5,401,000.00",
      },
      {
        basis: "STANDARD",
        count: 18_900,
        consumptionAmountGross: "¥2,010,400.00",
        costAmountNet: "¥1,820,100.00",
      },
      {
        basis: "NONE",
        count: 3_000,
        consumptionAmountGross: "¥312,000.00",
        costAmountNet: null,
      },
    ],
    coverageRate: "96.30%",
    coveragePercent: 96.3,
    allowedActions: ["RESUME", "REATTRIBUTE"],
    actionBlockers: [
      {
        action: "START",
        code: "JOB_ALREADY_EXISTS",
        message: "禁止新建重叠业务批次；请续跑原任务 HB-20260728-03。",
      },
    ],
  }),
  baseJob({
    id: "hb_job_completed_03",
    jobNo: "HB-20260715-02",
    mallId: "mall_east",
    mallName: "华东商城",
    environment: "verification",
    cutoverId: "cutover_east_verify_20260710",
    requiredHistoryStart: "2025-01-01T00:00:00+08:00",
    rangeStart: "2025-01-01T00:00:00+08:00",
    rangeEnd: "2026-07-10T00:00:00+08:00",
    cutoverAt: "2026-07-10T00:00:00+08:00",
    sourceCoverageStart: "2025-01-01T00:00:00+08:00",
    coverageComplete: true,
    coverageGaps: [],
    processingStatus: "COMPLETED",
    reportReviewStatus: "POLICY_NOT_CONFIGURED",
    pipelineStage: "DONE",
    lockVersion: 12,
    requestedBy: "系统管理员 · 陈浩",
    requestedAt: "2026-07-15T09:00:00+08:00",
    sourceAsOf: "2026-07-16T11:00:00+08:00",
    idempotencyNamespace: "mall-backfill:HB-20260715-02",
    formalDownstreamUnlocked: false,
    progress: {
      totalCount: 41_200,
      processedCount: 41_200,
      insertedCount: 36_880,
      deduplicatedCount: 3_410,
      unattributedCount: 620,
      failedCount: 290,
      lastProgressAt: "2026-07-16T11:00:00+08:00",
      heartbeatAt: "2026-07-16T11:00:00+08:00",
    },
    costBasis: [
      {
        basis: "ACTUAL",
        count: 28_000,
        consumptionAmountGross: "¥2,880,000.00",
        costAmountNet: "¥2,510,000.00",
      },
      {
        basis: "STANDARD",
        count: 7_200,
        consumptionAmountGross: "¥640,000.00",
        costAmountNet: "¥590,000.00",
      },
      {
        basis: "NONE",
        count: 1_680,
        consumptionAmountGross: "¥128,400.00",
        costAmountNet: null,
      },
    ],
    coverageRate: "96.48%",
    coveragePercent: 96.48,
    allowedActions: [],
    actionBlockers: [
      {
        action: "CONFIRM_REPORT",
        code: "REPORT_REVIEW_POLICY_MISSING",
        message:
          "报告复核策略未配置：仅可下载标记为「技术报告 · 未确认」的技术报告，不得宣称全历史完成或解锁下游。",
      },
    ],
  }),
  baseJob({
    id: "hb_job_ready_gap_04",
    jobNo: "HB-20260801-DRAFT",
    mallId: "mall_south",
    mallName: "华南商城",
    environment: "production",
    cutoverId: "cutover_south_20260725",
    requiredHistoryStart: "2024-03-01T00:00:00+08:00",
    rangeStart: "2024-03-01T00:00:00+08:00",
    rangeEnd: "2026-07-25T00:00:00+08:00",
    cutoverAt: "2026-07-25T00:00:00+08:00",
    sourceCoverageStart: "2024-09-01T00:00:00+08:00",
    coverageComplete: false,
    coverageGaps: [
      {
        from: "2024-03-01T00:00:00+08:00",
        to: "2024-09-01T00:00:00+08:00",
        reasonCode: "SOURCE_START_LATE",
        reasonLabel: "来源可提供起点晚于 requiredHistoryStart",
      },
    ],
    processingStatus: "VALIDATING",
    reportReviewStatus: "NOT_READY",
    pipelineStage: "VALIDATE_SOURCE",
    lockVersion: 1,
    requestedBy: "系统管理员 · 陈浩",
    requestedAt: "2026-08-01T07:30:00+08:00",
    sourceAsOf: "2026-08-01T07:35:00+08:00",
    idempotencyNamespace: "mall-backfill:HB-20260801-DRAFT",
    progress: {
      totalCount: 0,
      processedCount: 0,
      insertedCount: 0,
      deduplicatedCount: 0,
      unattributedCount: 0,
      failedCount: 0,
    },
    costBasis: [
      { basis: "ACTUAL", count: 0, consumptionAmountGross: "¥0.00", costAmountNet: "¥0.00" },
      {
        basis: "STANDARD",
        count: 0,
        consumptionAmountGross: "¥0.00",
        costAmountNet: "¥0.00",
      },
      { basis: "NONE", count: 0, consumptionAmountGross: "¥0.00", costAmountNet: null },
    ],
    coverageRate: null,
    coveragePercent: 0,
    allowedActions: ["VALIDATE_SOURCE"],
    actionBlockers: [
      {
        action: "START",
        code: "COVERAGE_INCOMPLETE",
        message:
          "来源覆盖不足：requiredHistoryStart=2024-03-01，sourceCoverageStart=2024-09-01。禁止改晚 rangeStart。",
      },
    ],
  }),
  baseJob({
    id: "hb_job_failed_05",
    jobNo: "HB-20260720-01",
    mallId: "mall_west",
    mallName: "西部商城",
    environment: "production",
    cutoverId: "cutover_west_20260718",
    requiredHistoryStart: "2024-01-01T00:00:00+08:00",
    rangeStart: "2024-01-01T00:00:00+08:00",
    rangeEnd: "2026-07-18T00:00:00+08:00",
    cutoverAt: "2026-07-18T00:00:00+08:00",
    sourceCoverageStart: "2024-01-01T00:00:00+08:00",
    coverageComplete: true,
    coverageGaps: [],
    processingStatus: "FAILED",
    reportReviewStatus: "NOT_READY",
    pipelineStage: "INGEST",
    lockVersion: 4,
    requestedBy: "系统管理员 · 陈浩",
    requestedAt: "2026-07-20T10:00:00+08:00",
    sourceAsOf: "2026-07-20T16:40:00+08:00",
    idempotencyNamespace: "mall-backfill:HB-20260720-01",
    progress: {
      totalCount: 54_000,
      processedCount: 12_400,
      insertedCount: 11_050,
      deduplicatedCount: 880,
      unattributedCount: 210,
      failedCount: 260,
      lastProgressAt: "2026-07-20T16:40:00+08:00",
      heartbeatAt: "2026-07-20T16:40:00+08:00",
    },
    costBasis: [
      {
        basis: "ACTUAL",
        count: 8_200,
        consumptionAmountGross: "¥980,000.00",
        costAmountNet: "¥860,000.00",
      },
      {
        basis: "STANDARD",
        count: 2_400,
        consumptionAmountGross: "¥210,000.00",
        costAmountNet: "¥190,000.00",
      },
      {
        basis: "NONE",
        count: 450,
        consumptionAmountGross: "¥42,000.00",
        costAmountNet: null,
      },
    ],
    coverageRate: "96.59%",
    coveragePercent: 96.59,
    allowedActions: ["RESUME"],
    actionBlockers: [
      {
        action: "START",
        code: "JOB_ALREADY_EXISTS",
        message: "禁止新建重叠业务批次；请续跑原任务并复用原任务号。",
      },
    ],
  }),
]

export const ITEM_SEEDS: HistoryBackfillItemView[] = [
  // 同一商城订单下五类记录 + 多次退款/恢复 —— 不得合并
  {
    itemId: "hbi_pay_1001",
    jobId: "hb_job_running_01",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-E-100861·2025-03-12",
    mallOrderNo: "MO-E-100861",
    occurredAt: "2025-03-12T14:22:00+08:00",
    result: "INSERTED",
    costBasis: "ACTUAL",
    costAmountNet: "¥86.20",
    consumptionAmountGross: "¥100.00",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "taxMode", label: "税口径", value: "含税 · 进项 13%" },
      { field: "payChannel", label: "支付来源", value: "商城余额" },
    ],
  },
  {
    itemId: "hbi_cancel_1001",
    jobId: "hb_job_running_01",
    factType: "ORDER_CANCELED",
    businessFactKeySummary: "cancel·MO-E-100861·2025-03-12",
    mallOrderNo: "MO-E-100861",
    occurredAt: "2025-03-12T15:01:00+08:00",
    result: "INSERTED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "cancelReason", label: "取消原因", value: "用户取消·库存不足" },
    ],
  },
  {
    itemId: "hbi_refund_1001a",
    jobId: "hb_job_running_01",
    factType: "REFUND_SUCCEEDED",
    businessFactKeySummary: "refund·MO-E-100861·RF-01",
    mallOrderNo: "MO-E-100861",
    sourceDocNo: "RF-E-100861-01",
    occurredAt: "2025-03-13T09:10:00+08:00",
    result: "INSERTED",
    costBasis: "ACTUAL",
    costAmountNet: "¥20.00",
    consumptionAmountGross: "¥30.00",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "refundKind", label: "退款类型", value: "部分退款 1/2" },
    ],
  },
  {
    itemId: "hbi_refund_1001b",
    jobId: "hb_job_running_01",
    factType: "REFUND_SUCCEEDED",
    businessFactKeySummary: "refund·MO-E-100861·RF-02",
    mallOrderNo: "MO-E-100861",
    sourceDocNo: "RF-E-100861-02",
    occurredAt: "2025-03-14T11:20:00+08:00",
    result: "INSERTED",
    costBasis: "ACTUAL",
    costAmountNet: "¥15.00",
    consumptionAmountGross: "¥20.00",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "refundKind", label: "退款类型", value: "部分退款 2/2" },
    ],
  },
  {
    itemId: "hbi_complete_1001",
    jobId: "hb_job_running_01",
    factType: "ORDER_COMPLETED",
    businessFactKeySummary: "complete·MO-E-100861·2025-03-15",
    mallOrderNo: "MO-E-100861",
    occurredAt: "2025-03-15T16:00:00+08:00",
    result: "INSERTED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "completeMode", label: "完成方式", value: "商城历史完成" },
    ],
  },
  {
    itemId: "hbi_restore_1001a",
    jobId: "hb_job_running_01",
    factType: "CARD_BALANCE_RESTORED",
    businessFactKeySummary: "restore·MO-E-100861·RS-01",
    mallOrderNo: "MO-E-100861",
    sourceDocNo: "RS-E-100861-01",
    occurredAt: "2025-03-13T09:11:00+08:00",
    result: "INSERTED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "restoreKind", label: "恢复类型", value: "部分余额恢复 1/2" },
      { field: "cardInstanceRef", label: "卡实例引用", value: "CI-****861A" },
    ],
  },
  {
    itemId: "hbi_restore_1001b",
    jobId: "hb_job_running_01",
    factType: "CARD_BALANCE_RESTORED",
    businessFactKeySummary: "restore·MO-E-100861·RS-02",
    mallOrderNo: "MO-E-100861",
    sourceDocNo: "RS-E-100861-02",
    occurredAt: "2025-03-14T11:21:00+08:00",
    result: "INSERTED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "restoreKind", label: "恢复类型", value: "部分余额恢复 2/2" },
      { field: "cardInstanceRef", label: "卡实例引用", value: "CI-****861A" },
    ],
  },
  // 与实时重叠去重证明
  {
    itemId: "hbi_dedupe_2002",
    jobId: "hb_job_running_01",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-E-200902·2026-07-30",
    mallOrderNo: "MO-E-200902",
    occurredAt: "2026-07-30T10:00:00+08:00",
    result: "DEDUPLICATED",
    costBasis: "ACTUAL",
    costAmountNet: "¥42.00",
    consumptionAmountGross: "¥50.00",
    fulfillmentChain: "LEGACY_MANUAL",
    dedupeProof: {
      matchedSource: "REALTIME",
      originalMessageId: "inbox_msg_rt_88921",
      formalFactId: "mof_pay_88921",
      formalFactSummary: "已存在同一业务记录 · 实时回流支付",
    },
    whitelistFields: [
      { field: "dedupeNote", label: "去重说明", value: "与实时记录键命中，不形成第二份" },
    ],
  },
  {
    itemId: "hbi_dedupe_prior",
    jobId: "hb_job_partial_02",
    factType: "ORDER_COMPLETED",
    businessFactKeySummary: "complete·MO-N-55110·2024-11-02",
    mallOrderNo: "MO-N-55110",
    occurredAt: "2024-11-02T18:30:00+08:00",
    result: "DEDUPLICATED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    dedupeProof: {
      matchedSource: "PRIOR_BACKFILL",
      originalMessageId: "inbox_msg_bf_4410",
      formalFactId: "mof_complete_4410",
      formalFactSummary: "已存在同一业务记录 · 原回填任务写入",
    },
    whitelistFields: [
      { field: "dedupeNote", label: "去重说明", value: "与原任务重跑重叠，保留首份业务记录" },
    ],
  },
  // 待归集 → W29
  {
    itemId: "hbi_unattr_3001",
    jobId: "hb_job_running_01",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-E-300771·2025-08-01",
    mallOrderNo: "MO-E-300771",
    occurredAt: "2025-08-01T12:00:00+08:00",
    result: "UNATTRIBUTED",
    costBasis: "NONE",
    costAmountNet: null,
    consumptionAmountGross: "¥220.00",
    fulfillmentChain: "LEGACY_MANUAL",
    unattributedReason: "供应商商品映射缺失 · 消费时点供给版本不可判定",
    workItemId: "wi_iet_map_002",
    whitelistFields: [
      { field: "productRef", label: "商城商品", value: "SKU-EXT-****771" },
      { field: "mapGap", label: "归集缺口", value: "W21 映射 / 税口径" },
    ],
  },
  {
    itemId: "hbi_unattr_3002",
    jobId: "hb_job_partial_02",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-N-88012·2024-05-18",
    mallOrderNo: "MO-N-88012",
    occurredAt: "2024-05-18T09:40:00+08:00",
    result: "UNATTRIBUTED",
    costBasis: "NONE",
    costAmountNet: null,
    consumptionAmountGross: "¥68.00",
    fulfillmentChain: "LEGACY_MANUAL",
    unattributedReason: "卡实例归属销售单无法解析",
    workItemId: "wi_rd_diff_010",
    whitelistFields: [
      { field: "cardInstanceRef", label: "卡实例引用", value: "CI-****012N" },
      { field: "mapGap", label: "归集缺口", value: "销售单主责 / 卡实例" },
    ],
  },
  // 失败
  {
    itemId: "hbi_fail_4001",
    jobId: "hb_job_failed_05",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-W-41001·2025-01-09",
    mallOrderNo: "MO-W-41001",
    occurredAt: "2025-01-09T08:15:00+08:00",
    result: "FAILED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    failure: {
      errorCode: "SOURCE_SCHEMA_FIELD_MISSING",
      stage: "INGEST",
      retryable: true,
      summary: "来源缺金额分摊字段 · 可续跑",
    },
    whitelistFields: [
      { field: "errorClass", label: "错误分类", value: "来源契约 · 可重试" },
    ],
  },
  {
    itemId: "hbi_fail_4002",
    jobId: "hb_job_partial_02",
    factType: "REFUND_SUCCEEDED",
    businessFactKeySummary: "refund·MO-N-66001·RF-09",
    mallOrderNo: "MO-N-66001",
    sourceDocNo: "RF-N-66001-09",
    occurredAt: "2025-12-01T20:00:00+08:00",
    result: "FAILED",
    costBasis: "N_A",
    fulfillmentChain: "LEGACY_MANUAL",
    failure: {
      errorCode: "TAX_BASIS_UNRESOLVED",
      stage: "ATTRIBUTE",
      retryable: false,
      summary: "税口径无法解析 · 需业务修复后重新归集",
    },
    whitelistFields: [
      { field: "errorClass", label: "错误分类", value: "业务修复 · 不可直接重试" },
    ],
  },
  // STANDARD / NONE 成本示例
  {
    itemId: "hbi_std_5001",
    jobId: "hb_job_completed_03",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-E-V-501·2025-06-01",
    mallOrderNo: "MO-E-V-501",
    occurredAt: "2025-06-01T13:00:00+08:00",
    result: "INSERTED",
    costBasis: "STANDARD",
    costAmountNet: "¥55.00",
    consumptionAmountGross: "¥66.00",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      {
        field: "supplyVersion",
        label: "供给版本",
        value: "SV-2025-06-01-A · 消费时点有效（非当前价）",
      },
    ],
  },
  {
    itemId: "hbi_none_5002",
    jobId: "hb_job_completed_03",
    factType: "PAYMENT_SUCCEEDED",
    businessFactKeySummary: "pay·MO-E-V-502·2025-06-02",
    mallOrderNo: "MO-E-V-502",
    occurredAt: "2025-06-02T13:00:00+08:00",
    result: "INSERTED",
    costBasis: "NONE",
    costAmountNet: null,
    consumptionAmountGross: "¥40.00",
    fulfillmentChain: "LEGACY_MANUAL",
    whitelistFields: [
      { field: "noneReason", label: "NONE 原因", value: "无商城成本记录且无时点供给版本" },
      { field: "costDisplay", label: "成本字段", value: "空（非 0）" },
    ],
  },
]

export function buildReportForJob(
  job: HistoryBackfillJobCore
): HistoryBackfillReportView | undefined {
  if (
    job.processingStatus !== "COMPLETED" &&
    job.processingStatus !== "PARTIAL" &&
    job.progress.processedCount === 0
  ) {
    return undefined
  }
  // 技术报告在 COMPLETED 后正式生成；PARTIAL 也可有中间摘要
  if (
    job.processingStatus !== "COMPLETED" &&
    job.processingStatus !== "PARTIAL"
  ) {
    return undefined
  }

  const unconfirmed =
    job.reportReviewStatus !== "CONFIRMED" || !job.coverageComplete
  const reviewLabel = unconfirmed ? "UNCONFIRMED" : "CONFIRMED"
  const downloadLabel =
    reviewLabel === "CONFIRMED"
      ? "已确认报告"
      : "技术报告 · 未确认"

  const unattributed = ITEM_SEEDS.filter(
    (i) => i.jobId === job.id && i.result === "UNATTRIBUTED"
  ).map(
    (i) =>
      `${i.mallOrderNo} · ${i.businessFactKeySummary} · ${i.unattributedReason ?? "待归集"}`
  )
  const failed = ITEM_SEEDS.filter(
    (i) => i.jobId === job.id && i.result === "FAILED"
  ).map(
    (i) =>
      `${i.mallOrderNo} · ${i.failure?.errorCode ?? "FAIL"} · ${i.failure?.summary ?? ""}`
  )

  const totalAmount = job.costBasis
    .map((c) => c.consumptionAmountGross)
    .join(" + ")

  return {
    reportId: `rpt_${job.id}`,
    reportVersion: job.processingStatus === "COMPLETED" ? 1 : 0,
    generatedAt: job.progress.lastProgressAt ?? job.requestedAt,
    reviewLabel,
    downloadLabel,
    schemaVersion: "mall-backfill-report@2026.07",
    ruleVersion: "hist-cost-v3",
    rangeStart: job.rangeStart,
    rangeEnd: job.rangeEnd,
    cutoverAt: job.cutoverAt,
    totalCount: job.progress.totalCount,
    totalAmount:
      job.processingStatus === "COMPLETED"
        ? "¥3,648,400.00"
        : totalAmount || "—",
    insertedCount: job.progress.insertedCount,
    deduplicatedCount: job.progress.deduplicatedCount,
    unattributedCount: job.progress.unattributedCount,
    failedCount: job.progress.failedCount,
    costBasis: job.costBasis,
    coverageRate: job.coverageRate,
    unattributedSummaries:
      unattributed.length > 0
        ? unattributed
        : [`待归集 ${job.progress.unattributedCount} 笔 · 详见任务明细`],
    failedSummaries:
      failed.length > 0
        ? failed
        : [`失败 ${job.progress.failedCount} 笔 · 详见失败诊断`],
    operatorLabel: job.requestedBy,
    processingStatus: job.processingStatus,
    reportReviewStatus: job.reportReviewStatus,
    fullHistoryFinalComplete:
      job.processingStatus === "COMPLETED" &&
      job.reportReviewStatus === "CONFIRMED" &&
      job.coverageComplete &&
      job.formalDownstreamUnlocked,
    sensitiveRedactionNote: SENSITIVE_NOTE,
  }
}

export function seedDetail(jobId: string): HistoryBackfillDetailView | null {
  const job = JOB_SEEDS.find((j) => j.id === jobId)
  if (!job) return null
  return {
    job,
    items: ITEM_SEEDS.filter((i) => i.jobId === jobId),
    report: buildReportForJob(job),
    queriedAt: new Date().toISOString(),
    permissionVersion: "pv-w30-1",
  }
}
