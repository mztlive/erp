/**
 * W08 采购单演示种子。
 * 金额为服务端舍入到分后的固定展示值；不在客户端重算正式税额。
 */

import type {
  PurchaseCreationBasis,
  PurchaseOrderCenterView,
  PurchaseOrderListItem,
  PurchaseOrderLineView,
  PurchaseOrderStatus,
  PurchaseReviewStatus,
  PurchaseType,
  FulfillmentResponsibility,
} from "@/features/purchase-orders/types"
import {
  FULFILLMENT_RESPONSIBILITY_LABEL,
  PO_STATUS_LABEL,
  PO_STATUS_TONE,
  PURCHASE_TYPE_LABEL,
  REVIEW_STATUS_LABEL,
} from "@/features/purchase-orders/types"

function lineAmounts(
  qty: number,
  unitGross: number,
  taxRate: number
): Pick<PurchaseOrderLineView, "grossAmount" | "netAmount" | "taxAmount"> {
  // 定点：行先舍入到分（演示固定算法，与页面只读服务端结果一致）
  const gross = Math.round(qty * unitGross * 100) / 100
  const net = Math.round((gross / (1 + taxRate)) * 100) / 100
  const tax = Math.round((gross - net) * 100) / 100
  return {
    grossAmount: gross.toFixed(2),
    netAmount: net.toFixed(2),
    taxAmount: tax.toFixed(2),
  }
}

function sumLines(lines: readonly PurchaseOrderLineView[]) {
  const gross = lines.reduce((s, l) => s + Number(l.grossAmount), 0)
  const net = lines.reduce((s, l) => s + Number(l.netAmount), 0)
  const tax = lines.reduce((s, l) => s + Number(l.taxAmount), 0)
  return {
    gross: gross.toFixed(2),
    net: net.toFixed(2),
    tax: tax.toFixed(2),
  }
}

type SeedLine = {
  lineId: string
  lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
  procurementConfirmationLineId?: string
  itemName: string
  itemSku?: string
  quantity?: string
  unit?: string
  unitCostGross: string
  inputTaxRate: string
  expectedDeliveryDate?: string
  logisticsFeeReason?: string
  salesAllocationLabel?: string
}

function buildLines(raw: SeedLine[]): PurchaseOrderLineView[] {
  return raw.map((r) => {
    const rate = Number(r.inputTaxRate)
    if (r.lineType === "LOGISTICS_FEE") {
      const unit = Number(r.unitCostGross)
      const amounts = lineAmounts(1, unit, rate)
      return {
        ...r,
        unitCostGross: Number(r.unitCostGross).toFixed(2),
        ...amounts,
      }
    }
    const qty = Number(r.quantity ?? "0")
    const unit = Number(r.unitCostGross)
    const amounts = lineAmounts(qty, unit, rate)
    return {
      ...r,
      unitCostGross: unit.toFixed(4),
      ...amounts,
    }
  })
}

type SeedPO = {
  purchaseOrderId: string
  purchaseNo?: string
  draftLabel?: string
  revisionNo?: number
  status: PurchaseOrderStatus
  reviewStatus: PurchaseReviewStatus
  salesOrderId: string
  salesOrderNo: string
  supplierId: string
  supplierName: string
  purchaseType: PurchaseType
  fulfillmentResponsibility: FulfillmentResponsibility
  paymentTermCode: string
  paymentTermLabel: string
  ownerName: string
  submittedBy?: string
  submittedAt?: string
  paymentProgress: string
  invoiceProgress: string
  fulfillmentProgress: string
  paymentGate: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
  prepayment: {
    state: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
    message: string
    required: string
    allocated: string
    gap: string
  }
  expectedDate?: string
  updatedAt: string
  lockVersion: number
  currentSubmissionId?: string
  currentRevisionId?: string
  subjectHash?: string
  creationBasisId?: string
  contentSource: "DRAFT" | "SUBMISSION" | "REVISION"
  lines: SeedLine[]
  payable?: {
    payableOpenAmount: string
    paidAllocatedAmount: string
    purchaseInvoiceAllocatedAmount: string
  }
  fulfillment: {
    progressLabel: string
    progressTone: "neutral" | "warning" | "success" | "info" | "destructive"
    inboundQty: string
    shippedQty: string
    remainingQty: string
    note?: string
  }
  changes: {
    changeId: string
    label: string
    statusLabel: string
    tone: "neutral" | "warning" | "success" | "info" | "destructive"
    baseRevisionNo?: number
  }[]
  workflow: {
    id: string
    actionLabel: string
    actorLabel: string
    at: string
    comment?: string
  }[]
  allowedActions: string[]
  actionBlockers: { action: string; code: string; message: string }[]
  reviewWorkItem?: {
    workItemId: string
    subjectHash: string
    subjectVersion: string
    submittedBy: string
  }
}

function toListItem(seed: SeedPO): PurchaseOrderListItem {
  const lines = buildLines(seed.lines)
  const totals = sumLines(lines)
  return {
    purchaseOrderId: seed.purchaseOrderId,
    purchaseNo: seed.purchaseNo,
    draftLabel: seed.draftLabel,
    revisionNo: seed.revisionNo,
    status: seed.status,
    statusLabel: PO_STATUS_LABEL[seed.status],
    statusTone: PO_STATUS_TONE[seed.status],
    reviewStatus: seed.reviewStatus,
    reviewLabel: REVIEW_STATUS_LABEL[seed.reviewStatus],
    salesOrderId: seed.salesOrderId,
    salesOrderNo: seed.salesOrderNo,
    supplierId: seed.supplierId,
    supplierName: seed.supplierName,
    purchaseType: seed.purchaseType,
    fulfillmentResponsibility: seed.fulfillmentResponsibility,
    paymentTermCode: seed.paymentTermCode,
    paymentTermLabel: seed.paymentTermLabel,
    ownerName: seed.ownerName,
    grossAmount: totals.gross,
    netAmount: totals.net,
    taxAmount: totals.tax,
    costMasked: false,
    paymentProgress: seed.paymentProgress,
    invoiceProgress: seed.invoiceProgress,
    fulfillmentProgress: seed.fulfillmentProgress,
    paymentGate: seed.paymentGate,
    expectedDate: seed.expectedDate,
    updatedAt: seed.updatedAt,
    allowedActions: seed.allowedActions,
    actionBlockers: seed.actionBlockers,
  }
}

function toCenter(seed: SeedPO): PurchaseOrderCenterView {
  const lines = buildLines(seed.lines)
  const totals = sumLines(lines)
  return {
    identity: {
      purchaseOrderId: seed.purchaseOrderId,
      purchaseNo: seed.purchaseNo,
      draftLabel: seed.draftLabel,
      status: seed.status,
      statusLabel: PO_STATUS_LABEL[seed.status],
      statusTone: PO_STATUS_TONE[seed.status],
      reviewStatus: seed.reviewStatus,
      reviewLabel: REVIEW_STATUS_LABEL[seed.reviewStatus],
      lockVersion: seed.lockVersion,
      currentSubmissionId: seed.currentSubmissionId,
      currentRevisionId: seed.currentRevisionId,
      revisionNo: seed.revisionNo,
      subjectHash: seed.subjectHash,
    },
    header: {
      salesOrderId: seed.salesOrderId,
      salesOrderNo: seed.salesOrderNo,
      supplierId: seed.supplierId,
      supplierSnapshot: seed.supplierName,
      purchaseType: seed.purchaseType,
      fulfillmentResponsibility: seed.fulfillmentResponsibility,
      paymentTermCode: seed.paymentTermCode,
      paymentTermLabel: seed.paymentTermLabel,
      ownerName: seed.ownerName,
      submittedBy: seed.submittedBy,
      submittedAt: seed.submittedAt,
      expectedDate: seed.expectedDate,
      creationBasisId: seed.creationBasisId,
    },
    progress: {
      payment: seed.paymentProgress,
      invoice: seed.invoiceProgress,
      fulfillment: seed.fulfillmentProgress,
      prepaymentGate: {
        state: seed.prepayment.state,
        message: seed.prepayment.message,
        required: seed.prepayment.required,
        allocated: seed.prepayment.allocated,
        gap: seed.prepayment.gap,
        updatedAt: seed.updatedAt,
      },
    },
    currentContent: {
      source: seed.contentSource,
      version: seed.revisionNo ?? seed.lockVersion,
      subjectHash: seed.subjectHash,
      lines,
      totals,
      costMasked: false,
    },
    allocations: lines
      .filter((l) => l.lineType === "ITEM_SERVICE")
      .map((l) => ({
        lineId: l.lineId,
        salesOrderLineLabel: l.salesAllocationLabel ?? l.itemName,
        allocatedQuantity: l.quantity ?? "0",
      })),
    payableSummary: seed.payable,
    fulfillmentSummary: seed.fulfillment,
    changes: seed.changes,
    workflow: seed.workflow,
    allowedActions: seed.allowedActions,
    actionBlockers: seed.actionBlockers,
    fieldVisibility: {
      grossAmount: "full",
      netAmount: "full",
      taxAmount: "full",
      unitCostGross: "full",
      supplierAccount: "full",
    },
    reviewWorkItem: seed.reviewWorkItem,
  }
}

const SEED: SeedPO[] = [
  {
    purchaseOrderId: "po_01",
    purchaseNo: "CG20260328001",
    revisionNo: undefined,
    status: "PENDING_REVIEW",
    reviewStatus: "PENDING",
    salesOrderId: "so_1002",
    salesOrderNo: "XS20260328002",
    supplierId: "sup_xg",
    supplierName: "鲜果直供供应链",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "PREPAY_100",
    paymentTermLabel: "先款 100% 后履约",
    ownerName: "赵强",
    submittedBy: "赵强",
    submittedAt: "2026-03-28 11:20",
    paymentProgress: "未付",
    invoiceProgress: "未收",
    fulfillmentProgress: "未开始",
    paymentGate: "BLOCKED",
    prepayment: {
      state: "BLOCKED",
      message: "先款条件未满足，禁止四类履约入口",
      required: "98000.00",
      allocated: "0.00",
      gap: "98000.00",
    },
    expectedDate: "2026-04-05",
    updatedAt: "2026-03-28 11:20",
    lockVersion: 3,
    currentSubmissionId: "posub_01_v1",
    subjectHash: "sha256:po01…a1",
    creationBasisId: "pcb_demo_01",
    contentSource: "SUBMISSION",
    lines: [
      {
        lineId: "pol_01_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_xg_1",
        itemName: "时令鲜果礼盒 A",
        itemSku: "SKU-FR-A",
        quantity: "200",
        unit: "箱",
        unitCostGross: "420.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-04-05",
        salesAllocationLabel: "销售行 · 鲜果礼盒 A ×200",
      },
      {
        lineId: "pol_01_2",
        lineType: "LOGISTICS_FEE",
        itemName: "入仓干线运费",
        unitCostGross: "14000.00",
        inputTaxRate: "0.09",
        logisticsFeeReason: "华东仓干线",
      },
    ],
    fulfillment: {
      progressLabel: "未开始",
      progressTone: "neutral",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: "200",
      note: "待财务审核通过后形成应付与履约资格",
    },
    changes: [],
    workflow: [
      {
        id: "wf_01_1",
        actionLabel: "提交财务审核",
        actorLabel: "赵强",
        at: "2026-03-28 11:20",
      },
    ],
    allowedActions: ["OPEN_CENTER", "REVIEW", "PRINT"],
    actionBlockers: [
      {
        action: "FULFILL",
        code: "NOT_EFFECTIVE",
        message: "采购单尚未生效，不能履约",
      },
    ],
    reviewWorkItem: {
      workItemId: "wi_po_review_01",
      subjectHash: "sha256:po01…a1",
      subjectVersion: "sub:1",
      submittedBy: "赵强",
    },
  },
  {
    purchaseOrderId: "po_02",
    purchaseNo: "CG20260327012",
    revisionNo: 2,
    status: "EFFECTIVE",
    reviewStatus: "APPROVED",
    salesOrderId: "so_1003",
    salesOrderNo: "XS20260327018",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "SUPPLIER_DIRECT",
    paymentTermCode: "PREPAY_50",
    paymentTermLabel: "先款 50% 后直发",
    ownerName: "赵强",
    submittedBy: "赵强",
    submittedAt: "2026-03-27 09:10",
    paymentProgress: "部分",
    invoiceProgress: "未收",
    fulfillmentProgress: "未开始",
    paymentGate: "BLOCKED",
    prepayment: {
      state: "BLOCKED",
      message: "有效已付净核销未达先款 50%",
      required: "107800.00",
      allocated: "50000.00",
      gap: "57800.00",
    },
    expectedDate: "2026-04-02",
    updatedAt: "2026-03-27 16:40",
    lockVersion: 5,
    currentSubmissionId: "posub_02_v1",
    currentRevisionId: "porev_02_v2",
    subjectHash: "sha256:po02…b2",
    creationBasisId: "pcb_demo_02",
    contentSource: "REVISION",
    lines: [
      {
        lineId: "pol_02_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_ly_1",
        itemName: "高定礼盒套装",
        itemSku: "SKU-GIFT-12",
        quantity: "280",
        unit: "套",
        unitCostGross: "770.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-04-02",
        salesAllocationLabel: "销售行 · 高定礼盒 ×280",
      },
    ],
    payable: {
      payableOpenAmount: "165600.00",
      paidAllocatedAmount: "50000.00",
      purchaseInvoiceAllocatedAmount: "0.00",
    },
    fulfillment: {
      progressLabel: "未开始（门禁阻塞）",
      progressTone: "warning",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: "280",
      note: "先款门禁阻塞，禁止直发登记",
    },
    changes: [],
    workflow: [
      {
        id: "wf_02_1",
        actionLabel: "财务审核通过",
        actorLabel: "财务 · 周敏",
        at: "2026-03-27 16:40",
        comment: "成本与付款条件核对通过",
      },
    ],
    allowedActions: [
      "OPEN_CENTER",
      "PAY",
      "START_CHANGE",
      "PRINT",
    ],
    actionBlockers: [
      {
        action: "FULFILL",
        code: "PREPAYMENT_GATE",
        message: "先款门禁未满足，请先完成有效付款核销",
      },
    ],
  },
  {
    purchaseOrderId: "po_03",
    draftLabel: "采购草稿 · 3a91",
    status: "DRAFT",
    reviewStatus: "NONE",
    salesOrderId: "so_1005",
    salesOrderNo: "XS20260325008",
    supplierId: "sup_yc",
    supplierName: "云仓配送服务",
    purchaseType: "SERVICE",
    fulfillmentResponsibility: "SERVICE",
    paymentTermCode: "POSTPAY_NET30",
    paymentTermLabel: "货到 30 天",
    ownerName: "陈璐",
    paymentProgress: "—",
    invoiceProgress: "—",
    fulfillmentProgress: "—",
    paymentGate: "NOT_APPLICABLE",
    prepayment: {
      state: "NOT_APPLICABLE",
      message: "后款条件，无先款门禁",
      required: "0.00",
      allocated: "0.00",
      gap: "0.00",
    },
    expectedDate: "2026-04-10",
    updatedAt: "2026-03-26 15:05",
    lockVersion: 2,
    creationBasisId: "pcb_demo_03",
    contentSource: "DRAFT",
    lines: [
      {
        lineId: "pol_03_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_yc_1",
        itemName: "线下安装服务",
        itemSku: "SVC-INSTALL",
        quantity: "12",
        unit: "次",
        unitCostGross: "1550.0000",
        inputTaxRate: "0.06",
        expectedDeliveryDate: "2026-04-10",
        salesAllocationLabel: "销售行 · 安装服务 ×12",
      },
    ],
    fulfillment: {
      progressLabel: "未开始",
      progressTone: "neutral",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: "12",
    },
    changes: [],
    workflow: [
      {
        id: "wf_03_1",
        actionLabel: "创建草稿",
        actorLabel: "陈璐",
        at: "2026-03-26 14:50",
      },
    ],
    allowedActions: ["EDIT", "SUBMIT", "VOID", "OPEN_CENTER"],
    actionBlockers: [
      {
        action: "REVIEW",
        code: "NOT_SUBMITTED",
        message: "草稿尚未提交，无审核任务",
      },
    ],
  },
  {
    purchaseOrderId: "po_04",
    purchaseNo: "CG20260325008",
    revisionNo: 1,
    status: "PARTIAL",
    reviewStatus: "APPROVED",
    salesOrderId: "so_1004",
    salesOrderNo: "XS20260324015",
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "POSTPAY_NET15",
    paymentTermLabel: "货到 15 天",
    ownerName: "赵强",
    submittedBy: "赵强",
    submittedAt: "2026-03-25 10:00",
    paymentProgress: "未付",
    invoiceProgress: "部分",
    fulfillmentProgress: "部分",
    paymentGate: "NOT_APPLICABLE",
    prepayment: {
      state: "NOT_APPLICABLE",
      message: "后款条件",
      required: "0.00",
      allocated: "0.00",
      gap: "0.00",
    },
    expectedDate: "2026-03-30",
    updatedAt: "2026-03-30 09:20",
    lockVersion: 6,
    currentSubmissionId: "posub_04_v1",
    currentRevisionId: "porev_04_v1",
    subjectHash: "sha256:po04…c4",
    contentSource: "REVISION",
    lines: [
      {
        lineId: "pol_04_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_hd_1",
        itemName: "员工福利大礼包",
        itemSku: "SKU-WL-01",
        quantity: "500",
        unit: "套",
        unitCostGross: "268.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-03-30",
        salesAllocationLabel: "销售行 · 福利礼包 ×500",
      },
    ],
    payable: {
      payableOpenAmount: "151420.00",
      paidAllocatedAmount: "0.00",
      purchaseInvoiceAllocatedAmount: "50000.00",
    },
    fulfillment: {
      progressLabel: "部分入库",
      progressTone: "info",
      inboundQty: "320",
      shippedQty: "0",
      remainingQty: "180",
    },
    changes: [],
    workflow: [
      {
        id: "wf_04_1",
        actionLabel: "财务审核通过",
        actorLabel: "财务 · 周敏",
        at: "2026-03-25 14:10",
      },
    ],
    allowedActions: ["OPEN_CENTER", "FULFILL", "PAY", "START_CHANGE", "PRINT"],
    actionBlockers: [],
  },
  {
    purchaseOrderId: "po_05",
    purchaseNo: "CG20260324003",
    revisionNo: 1,
    status: "EFFECTIVE",
    reviewStatus: "APPROVED",
    salesOrderId: "so_1006",
    salesOrderNo: "XS20260323009",
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    purchaseType: "VIRTUAL",
    fulfillmentResponsibility: "ELECTRONIC",
    paymentTermCode: "PREPAY_100",
    paymentTermLabel: "先款 100% 后开通",
    ownerName: "陈璐",
    submittedBy: "陈璐",
    submittedAt: "2026-03-24 13:30",
    paymentProgress: "已付",
    invoiceProgress: "完成",
    fulfillmentProgress: "未开始",
    paymentGate: "SATISFIED",
    prepayment: {
      state: "SATISFIED",
      message: "有效付款已满足先款 100%",
      required: "36000.00",
      allocated: "36000.00",
      gap: "0.00",
    },
    expectedDate: "2026-03-28",
    updatedAt: "2026-03-26 10:00",
    lockVersion: 4,
    currentSubmissionId: "posub_05_v1",
    currentRevisionId: "porev_05_v1",
    subjectHash: "sha256:po05…d5",
    contentSource: "REVISION",
    lines: [
      {
        lineId: "pol_05_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_xc_1",
        itemName: "电子权益包 · 季度",
        itemSku: "VIRT-Q1",
        quantity: "200",
        unit: "份",
        unitCostGross: "180.0000",
        inputTaxRate: "0.06",
        expectedDeliveryDate: "2026-03-28",
        salesAllocationLabel: "销售行 · 电子权益 ×200",
      },
    ],
    payable: {
      payableOpenAmount: "0.00",
      paidAllocatedAmount: "36000.00",
      purchaseInvoiceAllocatedAmount: "36000.00",
    },
    fulfillment: {
      progressLabel: "可电子交付",
      progressTone: "success",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: "200",
      note: "门禁已满足，可进入 W09 电子交付",
    },
    changes: [],
    workflow: [
      {
        id: "wf_05_1",
        actionLabel: "财务审核通过",
        actorLabel: "财务 · 周敏",
        at: "2026-03-24 16:00",
      },
    ],
    allowedActions: ["OPEN_CENTER", "FULFILL", "PAY", "START_CHANGE", "PRINT"],
    actionBlockers: [],
  },
  {
    purchaseOrderId: "po_06",
    draftLabel: "采购草稿 · 驳回重开 · 8f2c",
    status: "DRAFT",
    reviewStatus: "REJECTED",
    salesOrderId: "so_1007",
    salesOrderNo: "XS20260322011",
    supplierId: "sup_hf",
    supplierName: "恒丰礼赠有限公司",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "SUPPLIER_DIRECT",
    paymentTermCode: "PREPAY_30",
    paymentTermLabel: "先款 30%",
    ownerName: "赵强",
    submittedBy: "赵强",
    submittedAt: "2026-03-22 17:00",
    paymentProgress: "—",
    invoiceProgress: "—",
    fulfillmentProgress: "—",
    paymentGate: "NOT_APPLICABLE",
    prepayment: {
      state: "NOT_APPLICABLE",
      message: "未生效，无门禁",
      required: "0.00",
      allocated: "0.00",
      gap: "0.00",
    },
    expectedDate: "2026-04-01",
    updatedAt: "2026-03-23 09:40",
    lockVersion: 4,
    creationBasisId: "pcb_demo_06",
    contentSource: "DRAFT",
    lines: [
      {
        lineId: "pol_06_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_hf_1",
        itemName: "京津冀直发礼包",
        itemSku: "SKU-DIR-08",
        quantity: "150",
        unit: "套",
        unitCostGross: "435.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-04-01",
        salesAllocationLabel: "销售行 · 直发礼包 ×150",
      },
    ],
    fulfillment: {
      progressLabel: "未开始",
      progressTone: "neutral",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: "150",
    },
    changes: [],
    workflow: [
      {
        id: "wf_06_1",
        actionLabel: "财务驳回",
        actorLabel: "财务 · 周敏",
        at: "2026-03-23 09:40",
        comment: "税率与确认分行不一致，请修正后重新提交",
      },
    ],
    allowedActions: ["EDIT", "SUBMIT", "OPEN_CENTER"],
    actionBlockers: [
      {
        action: "REVIEW",
        code: "NOT_SUBMITTED",
        message: "需重新提交后才会创建新的审核任务",
      },
    ],
  },
  {
    purchaseOrderId: "po_07",
    purchaseNo: "CG20260320015",
    revisionNo: 3,
    status: "COMPLETED",
    reviewStatus: "APPROVED",
    salesOrderId: "so_1008",
    salesOrderNo: "XS20260318004",
    supplierId: "sup_xg",
    supplierName: "鲜果直供供应链",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "POSTPAY_NET30",
    paymentTermLabel: "货到 30 天",
    ownerName: "陈璐",
    submittedBy: "陈璐",
    submittedAt: "2026-03-20 11:00",
    paymentProgress: "已付",
    invoiceProgress: "完成",
    fulfillmentProgress: "完成",
    paymentGate: "NOT_APPLICABLE",
    prepayment: {
      state: "NOT_APPLICABLE",
      message: "后款，无先款门禁",
      required: "0.00",
      allocated: "0.00",
      gap: "0.00",
    },
    expectedDate: "2026-03-22",
    updatedAt: "2026-03-28 18:00",
    lockVersion: 8,
    currentSubmissionId: "posub_07_v1",
    currentRevisionId: "porev_07_v3",
    subjectHash: "sha256:po07…e7",
    contentSource: "REVISION",
    lines: [
      {
        lineId: "pol_07_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_xg_7",
        itemName: "季度水果补给箱",
        itemSku: "SKU-FR-Q",
        quantity: "80",
        unit: "箱",
        unitCostGross: "310.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-03-22",
        salesAllocationLabel: "销售行 · 水果补给 ×80",
      },
    ],
    payable: {
      payableOpenAmount: "0.00",
      paidAllocatedAmount: "28024.00",
      purchaseInvoiceAllocatedAmount: "28024.00",
    },
    fulfillment: {
      progressLabel: "已完成",
      progressTone: "success",
      inboundQty: "80",
      shippedQty: "80",
      remainingQty: "0",
    },
    changes: [
      {
        changeId: "poc_07_1",
        label: "数量调整变更 · 完成",
        statusLabel: "已生效 v3",
        tone: "success",
        baseRevisionNo: 2,
      },
    ],
    workflow: [
      {
        id: "wf_07_1",
        actionLabel: "采购变更生效",
        actorLabel: "系统",
        at: "2026-03-26 12:00",
      },
    ],
    allowedActions: ["OPEN_CENTER", "PRINT"],
    actionBlockers: [
      {
        action: "START_CHANGE",
        code: "COMPLETED",
        message: "已完成采购单不可再发起变更",
      },
      {
        action: "FULFILL",
        code: "COMPLETED",
        message: "履约已完成",
      },
    ],
  },
  {
    purchaseOrderId: "po_08",
    purchaseNo: "CG20260319002",
    revisionNo: 1,
    status: "EFFECTIVE",
    reviewStatus: "APPROVED",
    salesOrderId: "so_1009",
    salesOrderNo: "XS20260317020",
    supplierId: "sup_ly",
    supplierName: "礼遇包装工坊",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "PREPAY_100",
    paymentTermLabel: "先款 100%",
    ownerName: "赵强",
    submittedBy: "赵强",
    submittedAt: "2026-03-19 15:20",
    paymentProgress: "部分",
    invoiceProgress: "未收",
    fulfillmentProgress: "未开始",
    paymentGate: "BLOCKED",
    prepayment: {
      state: "BLOCKED",
      message: "有效付款不足 100%",
      required: "45200.00",
      allocated: "20000.00",
      gap: "25200.00",
    },
    expectedDate: "2026-03-25",
    updatedAt: "2026-03-21 08:30",
    lockVersion: 3,
    currentSubmissionId: "posub_08_v1",
    currentRevisionId: "porev_08_v1",
    subjectHash: "sha256:po08…f8",
    contentSource: "REVISION",
    lines: [
      {
        lineId: "pol_08_1",
        lineType: "ITEM_SERVICE",
        procurementConfirmationLineId: "cl_ly_8",
        itemName: "纸质礼袋套装",
        itemSku: "SKU-BAG-03",
        quantity: "400",
        unit: "套",
        unitCostGross: "98.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-03-25",
        salesAllocationLabel: "销售行 · 礼袋 ×400",
      },
      {
        lineId: "pol_08_2",
        lineType: "LOGISTICS_FEE",
        itemName: "入仓装卸费",
        unitCostGross: "6000.00",
        inputTaxRate: "0.06",
        logisticsFeeReason: "卸货与上架",
      },
    ],
    payable: {
      payableOpenAmount: "25200.00",
      paidAllocatedAmount: "20000.00",
      purchaseInvoiceAllocatedAmount: "0.00",
    },
    fulfillment: {
      progressLabel: "门禁阻塞",
      progressTone: "warning",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: "400",
    },
    changes: [],
    workflow: [
      {
        id: "wf_08_1",
        actionLabel: "财务审核通过",
        actorLabel: "财务 · 周敏",
        at: "2026-03-19 18:00",
      },
    ],
    allowedActions: ["OPEN_CENTER", "PAY", "START_CHANGE", "PRINT"],
    actionBlockers: [
      {
        action: "FULFILL",
        code: "PREPAYMENT_GATE",
        message: "先款门禁未满足，禁止入仓登记",
      },
    ],
  },
]

/** 可变会话覆盖前的不可变种子快照 */
export const MOCK_PURCHASE_ORDER_SEEDS: readonly SeedPO[] = SEED

export function listPurchaseOrderSeeds(): PurchaseOrderListItem[] {
  return SEED.map(toListItem)
}

export function getPurchaseOrderSeed(
  purchaseOrderId: string
): PurchaseOrderCenterView | null {
  const seed = SEED.find((s) => s.purchaseOrderId === purchaseOrderId)
  return seed ? toCenter(seed) : null
}

export function getPurchaseOrderSeedRaw(purchaseOrderId: string): SeedPO | null {
  return SEED.find((s) => s.purchaseOrderId === purchaseOrderId) ?? null
}

/** 可供 W07/W05 消费的采购创建依据（不依赖未注册 work_item） */
export const MOCK_CREATION_BASES: readonly PurchaseCreationBasis[] = [
  {
    basisId: "pcb_open_01",
    salesOrderId: "so_1010",
    salesOrderNo: "XS20260329001",
    salesSubmissionId: "sub_1010_v1",
    salesSubmissionNo: 1,
    supplierId: "sup_hd",
    supplierName: "华东优选供应链有限公司",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "POSTPAY_NET15",
    paymentTermLabel: "货到 15 天",
    lines: [
      {
        procurementConfirmationLineId: "cl_open_1",
        itemName: "中秋礼盒标准版",
        itemSku: "SKU-MQ-01",
        quantity: "100",
        unit: "套",
        unitCostGross: "388.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-04-12",
        salesAllocationLabel: "销售行 · 中秋礼盒 ×100",
      },
      {
        procurementConfirmationLineId: "cl_open_2",
        itemName: "中秋礼盒升级版",
        itemSku: "SKU-MQ-02",
        quantity: "40",
        unit: "套",
        unitCostGross: "520.0000",
        inputTaxRate: "0.13",
        expectedDeliveryDate: "2026-04-12",
        salesAllocationLabel: "销售行 · 升级礼盒 ×40",
      },
    ],
    estimatedGross: "59600.00",
    consumed: false,
  },
  {
    basisId: "pcb_open_02",
    salesOrderId: "so_1010",
    salesOrderNo: "XS20260329001",
    salesSubmissionId: "sub_1010_v1",
    salesSubmissionNo: 1,
    supplierId: "sup_xc",
    supplierName: "新程数字科技有限公司",
    purchaseType: "VIRTUAL",
    fulfillmentResponsibility: "ELECTRONIC",
    paymentTermCode: "PREPAY_100",
    paymentTermLabel: "先款 100%",
    lines: [
      {
        procurementConfirmationLineId: "cl_open_3",
        itemName: "电子祝福卡",
        itemSku: "VIRT-CARD",
        quantity: "300",
        unit: "份",
        unitCostGross: "12.0000",
        inputTaxRate: "0.06",
        expectedDeliveryDate: "2026-04-08",
        salesAllocationLabel: "销售行 · 电子祝福卡 ×300",
      },
    ],
    estimatedGross: "3600.00",
    consumed: false,
  },
  {
    // 演示：已消费依据不可重复建单
    basisId: "pcb_demo_01",
    salesOrderId: "so_1002",
    salesOrderNo: "XS20260328002",
    salesSubmissionId: "sub_1002_v1",
    salesSubmissionNo: 1,
    supplierId: "sup_xg",
    supplierName: "鲜果直供供应链",
    purchaseType: "PHYSICAL",
    fulfillmentResponsibility: "WAREHOUSE",
    paymentTermCode: "PREPAY_100",
    paymentTermLabel: "先款 100% 后履约",
    lines: [],
    estimatedGross: "98000.00",
    consumed: true,
  },
]

export {
  PURCHASE_TYPE_LABEL,
  FULFILLMENT_RESPONSIBILITY_LABEL,
  PO_STATUS_LABEL,
  buildLines,
  sumLines,
  toListItem,
  toCenter,
}

export type { SeedPO }
