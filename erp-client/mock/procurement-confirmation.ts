import type {
  ConfirmationLineDraft,
  CoverageByLine,
  ProcurementConfirmationTask,
  SubmissionLineView,
} from "@/features/procurement-confirmation/types"

function coverageFor(
  lines: readonly SubmissionLineView[],
  confirmationLines: readonly ConfirmationLineDraft[]
): CoverageByLine[] {
  return lines.map((line) => {
    const confirmed = confirmationLines
      .filter((c) => c.submissionLineId === line.submissionLineId)
      .reduce((sum, c) => sum + Number(c.confirmedQuantity || 0), 0)
    const required = Number(line.committedQuantity)
    const complete = confirmed + 1e-9 >= required && required > 0
    const gap = Math.max(0, required - confirmed)
    return {
      submissionLineId: line.submissionLineId,
      itemName: line.itemName,
      confirmed: confirmed.toFixed(0),
      required: line.committedQuantity,
      complete,
      gap: gap.toFixed(0),
    }
  })
}

function sumPurchase(lines: readonly ConfirmationLineDraft[]): string {
  const total = lines.reduce((sum, line) => {
    const qty = Number(line.confirmedQuantity || 0)
    const cost = Number(line.latestCostGross || 0)
    return sum + qty * cost
  }, 0)
  return total.toFixed(2)
}

function buildTask(
  partial: Omit<
    ProcurementConfirmationTask,
    "decisionSummary" | "confirmation"
  > & {
    confirmation: Omit<
      ProcurementConfirmationTask["confirmation"],
      never
    >
  }
): ProcurementConfirmationTask {
  const coverageByLine = coverageFor(
    partial.salesSubmission.lines,
    partial.confirmation.lines
  )
  const incomplete = coverageByLine.filter((c) => !c.complete)
  const invalidQual = partial.confirmation.lines.filter(
    (l) => l.qualificationStatus === "INVALID"
  )
  const blockingIssues = [
    ...incomplete.map((c) => ({
      code: "QTY_COVERAGE_INCOMPLETE",
      message: `「${c.itemName}」已确认 ${c.confirmed}/${c.required}，缺口 ${c.gap}`,
      lineId: c.submissionLineId,
    })),
    ...invalidQual.map((l) => ({
      code: "QUALIFICATION_INVALID",
      message: `供应商「${l.supplierName}」资质失效，不得通过`,
      lineId: l.submissionLineId,
    })),
  ]
  const lateLines = partial.confirmation.lines.filter((cl) => {
    const sub = partial.salesSubmission.lines.find(
      (s) => s.submissionLineId === cl.submissionLineId
    )
    return sub && cl.expectedDeliveryDate > sub.requestedDeliveryDate
  })
  const warnings = lateLines.map((l) => ({
    code: "DELIVERY_LATER_THAN_COMMITMENT",
    message: `「${l.supplierName}」预计交期 ${l.expectedDeliveryDate} 晚于客户期望`,
    lineId: l.submissionLineId,
  }))

  const purchase = sumPurchase(partial.confirmation.lines)
  const sales = Number(partial.salesSubmission.grossAmount)
  const margin =
    sales > 0
      ? (((sales - Number(purchase)) / sales) * 100).toFixed(2) + "%"
      : undefined

  return {
    ...partial,
    decisionSummary: {
      coverageByLine,
      estimatedPurchaseGross: purchase,
      estimatedMargin: margin,
      marginDelta: undefined,
      blockingIssues,
      warnings,
    },
  }
}

const task01Lines: ConfirmationLineDraft[] = [
  {
    lineKey: "cl_01_a",
    submissionLineId: "ssl_01_1",
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    confirmedQuantity: "180",
    latestCostGross: "420.00",
    inputTaxRate: "0.13",
    expectedDeliveryDate: "2026-08-07",
    fulfillmentMode: "WAREHOUSE",
    capabilityRevisionId: "cap_hd_v3",
    capabilitySummary: "礼包仓发 · 华东仓",
    qualificationStatus: "VALID",
  },
  {
    lineKey: "cl_01_b",
    submissionLineId: "ssl_01_1",
    supplierId: "sup_hf",
    supplierName: "恒丰礼赠有限公司",
    confirmedQuantity: "120",
    latestCostGross: "435.00",
    inputTaxRate: "0.13",
    expectedDeliveryDate: "2026-08-06",
    fulfillmentMode: "SUPPLIER_DIRECT",
    capabilityRevisionId: "cap_hf_v2",
    capabilitySummary: "礼包直发 · 京津冀",
    qualificationStatus: "VALID",
  },
  {
    lineKey: "cl_01_c",
    submissionLineId: "ssl_01_2",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    confirmedQuantity: "200",
    latestCostGross: "180.00",
    inputTaxRate: "0.06",
    expectedDeliveryDate: "2026-08-08",
    fulfillmentMode: "ELECTRONIC",
    capabilityRevisionId: "cap_xc_v1",
    capabilitySummary: "电子权益 · 即时开通",
    qualificationStatus: "VALID",
  },
]

const task02Lines: ConfirmationLineDraft[] = [
  {
    lineKey: "cl_02_a",
    submissionLineId: "ssl_02_1",
    supplierId: "sup_hf",
    supplierName: "恒丰礼赠有限公司",
    confirmedQuantity: "100",
    latestCostGross: "980.00",
    inputTaxRate: "0.13",
    expectedDeliveryDate: "2026-08-04",
    fulfillmentMode: "WAREHOUSE",
    capabilityRevisionId: "cap_hf_v2",
    capabilitySummary: "户外套装 · 华北仓",
    qualificationStatus: "VALID",
  },
  {
    lineKey: "cl_02_b",
    submissionLineId: "ssl_02_1",
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    confirmedQuantity: "40",
    latestCostGross: "995.00",
    inputTaxRate: "0.13",
    expectedDeliveryDate: "2026-08-05",
    fulfillmentMode: "SUPPLIER_DIRECT",
    capabilityRevisionId: "cap_hd_v3",
    capabilitySummary: "户外套装 · 直发",
    qualificationStatus: "EXPIRING",
  },
  {
    lineKey: "cl_02_c",
    submissionLineId: "ssl_02_2",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    confirmedQuantity: "50",
    latestCostGross: "220.00",
    inputTaxRate: "0.06",
    expectedDeliveryDate: "2026-08-10",
    fulfillmentMode: "SERVICE",
    capabilityRevisionId: "cap_xc_v1",
    capabilitySummary: "健康服务兑换",
    qualificationStatus: "VALID",
  },
]

const task03Lines: ConfirmationLineDraft[] = [
  {
    lineKey: "cl_03_a",
    submissionLineId: "ssl_03_1",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    confirmedQuantity: "500",
    latestCostGross: "495.00",
    inputTaxRate: "0.06",
    expectedDeliveryDate: "2026-08-14",
    fulfillmentMode: "ELECTRONIC",
    capabilityRevisionId: "cap_xc_v1",
    capabilitySummary: "健康服务兑换权益",
    qualificationStatus: "VALID",
  },
  {
    lineKey: "cl_03_b",
    submissionLineId: "ssl_03_2",
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    confirmedQuantity: "120",
    latestCostGross: "68.00",
    inputTaxRate: "0.13",
    expectedDeliveryDate: "2026-08-12",
    fulfillmentMode: "WAREHOUSE",
    capabilityRevisionId: "cap_hd_v3",
    capabilitySummary: "实物配件 · 仓发",
    qualificationStatus: "VALID",
  },
]

/** 二次确认队列样板：不可变提交 + 多供应商确认分行。 */
export const PROCUREMENT_CONFIRMATION_SEED: readonly ProcurementConfirmationTask[] =
  [
    buildTask({
      workItemId: "wi_pc_01",
      responsibilityScope: "mine",
      status: "IN_PROGRESS",
      priority: 20,
      dueAt: "2026-08-01T18:00:00+08:00",
      impactSummary: "影响客户 8 月 8 日交付窗口",
      subjectVersion: "sub_v2",
      subjectHash: "sha256:a1b2c3d4e5f6789012345678abcdef01",
      lease: {
        claimedByLabel: "当前用户 · 李采购",
        expiresAt: "2026-08-01T10:30:00+08:00",
        leaseVersion: 1,
      },
      salesSubmission: {
        salesOrderId: "so_1001",
        salesOrderNo: "XS20260328001",
        submissionId: "sosub_1001_02",
        submissionNo: 2,
        subjectHash: "sha256:a1b2c3d4e5f6789012345678abcdef01",
        subjectHashSummary: "a1b2c3d4…ef01",
        submittedAt: "2026-08-01 08:42",
        submittedByLabel: "王敏",
        customerSnapshot: "星河制造股份有限公司",
        contractSnapshot: "HT-2026-0312",
        paymentTermLabel: "货到 30 日内付款",
        grossAmount: "186000.00",
        origin: "INITIAL",
        lines: [
          {
            submissionLineId: "ssl_01_1",
            itemName: "员工关怀礼包 A",
            itemSku: "SKU-CARE-A",
            committedQuantity: "300",
            unit: "套",
            requestedDeliveryDate: "2026-08-08",
            referenceSupplier: "华东优选供应链有限公司",
            referenceCost: "430.00",
            salesAmountGross: "136000.00",
          },
          {
            submissionLineId: "ssl_01_2",
            itemName: "定制贺卡套装",
            itemSku: "SKU-CARD-02",
            committedQuantity: "200",
            unit: "套",
            requestedDeliveryDate: "2026-08-10",
            referenceSupplier: "新程数字科技有限公司",
            referenceCost: "185.00",
            salesAmountGross: "50000.00",
          },
        ],
      },
      confirmation: {
        confirmationId: "pc_1001_02",
        status: "PENDING",
        editVersion: 3,
        lines: task01Lines,
      },
      allowedActions: ["SAVE", "DEFER", "REJECT", "APPROVE", "CLAIM"],
      actionBlockers: [],
      riskLabel: "交期需确认",
      riskTone: "warning",
      riskDescription:
        "供应商承诺 8 月 7 日到仓，距离客户最晚交付仅 1 天；分行覆盖已完整。",
    }),
    buildTask({
      workItemId: "wi_pc_02",
      responsibilityScope: "mine",
      status: "PENDING",
      priority: 30,
      dueAt: "2026-07-31T18:00:00+08:00",
      impactSummary: "确认截止已过，客户要求首批 8 月 3 日交付",
      subjectVersion: "sub_v1",
      subjectHash: "sha256:b2c3d4e5f6789012345678abcdef012a",
      salesSubmission: {
        salesOrderId: "so_1002",
        salesOrderNo: "XS20260327012",
        submissionId: "sosub_1002_01",
        submissionNo: 1,
        subjectHash: "sha256:b2c3d4e5f6789012345678abcdef012a",
        subjectHashSummary: "b2c3d4e5…012a",
        submittedAt: "2026-07-31 16:18",
        submittedByLabel: "周航",
        customerSnapshot: "北辰能源集团",
        contractSnapshot: "HT-2026-0290",
        paymentTermLabel: "预付 30%，到货付清",
        grossAmount: "268800.00",
        origin: "CHANGED_TERMS_AFTER_REJECTION",
        resubmissionContext: {
          origin: "CHANGED_TERMS_AFTER_REJECTION",
          previousRejectedConfirmationId: "pc_1002_00",
          previousRejectedSubmissionId: "sosub_1002_00",
          previousRejectedSubjectHash: "sha256:old_rejected_hash_1002",
        },
        lines: [
          {
            submissionLineId: "ssl_02_1",
            itemName: "户外保障套装",
            itemSku: "SKU-OUT-01",
            committedQuantity: "160",
            unit: "套",
            requestedDeliveryDate: "2026-08-03",
            referenceSupplier: "恒丰礼赠有限公司",
            referenceCost: "990.00",
            salesAmountGross: "220800.00",
          },
          {
            submissionLineId: "ssl_02_2",
            itemName: "健康服务兑换权益",
            itemSku: "SKU-SVC-H",
            committedQuantity: "80",
            unit: "份",
            requestedDeliveryDate: "2026-08-08",
            referenceSupplier: "新程数字科技有限公司",
            referenceCost: "230.00",
            salesAmountGross: "48000.00",
          },
        ],
      },
      confirmation: {
        confirmationId: "pc_1002_01",
        status: "PENDING",
        editVersion: 1,
        // 故意留下第二行缺口：50/80，演示逐明细覆盖
        lines: task02Lines,
      },
      allowedActions: ["CLAIM", "SAVE", "DEFER", "REJECT", "APPROVE"],
      actionBlockers: [
        {
          action: "APPROVE",
          code: "QTY_COVERAGE_INCOMPLETE",
          message: "存在未完整覆盖的销售明细，系统将拒绝通过",
        },
      ],
      riskLabel: "任务已超期 · 覆盖缺口",
      riskTone: "destructive",
      riskDescription:
        "改品改价重提任务；户外套装已拆分两家供应商，健康权益仍差 30 份。",
    }),
    buildTask({
      workItemId: "wi_pc_03",
      responsibilityScope: "role_pool",
      status: "PENDING",
      priority: 10,
      dueAt: "2026-08-05T18:00:00+08:00",
      impactSummary: "常规确认 · 信息完整",
      subjectVersion: "sub_v1",
      subjectHash: "sha256:c3d4e5f6789012345678abcdef012ab3",
      salesSubmission: {
        salesOrderId: "so_1003",
        salesOrderNo: "XS20260326009",
        submissionId: "sosub_1003_01",
        submissionNo: 1,
        subjectHash: "sha256:c3d4e5f6789012345678abcdef012ab3",
        subjectHashSummary: "c3d4e5f6…2ab3",
        submittedAt: "2026-08-01 09:12",
        submittedByLabel: "王敏",
        customerSnapshot: "海纳教育科技有限公司",
        contractSnapshot: "HT-2026-0271",
        paymentTermLabel: "月结 45 天",
        grossAmount: "325000.00",
        origin: "LOW_MARGIN_MANAGER_APPROVED",
        resubmissionContext: {
          origin: "LOW_MARGIN_MANAGER_APPROVED",
          previousRejectedConfirmationId: "pc_1003_00",
          previousRejectedSubmissionId: "sosub_1003_00",
          previousRejectedSubjectHash: "sha256:old_rejected_hash_1003",
          lowMarginManagerConfirmationEvidenceReference: "LMMC-2026-0811-03",
        },
        lines: [
          {
            submissionLineId: "ssl_03_1",
            itemName: "健康服务兑换权益",
            itemSku: "SKU-SVC-H",
            committedQuantity: "500",
            unit: "份",
            requestedDeliveryDate: "2026-08-15",
            referenceSupplier: "新程数字科技有限公司",
            referenceCost: "500.00",
            salesAmountGross: "300000.00",
          },
          {
            submissionLineId: "ssl_03_2",
            itemName: "配套说明手册",
            itemSku: "SKU-MANUAL-1",
            committedQuantity: "120",
            unit: "册",
            requestedDeliveryDate: "2026-08-12",
            referenceSupplier: "华东优选供应链有限公司",
            referenceCost: "70.00",
            salesAmountGross: "25000.00",
          },
        ],
      },
      confirmation: {
        confirmationId: "pc_1003_01",
        status: "PENDING",
        editVersion: 1,
        lines: task03Lines,
      },
      allowedActions: ["CLAIM", "SAVE", "DEFER", "REJECT", "APPROVE"],
      actionBlockers: [],
      riskLabel: "低毛利上级已通过 · 仍待采购确认",
      riskTone: "info",
      riskDescription:
        "上级确认证据 LMMC-2026-0811-03 仅证明公司愿承担低毛利，不预填供应商、不自动通过。",
    }),
  ]

/** @deprecated 旧扁平列表兼容导出，请使用 seed + session 投影。 */
export const PROCUREMENT_CONFIRMATION_TASKS = PROCUREMENT_CONFIRMATION_SEED
