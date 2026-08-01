/**
 * W13 卡券票款复核 · 静态种子。
 * 会话覆盖（登记票款、完成、暂挂、指纹变化）在 session-state / api 中投影。
 */

import type {
  CardFundsReviewItemView,
  ReviewHistoryItem,
} from "@/features/card-funds-review/types"

const chainOpeningNone: ReviewHistoryItem[] = []

const chainWithPriorApproved: ReviewHistoryItem[] = [
  {
    reviewId: "rfr_prev_01",
    reviewNo: 1,
    reviewType: "OPENING",
    reviewResult: "APPROVED",
    conclusion: "RECORDED_FACTS_RECONCILED",
    reviewerLabel: "财务 · 王敏",
    completedAt: "2026-07-10T15:20:00+08:00",
    subjectHashAtReview: "sha256:opening_v1_settled86k",
    readOnly: true,
  },
]

function baseItem(
  partial: CardFundsReviewItemView
): CardFundsReviewItemView {
  return partial
}

/** 期初 · 净已收/已开均为 0 · 可「从 0 起」 */
export const SEED_OPENING_ZERO: CardFundsReviewItemView = baseItem({
  workItem: {
    workItemId: "wi_card_01",
    workItemType: "CARD_FUNDS_REVIEW",
    completionAction: "COMPLETE_CARD_FUNDS_REVIEW",
    subjectVersion: "sv_card_01_v1",
    subjectHash: "sha256:card01_open_zero_rev3",
    workItemStatus: "PENDING",
    dueAt: "2026-08-01T16:00:00+08:00",
    allowedActions: [
      "CLAIM",
      "CONFIRM_ZERO",
      "APPROVE",
      "REJECT",
      "HOLD",
      "REGISTER_RECEIPT",
      "REGISTER_INVOICE",
    ],
    actionBlockers: [],
    reason: "期初卡券应收已形成，净已收/已开为 0，须人工确认有无历史票款",
    impact: "未复核前不得计入已确认经营收入；W11/W15 指标标记不可靠",
    priority: 90,
  },
  salesOrder: {
    id: "so_card_01",
    orderNo: "XS20260325008",
    revisionNo: 3,
    snapshotAt: "2026-03-25T11:02:00+08:00",
  },
  account: {
    id: "recv_card_01",
    accountSeq: 1,
    domainVersion: "adv_card_01_1",
    customerId: "cust_lanwan",
    customerName: "蓝湾集团",
    counterpartyPartyId: "party_lanwan_fin",
    counterpartyPartyName: "蓝湾集团财务共享中心",
    mallName: "蓝湾企业商城",
    reviewStatus: "PENDING_OPENING_REVIEW",
    grossTotal: "128000.00",
    settledTotal: "0.00",
    openTotal: "128000.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "128000.00",
    syncedGrossAmount: "128000.00",
    fundsReliability: "UNRELIABLE_PENDING_REVIEW",
    reliabilityNote:
      "卡券期初复核未完成：净已收/净已开票为 0 不代表已核实无票款，不得作为经营结果。",
  },
  reviewChain: {
    chainVersion: "chain_card_01_v0",
    nextReviewNo: 1,
    items: chainOpeningNone,
  },
  currentSalesOrderRevisionId: "sor_card_01_r3",
  fundsFactVersion: "ffv_card_01_0",
  receiptFacts: [],
  invoiceFacts: [],
  reviewType: "OPENING",
  fingerprintStatus: {
    label: "待复核数据版本",
    tone: "warning",
    detail: "当前 subject_hash 与任务一致；完成时将三方重算校验",
  },
  currentEvidence: {
    evidenceDocumentIds: [],
    evidenceReferences: [],
    comment: "",
  },
})

/** 期初 · 已有部分回款待登记/核对 */
export const SEED_OPENING_PARTIAL: CardFundsReviewItemView = baseItem({
  workItem: {
    workItemId: "wi_card_02",
    workItemType: "CARD_FUNDS_REVIEW",
    completionAction: "COMPLETE_CARD_FUNDS_REVIEW",
    subjectVersion: "sv_card_02_v1",
    subjectHash: "sha256:card02_open_partial_rev2",
    workItemStatus: "PENDING",
    dueAt: "2026-08-01T18:00:00+08:00",
    allowedActions: [
      "CLAIM",
      "APPROVE",
      "REJECT",
      "HOLD",
      "REGISTER_RECEIPT",
      "REGISTER_INVOICE",
    ],
    actionBlockers: [
      {
        action: "CONFIRM_ZERO",
        code: "SETTLED_OR_INVOICED_NOT_ZERO",
        message: "净已收或净已开不为 0，不能使用「从 0 起」结论",
      },
    ],
    reason: "商城支付成功记录与 ERP 应收需人工对齐，历史上线前已有回款",
    impact: "未复核前不得计入已确认经营收入",
    priority: 80,
  },
  salesOrder: {
    id: "so_card_02",
    orderNo: "XS20260412015",
    revisionNo: 2,
    snapshotAt: "2026-04-12T09:40:00+08:00",
  },
  account: {
    id: "recv_card_02",
    accountSeq: 1,
    domainVersion: "adv_card_02_1",
    customerId: "cust_beichen",
    customerName: "北辰能源集团",
    counterpartyPartyId: "party_beichen_ap",
    counterpartyPartyName: "北辰能源应付中心",
    mallName: "北辰礼赠商城",
    reviewStatus: "PENDING_OPENING_REVIEW",
    grossTotal: "86000.00",
    settledTotal: "0.00",
    openTotal: "86000.00",
    invoicedTotal: "0.00",
    openInvoiceableTotal: "86000.00",
    syncedGrossAmount: "86000.00",
    fundsReliability: "UNRELIABLE_PENDING_REVIEW",
    reliabilityNote:
      "卡券期初复核未完成：页面金额不可靠，不得以 0 冒充已核实。",
  },
  reviewChain: {
    chainVersion: "chain_card_02_v0",
    nextReviewNo: 1,
    items: chainOpeningNone,
  },
  currentSalesOrderRevisionId: "sor_card_02_r2",
  fundsFactVersion: "ffv_card_02_0",
  receiptFacts: [],
  invoiceFacts: [],
  reviewType: "OPENING",
  fingerprintStatus: {
    label: "待登记票款",
    tone: "info",
    detail: "可通过内嵌 Allocation 登记历史回款/发票后再通过",
  },
  currentEvidence: {
    evidenceDocumentIds: [],
    evidenceReferences: [],
    comment: "",
  },
})

/** 同步差额 · 上一有效复核指纹失效 */
export const SEED_SYNC_DELTA: CardFundsReviewItemView = baseItem({
  workItem: {
    workItemId: "wi_card_03",
    workItemType: "CARD_FUNDS_DELTA_REVIEW",
    completionAction: "COMPLETE_CARD_FUNDS_REVIEW",
    subjectVersion: "sv_card_03_v4",
    subjectHash: "sha256:card03_delta_rev5_settled92k",
    workItemStatus: "PENDING",
    dueAt: "2026-08-02T12:00:00+08:00",
    allowedActions: [
      "CLAIM",
      "APPROVE",
      "REJECT",
      "HOLD",
      "REGISTER_RECEIPT",
      "REGISTER_INVOICE",
    ],
    actionBlockers: [
      {
        action: "CONFIRM_ZERO",
        code: "NOT_OPENING",
        message: "「从 0 起」仅适用于 OPENING 期初任务",
      },
    ],
    reason: "商城同步版本上升导致成交额与应收变化，上一复核数据版本失效",
    impact: "旧通过结论不可沿用；须形成新 SYNC_DELTA 链尾",
    priority: 95,
  },
  salesOrder: {
    id: "so_card_03",
    orderNo: "XS20260508022",
    revisionNo: 5,
    snapshotAt: "2026-07-28T14:10:00+08:00",
  },
  account: {
    id: "recv_card_03",
    accountSeq: 2,
    domainVersion: "adv_card_03_4",
    customerId: "cust_xinghe",
    customerName: "星河制造股份有限公司",
    counterpartyPartyId: "party_xinghe_fin",
    counterpartyPartyName: "星河制造财务部",
    mallName: "星河员工福利商城",
    reviewStatus: "PENDING_DELTA_REVIEW",
    grossTotal: "142000.00",
    settledTotal: "92000.00",
    openTotal: "50000.00",
    invoicedTotal: "86000.00",
    openInvoiceableTotal: "56000.00",
    syncedGrossAmount: "142000.00",
    fundsReliability: "STALE_FINGERPRINT",
    reliabilityNote:
      "上一复核数据版本已失效：当前应收/已收/已开票指标在新差额复核完成前不可靠。",
  },
  reviewChain: {
    tailReviewId: "rfr_prev_01",
    chainVersion: "chain_card_03_v1",
    nextReviewNo: 2,
    items: chainWithPriorApproved,
  },
  currentSalesOrderRevisionId: "sor_card_03_r5",
  fundsFactVersion: "ffv_card_03_4",
  receiptFacts: [
    {
      receiptId: "rcpt_03_a",
      receiptNo: "SK20260618003",
      receivedAt: "2026-06-18",
      grossAmount: "92000.00",
      allocatedToAccount: "92000.00",
      otherAllocationSummary: "同主体其它应收 0",
      reversed: false,
    },
  ],
  invoiceFacts: [
    {
      invoiceId: "inv_03_a",
      invoiceNo: "FP-2026-061901",
      direction: "BLUE",
      issuedAt: "2026-06-19",
      grossAmount: "86000.00",
      netAmount: "76106.19",
      taxAmount: "9893.81",
      allocatedToAccount: "86000.00",
      reversed: false,
    },
  ],
  difference: {
    title: "上一有效复核 vs 当前记录",
    baselineReviewNo: 1,
    baselineSubjectHash: "sha256:opening_v1_settled86k",
    invalidatedAt: "2026-07-28T14:12:00+08:00",
    changes: [
      {
        id: "d1",
        field: "同步成交额",
        before: "128000.00",
        after: "142000.00",
        note: "商城版本 r4→r5 追加差额",
        sourceObject: "sales_order_revision r5",
        occurredAt: "2026-07-28T14:10:00+08:00",
      },
      {
        id: "d2",
        field: "当前应收 gross_total",
        before: "128000.00",
        after: "142000.00",
        note: "追加应收分录 +14000",
        sourceObject: "receivable_entry ae_delta_14k",
        occurredAt: "2026-07-28T14:11:00+08:00",
      },
      {
        id: "d3",
        field: "净已收 settled_total",
        before: "86000.00",
        after: "92000.00",
        note: "回款分配 APPLY 追加 6000",
        sourceObject: "receipt_allocation ra_03_b",
        occurredAt: "2026-07-20T10:00:00+08:00",
      },
      {
        id: "d4",
        field: "净已开票 invoiced_total",
        before: "86000.00",
        after: "86000.00",
        note: "发票分配未变",
        sourceObject: "sales_invoice_allocation",
      },
      {
        id: "d5",
        field: "subject_hash",
        before: "sha256:opening_v1_settled86k",
        after: "sha256:card03_delta_rev5_settled92k",
        note: "上一有效复核数据版本失效",
        sourceObject: "receivable_funds_review",
        occurredAt: "2026-07-28T14:12:00+08:00",
      },
    ],
  },
  reviewType: "SYNC_DELTA",
  fingerprintStatus: {
    label: "数据版本已变化",
    tone: "destructive",
    detail: "须形成新链尾；禁止复制旧 subject_hash 或沿用旧通过结论",
  },
  currentEvidence: {
    evidenceDocumentIds: [],
    evidenceReferences: [],
    comment: "",
  },
})

export const CARD_FUNDS_REVIEW_SEED: readonly CardFundsReviewItemView[] = [
  SEED_OPENING_ZERO,
  SEED_OPENING_PARTIAL,
  SEED_SYNC_DELTA,
]

/** W11 列表 mock：未完成卡券复核时的不可靠标识（账户级）。 */
export const W11_CARD_FUNDS_RELIABILITY: ReadonlyArray<{
  accountId: string
  customerName: string
  fundsReliability: "UNRELIABLE_PENDING_REVIEW" | "STALE_FINGERPRINT" | "VERIFIED"
  note: string
}> = [
  {
    accountId: "recv_card_01",
    customerName: "蓝湾集团",
    fundsReliability: "UNRELIABLE_PENDING_REVIEW",
    note: "卡券票款复核未完成：不以 0 冒充已核实",
  },
  {
    accountId: "recv_card_02",
    customerName: "北辰能源集团",
    fundsReliability: "UNRELIABLE_PENDING_REVIEW",
    note: "卡券票款复核未完成：指标不可靠",
  },
  {
    accountId: "recv_card_03",
    customerName: "星河制造股份有限公司",
    fundsReliability: "STALE_FINGERPRINT",
    note: "旧复核数据版本失效：待差额复核",
  },
]
