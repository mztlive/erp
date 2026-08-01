/**
 * W27 mock seed — API 供应商结算
 * 金额均为服务端已舍入字符串；前端只展示不重算。
 */

import type {
  SettlementDifferenceView,
  SettlementItemView,
  SettlementStatus,
  AuditEventView,
  ReviewRecordView,
  ActorView,
} from "@/features/supplier-settlements/types"
import {
  DIFF_STATUS_LABEL,
  DIFF_TYPE_LABEL,
  STATUS_LABEL,
  STATUS_TONE,
  ACTORS,
} from "@/features/supplier-settlements/types"
import type { StatusTone } from "@/components/ui/status-badge"

export type SeedDifference = Omit<
  SettlementDifferenceView,
  "typeLabel" | "statusLabel" | "statusTone"
> & {
  type: SettlementDifferenceView["type"]
  status: SettlementDifferenceView["status"]
}

export type SeedStatement = {
  statementId: string
  statementNo: string
  supplierId: string
  supplierName: string
  periodStart: string
  periodEnd: string
  periodLabel: string
  status: SettlementStatus
  externalBillNo?: string
  externalBillVersion?: string
  orderAmountGross: string
  freightGross: string
  serviceFeeGross: string
  refundGross: string
  erpAmountGross: string
  supplierAmountGross?: string
  differenceAmountGross?: string
  preparedBy?: ActorView
  reviewedBy?: ActorView
  lockVersion: number
  subjectHash?: string
  sourceAsOf: string
  sourceSnapshotAt: string
  sourceSnapshotHash: string
  items: SettlementItemView[]
  differences: SeedDifference[]
  reviewRecords: ReviewRecordView[]
  auditEvents: AuditEventView[]
  payable?: {
    payableAccountId: string
    payableNo: string
    grossAmount: string
    dueDate: string
    statusLabel: string
  }
  workItem?: {
    workItemId: string
    subjectVersion: string
    subjectHash: string
    claimedBy?: ActorView
    leaseVersion?: number
  }
  pendingCostDeltaGross?: string
  confirmedCostDeltaGross?: string
}

export const SUPPLIERS = [
  { supplierId: "sup_jd", supplierName: "京东企业购" },
  { supplierId: "sup_sf", supplierName: "顺丰同城" },
  { supplierId: "sup_mt", supplierName: "美团企业版" },
] as const

export const DEFAULT_PERIOD_POLICY = {
  state: "CONFIGURED" as const,
  policyId: "spp_api_default",
  policyVersion: "3",
  timezone: "Asia/Shanghai",
  selectablePeriods: [
    {
      periodStart: "2026-07-01",
      periodEnd: "2026-07-31",
      label: "2026-07（自然月）",
    },
    {
      periodStart: "2026-06-01",
      periodEnd: "2026-06-30",
      label: "2026-06（自然月）",
    },
    {
      periodStart: "2026-08-01",
      periodEnd: "2026-08-31",
      label: "2026-08（自然月）",
    },
  ],
}

export const DEFAULT_REFRESH_CUTOFF = {
  state: "CONFIGURED" as const,
  policyId: "rcp_settlement_v1",
  policyVersion: "2",
  label: "提交复核前冻结：含当日 18:00 前不可变记录与账单版本",
}

function item(
  partial: Omit<SettlementItemView, "readOnly">
): SettlementItemView {
  return { ...partial, readOnly: true }
}

function diffTone(
  status: SeedDifference["status"]
): StatusTone {
  if (status === "RESOLVED") return "success"
  if (status === "BLOCKING" || status === "OPEN") return "warning"
  return "info"
}

export function projectDifference(
  d: SeedDifference
): SettlementDifferenceView {
  return {
    ...d,
    typeLabel: DIFF_TYPE_LABEL[d.type],
    statusLabel: DIFF_STATUS_LABEL[d.status],
    statusTone: diffTone(d.status),
  }
}

export function statusMeta(status: SettlementStatus) {
  return {
    statusLabel: STATUS_LABEL[status],
    statusTone: STATUS_TONE[status],
  }
}

function directionLabel(diff?: string): string | undefined {
  if (diff == null) return undefined
  const n = Number(diff)
  if (!Number.isFinite(n) || n === 0) return "无差异"
  if (n > 0) return "供应商账单高于 ERP"
  return "ERP 高于供应商账单"
}

export function withDirection(seed: SeedStatement) {
  return {
    differenceDirectionLabel: directionLabel(seed.differenceAmountGross),
  }
}

export const SEED_STATEMENTS: SeedStatement[] = [
  {
    statementId: "st_jd_202607",
    statementNo: "ST-2026-07-JD",
    supplierId: "sup_jd",
    supplierName: "京东企业购",
    periodStart: "2026-07-01",
    periodEnd: "2026-07-31",
    periodLabel: "2026-07",
    status: "HAS_DIFFERENCE",
    externalBillNo: "JD-BILL-202607",
    externalBillVersion: "v2",
    orderAmountGross: "472000.00",
    freightGross: "8600.00",
    serviceFeeGross: "4200.00",
    refundGross: "-5600.00",
    erpAmountGross: "479200.00",
    supplierAmountGross: "486200.00",
    differenceAmountGross: "7000.00",
    preparedBy: { ...ACTORS.prep },
    lockVersion: 4,
    subjectHash: "sh_jd_202607_draft",
    sourceAsOf: "2026-08-01T08:30:00+08:00",
    sourceSnapshotAt: "2026-08-01T08:30:00+08:00",
    sourceSnapshotHash: "ssh_jd_202607_a4f2",
    pendingCostDeltaGross: "7000.00",
    items: [
      item({
        itemId: "it1",
        supplierOrderNo: "SO-SF-77120",
        externalOrderNo: "JD-EX-9001",
        productName: "办公耗材套装 A",
        quantity: "120",
        factLabel: "已完成",
        orderAmountGross: "36000.00",
        freightGross: "600.00",
        serviceFeeGross: "200.00",
        refundGross: "0.00",
        erpAmountGross: "36800.00",
        supplierBillLineGross: "36800.00",
      }),
      item({
        itemId: "it2",
        supplierOrderNo: "SO-SF-77188",
        externalOrderNo: "JD-EX-9044",
        productName: "礼品卡实体卡",
        quantity: "50",
        factLabel: "已完成 + 部分退款",
        orderAmountGross: "50000.00",
        freightGross: "0.00",
        serviceFeeGross: "500.00",
        refundGross: "-2000.00",
        erpAmountGross: "48500.00",
        supplierBillLineGross: "50500.00",
      }),
      item({
        itemId: "it3",
        supplierOrderNo: "SO-SF-77201",
        externalOrderNo: "JD-EX-9100",
        productName: "企业购标准品",
        quantity: "200",
        factLabel: "已取消（不可变记录）",
        orderAmountGross: "0.00",
        freightGross: "0.00",
        serviceFeeGross: "0.00",
        refundGross: "0.00",
        erpAmountGross: "0.00",
        supplierBillLineGross: "5000.00",
      }),
    ],
    differences: [
      {
        differenceId: "df_jd_amt",
        type: "AMOUNT",
        status: "OPEN",
        blocking: true,
        erpSideLabel: "ERP 明细汇总（含税）",
        erpSideAmount: "48500.00",
        supplierSideLabel: "供应商账单行（含税）",
        supplierSideAmount: "50500.00",
        amountDirectionLabel: "供应商账单高于 ERP",
        amountGross: "2000.00",
        version: 1,
        requiresProcurementEvidence: true,
        evidence: [],
        leftFields: [
          {
            id: "l1",
            field: "结算行金额（含税）",
            before: "¥48,500.00",
            after: "¥50,500.00",
            note: "外部账单 v2 · 不可改写原值",
          },
          {
            id: "l2",
            field: "退款记录",
            before: "供应商退款 −¥2,000.00",
            after: "账单未体现退款",
            note: "以不可变退款记录为准",
          },
        ],
      },
      {
        differenceId: "df_jd_miss",
        type: "MISSING_ORDER",
        status: "EVIDENCE_PENDING",
        blocking: true,
        erpSideLabel: "ERP 无对应完成记录",
        supplierSideLabel: "账单含 JD-EX-9100",
        supplierSideAmount: "5000.00",
        amountDirectionLabel: "供应商账单高于 ERP",
        amountGross: "5000.00",
        version: 2,
        requiresProcurementEvidence: true,
        evidence: [
          {
            evidenceId: "ev1",
            kind: "TICKET",
            label: "工单 TKT-4481",
            comment: "供应商确认该单已取消，账单误挂",
            by: { ...ACTORS.procurement },
            at: "2026-07-30T15:20:00+08:00",
          },
        ],
        leftFields: [
          {
            id: "m1",
            field: "订单记录",
            before: "已取消 · 金额 0",
            after: "账单仍计 ¥5,000.00",
            note: "不得用 W26 数据改写历史",
          },
        ],
      },
    ],
    reviewRecords: [],
    auditEvents: [
      {
        eventId: "ae1",
        at: "2026-07-28T10:00:00+08:00",
        actor: "李经办",
        action: "CREATE_DRAFT",
        summary: "按期间策略 v3 创建草稿并冻结来源数据",
        auditNo: "AUD-W27-0701",
      },
      {
        eventId: "ae2",
        at: "2026-08-01T08:30:00+08:00",
        actor: "李经办",
        action: "REFRESH_TRIAL",
        summary: "刷新试算 · sourceSnapshotHash=ssh_jd_202607_a4f2",
        auditNo: "AUD-W27-0702",
      },
    ],
  },
  {
    statementId: "st_sf_202606",
    statementNo: "ST-2026-06-SF",
    supplierId: "sup_sf",
    supplierName: "顺丰同城",
    periodStart: "2026-06-01",
    periodEnd: "2026-06-30",
    periodLabel: "2026-06",
    status: "CONFIRMED",
    externalBillNo: "SF-BILL-202606",
    externalBillVersion: "v1",
    orderAmountGross: "58000.00",
    freightGross: "3200.00",
    serviceFeeGross: "1600.00",
    refundGross: "0.00",
    erpAmountGross: "62800.00",
    supplierAmountGross: "62800.00",
    differenceAmountGross: "0.00",
    preparedBy: { ...ACTORS.prep },
    reviewedBy: { ...ACTORS.review },
    lockVersion: 8,
    subjectHash: "sh_sf_202606_final",
    sourceAsOf: "2026-07-02T18:00:00+08:00",
    sourceSnapshotAt: "2026-07-02T18:00:00+08:00",
    sourceSnapshotHash: "ssh_sf_202606_final",
    confirmedCostDeltaGross: "0.00",
    payable: {
      payableAccountId: "pa_sf_0626",
      payableNo: "AP-SF-202606-01",
      grossAmount: "62800.00",
      dueDate: "2026-07-15",
      statusLabel: "未结",
    },
    items: [
      item({
        itemId: "sf1",
        supplierOrderNo: "SO-SF-66001",
        externalOrderNo: "SF-EX-2201",
        productName: "同城急送服务包",
        quantity: "80",
        factLabel: "已完成",
        orderAmountGross: "58000.00",
        freightGross: "3200.00",
        serviceFeeGross: "1600.00",
        refundGross: "0.00",
        erpAmountGross: "62800.00",
        supplierBillLineGross: "62800.00",
      }),
    ],
    differences: [],
    reviewRecords: [
      {
        recordId: "rr1",
        action: "SUBMIT",
        actionLabel: "提交复核",
        by: { ...ACTORS.prep },
        at: "2026-07-01T11:00:00+08:00",
        comment: "无未解决差异",
      },
      {
        recordId: "rr2",
        action: "CONFIRM",
        actionLabel: "确认结算",
        by: { ...ACTORS.review },
        at: "2026-07-02T16:30:00+08:00",
        comment: "确认形成应付",
      },
    ],
    auditEvents: [
      {
        eventId: "ae_sf1",
        at: "2026-07-02T16:30:00+08:00",
        actor: "王复核",
        action: "CONFIRM",
        summary: "确认结算 · 应付 AP-SF-202606-01 · 成本差额 0",
        auditNo: "AUD-W27-0601",
      },
    ],
  },
  {
    statementId: "st_mt_202607",
    statementNo: "ST-2026-07-MT",
    supplierId: "sup_mt",
    supplierName: "美团企业版",
    periodStart: "2026-07-01",
    periodEnd: "2026-07-31",
    periodLabel: "2026-07",
    status: "PENDING_REVIEW",
    externalBillNo: "MT-BILL-202607",
    externalBillVersion: "v1",
    orderAmountGross: "128400.00",
    freightGross: "0.00",
    serviceFeeGross: "2400.00",
    refundGross: "-1200.00",
    erpAmountGross: "129600.00",
    supplierAmountGross: "129600.00",
    differenceAmountGross: "0.00",
    preparedBy: { ...ACTORS.prep },
    lockVersion: 6,
    subjectHash: "sh_mt_202607_sub",
    sourceAsOf: "2026-07-31T18:00:00+08:00",
    sourceSnapshotAt: "2026-07-31T18:00:00+08:00",
    sourceSnapshotHash: "ssh_mt_202607_sub",
    workItem: {
      workItemId: "wi_mt_202607",
      subjectVersion: "6",
      subjectHash: "sh_mt_202607_sub",
      claimedBy: { ...ACTORS.review },
      leaseVersion: 1,
    },
    items: [
      item({
        itemId: "mt1",
        supplierOrderNo: "SO-MT-8801",
        externalOrderNo: "MT-EX-501",
        productName: "企业团餐券包",
        quantity: "300",
        factLabel: "已完成",
        orderAmountGross: "128400.00",
        freightGross: "0.00",
        serviceFeeGross: "2400.00",
        refundGross: "-1200.00",
        erpAmountGross: "129600.00",
        supplierBillLineGross: "129600.00",
      }),
    ],
    differences: [
      {
        differenceId: "df_mt_closed",
        type: "REFUND",
        status: "RESOLVED",
        blocking: false,
        erpSideLabel: "退款记录 −¥1,200.00",
        supplierSideLabel: "账单已扣减",
        amountDirectionLabel: "无差异",
        amountGross: "0.00",
        version: 3,
        requiresProcurementEvidence: false,
        evidence: [],
        resolution: {
          resolutionId: "res_mt1",
          resolution: "ERP_ACCEPTED",
          resolutionLabel: "ERP 认可",
          reasonCode: "BILL_ALIGNED",
          reasonLabel: "账单与退款记录一致",
          by: { ...ACTORS.prep },
          at: "2026-07-30T10:00:00+08:00",
          costImpactPreview: "0.00",
        },
        leftFields: [
          {
            id: "r1",
            field: "退款金额（含税）",
            before: "−¥1,200.00",
            after: "−¥1,200.00",
            note: "已对齐",
          },
        ],
      },
    ],
    reviewRecords: [
      {
        recordId: "rr_mt1",
        action: "SUBMIT",
        actionLabel: "提交复核",
        by: { ...ACTORS.prep },
        at: "2026-07-31T19:00:00+08:00",
        comment: "差异已处理，请求复核",
      },
    ],
    auditEvents: [
      {
        eventId: "ae_mt1",
        at: "2026-07-31T19:00:00+08:00",
        actor: "李经办",
        action: "SUBMIT_REVIEW",
        summary: "提交复核 · 数据版本=sh_mt_202607_sub",
        auditNo: "AUD-W27-0711",
      },
    ],
  },
  {
    statementId: "st_jd_202605",
    statementNo: "ST-2026-05-JD",
    supplierId: "sup_jd",
    supplierName: "京东企业购",
    periodStart: "2026-05-01",
    periodEnd: "2026-05-31",
    periodLabel: "2026-05",
    status: "PENDING_RECONCILE",
    externalBillNo: "JD-BILL-202605",
    externalBillVersion: "v1",
    orderAmountGross: "210000.00",
    freightGross: "4000.00",
    serviceFeeGross: "1800.00",
    refundGross: "0.00",
    erpAmountGross: "215800.00",
    supplierAmountGross: "215800.00",
    differenceAmountGross: "0.00",
    preparedBy: { ...ACTORS.prep },
    lockVersion: 2,
    subjectHash: "sh_jd_202605_a",
    sourceAsOf: "2026-06-01T12:00:00+08:00",
    sourceSnapshotAt: "2026-06-01T12:00:00+08:00",
    sourceSnapshotHash: "ssh_jd_202605_a",
    items: [
      item({
        itemId: "jd5_1",
        supplierOrderNo: "SO-JD-5501",
        externalOrderNo: "JD-EX-5501",
        productName: "标准 SKU 包",
        quantity: "100",
        factLabel: "已完成",
        orderAmountGross: "210000.00",
        freightGross: "4000.00",
        serviceFeeGross: "1800.00",
        refundGross: "0.00",
        erpAmountGross: "215800.00",
        supplierBillLineGross: "215800.00",
      }),
    ],
    differences: [],
    reviewRecords: [],
    auditEvents: [
      {
        eventId: "ae_jd5",
        at: "2026-06-01T12:00:00+08:00",
        actor: "李经办",
        action: "CREATE_DRAFT",
        summary: "创建 5 月草稿，待对账",
        auditNo: "AUD-W27-0501",
      },
    ],
  },
  {
    statementId: "st_sf_202607",
    statementNo: "ST-2026-07-SF",
    supplierId: "sup_sf",
    supplierName: "顺丰同城",
    periodStart: "2026-07-01",
    periodEnd: "2026-07-31",
    periodLabel: "2026-07",
    status: "DRAFT",
    orderAmountGross: "44200.00",
    freightGross: "2800.00",
    serviceFeeGross: "900.00",
    refundGross: "-400.00",
    erpAmountGross: "47500.00",
    supplierAmountGross: undefined,
    differenceAmountGross: undefined,
    preparedBy: { ...ACTORS.prep },
    lockVersion: 1,
    sourceAsOf: "2026-08-01T09:00:00+08:00",
    sourceSnapshotAt: "2026-08-01T09:00:00+08:00",
    sourceSnapshotHash: "ssh_sf_202607_draft1",
    items: [
      item({
        itemId: "sf7_1",
        supplierOrderNo: "SO-SF-77001",
        externalOrderNo: "SF-EX-7701",
        productName: "即时配送",
        quantity: "40",
        factLabel: "已完成",
        orderAmountGross: "44200.00",
        freightGross: "2800.00",
        serviceFeeGross: "900.00",
        refundGross: "-400.00",
        erpAmountGross: "47500.00",
      }),
    ],
    differences: [],
    reviewRecords: [],
    auditEvents: [
      {
        eventId: "ae_sf7",
        at: "2026-08-01T09:00:00+08:00",
        actor: "李经办",
        action: "CREATE_DRAFT",
        summary: "草稿 · 账单尚未同步",
        auditNo: "AUD-W27-0715",
      },
    ],
  },
  {
    statementId: "st_mt_202604",
    statementNo: "ST-2026-04-MT",
    supplierId: "sup_mt",
    supplierName: "美团企业版",
    periodStart: "2026-04-01",
    periodEnd: "2026-04-30",
    periodLabel: "2026-04",
    status: "CONFIRMED",
    externalBillNo: "MT-BILL-202604",
    externalBillVersion: "v1",
    orderAmountGross: "88000.00",
    freightGross: "0.00",
    serviceFeeGross: "1600.00",
    refundGross: "0.00",
    erpAmountGross: "89600.00",
    supplierAmountGross: "91200.00",
    differenceAmountGross: "1600.00",
    preparedBy: { ...ACTORS.prep },
    reviewedBy: { ...ACTORS.review },
    lockVersion: 10,
    subjectHash: "sh_mt_202604_final",
    sourceAsOf: "2026-05-05T18:00:00+08:00",
    sourceSnapshotAt: "2026-05-05T18:00:00+08:00",
    sourceSnapshotHash: "ssh_mt_202604_final",
    confirmedCostDeltaGross: "1600.00",
    payable: {
      payableAccountId: "pa_mt_0426",
      payableNo: "AP-MT-202604-01",
      grossAmount: "91200.00",
      dueDate: "2026-05-20",
      statusLabel: "部分结清",
    },
    items: [
      item({
        itemId: "mt4_1",
        supplierOrderNo: "SO-MT-4401",
        externalOrderNo: "MT-EX-401",
        productName: "企业餐券",
        quantity: "200",
        factLabel: "已完成",
        orderAmountGross: "88000.00",
        freightGross: "0.00",
        serviceFeeGross: "1600.00",
        refundGross: "0.00",
        erpAmountGross: "89600.00",
        supplierBillLineGross: "91200.00",
      }),
    ],
    differences: [
      {
        differenceId: "df_mt4",
        type: "AMOUNT",
        status: "RESOLVED",
        blocking: false,
        erpSideLabel: "ERP 含税",
        erpSideAmount: "89600.00",
        supplierSideLabel: "账单含税",
        supplierSideAmount: "91200.00",
        amountDirectionLabel: "供应商账单高于 ERP",
        amountGross: "1600.00",
        version: 2,
        requiresProcurementEvidence: false,
        evidence: [],
        resolution: {
          resolutionId: "res_mt4",
          resolution: "SUPPLIER_ACCEPTED",
          resolutionLabel: "供应商认可",
          reasonCode: "ACCEPT_BILL",
          reasonLabel: "接受账单，追加成本差额",
          by: { ...ACTORS.prep },
          at: "2026-05-04T14:00:00+08:00",
          costImpactPreview: "1600.00",
        },
        leftFields: [
          {
            id: "x1",
            field: "含税金额",
            before: "¥89,600.00",
            after: "¥91,200.00",
            note: "确认时追加 cost_entry",
          },
        ],
      },
    ],
    reviewRecords: [
      {
        recordId: "rr_mt4a",
        action: "SUBMIT",
        actionLabel: "提交复核",
        by: { ...ACTORS.prep },
        at: "2026-05-04T15:00:00+08:00",
      },
      {
        recordId: "rr_mt4b",
        action: "CONFIRM",
        actionLabel: "确认结算",
        by: { ...ACTORS.review },
        at: "2026-05-05T17:00:00+08:00",
      },
    ],
    auditEvents: [
      {
        eventId: "ae_mt4",
        at: "2026-05-05T17:00:00+08:00",
        actor: "王复核",
        action: "CONFIRM",
        summary: "确认 · 应付 AP-MT-202604-01 · 成本差额 +1600",
        auditNo: "AUD-W27-0401",
      },
    ],
  },
  {
    statementId: "st_jd_202608",
    statementNo: "ST-2026-08-JD",
    supplierId: "sup_jd",
    supplierName: "京东企业购",
    periodStart: "2026-08-01",
    periodEnd: "2026-08-31",
    periodLabel: "2026-08",
    status: "HAS_DIFFERENCE",
    externalBillNo: "JD-BILL-202608",
    externalBillVersion: "v1",
    orderAmountGross: "98000.00",
    freightGross: "2100.00",
    serviceFeeGross: "900.00",
    refundGross: "0.00",
    erpAmountGross: "101000.00",
    supplierAmountGross: "103500.00",
    differenceAmountGross: "2500.00",
    preparedBy: { ...ACTORS.prep },
    lockVersion: 2,
    subjectHash: "sh_jd_202608_a",
    sourceAsOf: "2026-08-01T10:00:00+08:00",
    sourceSnapshotAt: "2026-08-01T10:00:00+08:00",
    sourceSnapshotHash: "ssh_jd_202608_a",
    pendingCostDeltaGross: "2500.00",
    items: [
      item({
        itemId: "jd8_1",
        supplierOrderNo: "SO-JD-8801",
        externalOrderNo: "JD-EX-8801",
        productName: "8 月新品包",
        quantity: "60",
        factLabel: "已完成",
        orderAmountGross: "98000.00",
        freightGross: "2100.00",
        serviceFeeGross: "900.00",
        refundGross: "0.00",
        erpAmountGross: "101000.00",
        supplierBillLineGross: "103500.00",
      }),
    ],
    differences: [
      {
        differenceId: "df_jd8",
        type: "AMOUNT",
        status: "OPEN",
        blocking: true,
        erpSideLabel: "ERP 含税",
        erpSideAmount: "101000.00",
        supplierSideLabel: "账单含税",
        supplierSideAmount: "103500.00",
        amountDirectionLabel: "供应商账单高于 ERP",
        amountGross: "2500.00",
        version: 1,
        requiresProcurementEvidence: false,
        evidence: [],
        leftFields: [
          {
            id: "j81",
            field: "含税金额",
            before: "¥101,000.00",
            after: "¥103,500.00",
            note: "账单原值只读",
          },
        ],
      },
    ],
    reviewRecords: [],
    auditEvents: [
      {
        eventId: "ae_jd8",
        at: "2026-08-01T10:00:00+08:00",
        actor: "李经办",
        action: "CREATE_DRAFT",
        summary: "创建 8 月草稿",
        auditNo: "AUD-W27-0801",
      },
    ],
  },
]
