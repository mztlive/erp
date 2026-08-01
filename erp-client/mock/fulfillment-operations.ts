/**
 * W09 履约作业种子数据（演示）。
 * 覆盖入库 / 仓发 / 直发 / 电子 / 服务五类，保证默认队列 ≥5 条可见。
 */

import type {
  FulfillmentDraft,
  FulfillmentTask,
} from "@/features/fulfillment-operations/types"

function localNowInput(offsetMinutes = 0): string {
  const d = new Date(Date.now() + offsetMinutes * 60_000)
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`
}

const receiptDraft = (
  warehouseId: string,
  warehouseLabel: string,
  line: {
    purchaseRevisionLineId: string
    remaining: string
  }
): FulfillmentDraft => ({
  type: "RECEIPT",
  warehouseId,
  warehouseLabel,
  occurredAt: localNowInput(),
  lines: [
    {
      purchaseRevisionLineId: line.purchaseRevisionLineId,
      receivedQuantity: line.remaining,
      qualifiedQuantity: line.remaining,
      rejectedQuantity: "0",
      qualityResult: "合格",
      evidenceNote: "",
    },
  ],
})

const shipDraft = (
  warehouseId: string,
  warehouseLabel: string,
  line: {
    salesOrderLineId: string
    stockReservationId: string
    remaining: string
  }
): FulfillmentDraft => ({
  type: "WAREHOUSE_SHIP",
  warehouseId,
  warehouseLabel,
  carrier: "顺丰速运",
  trackingNo: "",
  shippedAt: localNowInput(),
  evidenceNote: "",
  lines: [
    {
      salesOrderLineId: line.salesOrderLineId,
      stockReservationId: line.stockReservationId,
      quantity: line.remaining,
    },
  ],
})

const directDraft = (line: {
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  remaining: string
}): FulfillmentDraft => ({
  type: "SUPPLIER_DIRECT",
  carrier: "供应商物流",
  trackingNo: "",
  shippedAt: localNowInput(),
  evidenceNote: "",
  lines: [
    {
      salesOrderLineId: line.salesOrderLineId,
      purchaseLineSalesAllocationId: line.purchaseLineSalesAllocationId,
      quantity: line.remaining,
    },
  ],
})

const electronicDraft = (line: {
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  remaining: string
}): FulfillmentDraft => ({
  type: "ELECTRONIC",
  occurredAt: localNowInput(),
  recipientMasked: "收件人 ·••• 手机 ·•••8123",
  result: "SUCCESS",
  evidenceNote: "",
  lines: [
    {
      salesOrderLineId: line.salesOrderLineId,
      purchaseLineSalesAllocationId: line.purchaseLineSalesAllocationId,
      quantity: line.remaining,
      evidenceNote: "",
    },
  ],
})

const serviceDraft = (line: {
  salesOrderLineId: string
  purchaseLineSalesAllocationId: string
  remaining: string
}): FulfillmentDraft => ({
  type: "SERVICE",
  startedAt: localNowInput(-120),
  endedAt: localNowInput(),
  serviceLocation: "客户现场 · 地址已掩码",
  result: "SUCCESS",
  completionNote: "",
  evidenceNote: "",
  lines: [
    {
      salesOrderLineId: line.salesOrderLineId,
      purchaseLineSalesAllocationId: line.purchaseLineSalesAllocationId,
      quantity: line.remaining,
      evidenceNote: "",
    },
  ],
})

export const FULFILLMENT_OPERATIONS_SEED: readonly FulfillmentTask[] = [
  {
    workItemId: "wi_ff_receipt_01",
    operationType: "RECEIPT",
    priority: 90,
    dueAt: "2026-08-01T11:00:00+08:00",
    dueLabel: "今天 11:00",
    overdue: true,
    statusLabel: "待处理",
    statusTone: "warning",
    responsibleLabel: "仓储 · 周航",
    sourceVersion: "v3",
    subjectHash: "sha_ff_receipt_01_v3",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2001",
      purchaseNo: "CG20260328011",
      purchaseRevisionId: "pr_2001_v3",
      salesOrderId: "so_1002",
      salesOrderNo: "XS20260327018",
      salesRevisionId: "sr_1002_v2",
      supplierLabel: "华东优选供应链有限公司",
      customerLabel: "青禾科技有限公司",
      warehouseId: "wh_east_1",
      warehouseLabel: "华东一号仓",
    },
    gate: {
      state: "SATISFIED",
      message: "先款条件已满足，可登记入库。",
      effectivePaidAmount: "128000.00",
      requiredAmount: "100000.00",
    },
    lines: [
      {
        lineId: "ffl_r01",
        salesOrderLineId: "sol_1002_01",
        purchaseRevisionLineId: "prl_2001_01",
        itemName: "企业年节礼盒 A",
        skuCode: "SKU-GIFT-A",
        unitCode: "套",
        orderedQuantity: "200",
        remainingQuantity: "100",
        purchaseLineSalesAllocationId: "plsa_2001_01",
      },
    ],
    draft: receiptDraft("wh_east_1", "华东一号仓", {
      purchaseRevisionLineId: "prl_2001_01",
      remaining: "100",
    }),
    summary: "采购到货待入库 · 合格量将形成库存与销售预占",
    impact: "阻塞后续仓发与客户验收窗口",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_ship_01",
    operationType: "WAREHOUSE_SHIP",
    priority: 85,
    dueAt: "2026-08-01T15:00:00+08:00",
    dueLabel: "今天 15:00",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "warning",
    responsibleLabel: "仓储 · 周航",
    sourceVersion: "v2",
    subjectHash: "sha_ff_ship_01_v2",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2001",
      purchaseNo: "CG20260328011",
      salesOrderId: "so_1002",
      salesOrderNo: "XS20260327018",
      salesRevisionId: "sr_1002_v2",
      supplierLabel: "华东优选供应链有限公司",
      customerLabel: "青禾科技有限公司",
      warehouseId: "wh_east_1",
      warehouseLabel: "华东一号仓",
    },
    gate: {
      state: "NOT_APPLICABLE",
      message: "公司仓发以有效预占与可用库存为门禁，不另造付款规则。",
    },
    lines: [
      {
        lineId: "ffl_s01",
        salesOrderLineId: "sol_1002_02",
        itemName: "企业年节礼盒 B",
        skuCode: "SKU-GIFT-B",
        unitCode: "套",
        orderedQuantity: "80",
        remainingQuantity: "80",
        stockReservationId: "rsv_1002_02",
        reservedQuantity: "80",
        availableOnHand: "120",
      },
    ],
    draft: shipDraft("wh_east_1", "华东一号仓", {
      salesOrderLineId: "sol_1002_02",
      stockReservationId: "rsv_1002_02",
      remaining: "80",
    }),
    summary: "销售预占已就绪，待公司仓发出库",
    impact: "客户要求今日达；延迟将影响验收窗口",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_direct_01",
    operationType: "SUPPLIER_DIRECT",
    priority: 70,
    dueAt: "2026-08-01T17:00:00+08:00",
    dueLabel: "今天 17:00",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "info",
    responsibleLabel: "采购 · 李采",
    sourceVersion: "v1",
    subjectHash: "sha_ff_direct_01_v1",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2008",
      purchaseNo: "CG20260330004",
      purchaseRevisionId: "pr_2008_v1",
      salesOrderId: "so_1002",
      salesOrderNo: "XS20260327018",
      salesRevisionId: "sr_1002_v2",
      supplierLabel: "南岭食品直供",
      customerLabel: "青禾科技有限公司",
    },
    gate: {
      state: "SATISFIED",
      message: "直发付款门禁已满足。",
      effectivePaidAmount: "36000.00",
      requiredAmount: "36000.00",
    },
    lines: [
      {
        lineId: "ffl_d01",
        salesOrderLineId: "sol_1002_03",
        purchaseRevisionLineId: "prl_2008_01",
        itemName: "定制茶礼组合",
        skuCode: "SKU-TEA-C",
        unitCode: "箱",
        orderedQuantity: "40",
        remainingQuantity: "40",
        purchaseLineSalesAllocationId: "plsa_2008_01",
      },
    ],
    draft: directDraft({
      salesOrderLineId: "sol_1002_03",
      purchaseLineSalesAllocationId: "plsa_2008_01",
      remaining: "40",
    }),
    summary: "供应商直发客户 · 不写自有库存流水",
    impact: "登记后销售可在 W06 验收；物流签收≠验收",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_electronic_01",
    operationType: "ELECTRONIC",
    priority: 65,
    dueAt: "2026-08-01T18:00:00+08:00",
    dueLabel: "今天 18:00",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "info",
    responsibleLabel: "采购 · 李采",
    sourceVersion: "v1",
    subjectHash: "sha_ff_electronic_01_v1",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2012",
      purchaseNo: "CG20260401002",
      purchaseRevisionId: "pr_2012_v1",
      salesOrderId: "so_1004",
      salesOrderNo: "XS20260329006",
      salesRevisionId: "sr_1004_v1",
      supplierLabel: "云启数字权益",
      customerLabel: "星河传媒",
    },
    gate: {
      state: "SATISFIED",
      message: "电子交付付款门禁已满足。",
      effectivePaidAmount: "18000.00",
      requiredAmount: "15000.00",
    },
    lines: [
      {
        lineId: "ffl_e01",
        salesOrderLineId: "sol_1004_01",
        purchaseRevisionLineId: "prl_2012_01",
        itemName: "电子提货券包",
        skuCode: "SKU-E-VOUCHER",
        unitCode: "份",
        orderedQuantity: "50",
        remainingQuantity: "50",
        purchaseLineSalesAllocationId: "plsa_2012_01",
      },
    ],
    draft: electronicDraft({
      salesOrderLineId: "sol_1004_01",
      purchaseLineSalesAllocationId: "plsa_2012_01",
      remaining: "50",
    }),
    summary: "电子交付待确认 · 不保存卡号/卡密明文",
    impact: "失败不可覆盖，重做须新建记录",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_service_01",
    operationType: "SERVICE",
    priority: 60,
    dueAt: "2026-08-01T12:00:00+08:00",
    dueLabel: "今天 12:00",
    overdue: true,
    statusLabel: "待处理",
    statusTone: "warning",
    responsibleLabel: "采购 · 李采",
    sourceVersion: "v1",
    subjectHash: "sha_ff_service_01_v1",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2015",
      purchaseNo: "CG20260402008",
      purchaseRevisionId: "pr_2015_v1",
      salesOrderId: "so_1005",
      salesOrderNo: "XS20260326004",
      salesRevisionId: "sr_1005_v1",
      supplierLabel: "蓝湾活动执行",
      customerLabel: "蓝湾集团",
    },
    gate: {
      state: "SATISFIED",
      message: "服务履约付款门禁已满足。",
      effectivePaidAmount: "22000.00",
      requiredAmount: "20000.00",
    },
    lines: [
      {
        lineId: "ffl_svc01",
        salesOrderLineId: "sol_1005_01",
        purchaseRevisionLineId: "prl_2015_01",
        itemName: "年节礼包现场布置",
        skuCode: "SKU-SVC-SETUP",
        unitCode: "场",
        orderedQuantity: "2",
        remainingQuantity: "1",
        purchaseLineSalesAllocationId: "plsa_2015_01",
      },
    ],
    draft: serviceDraft({
      salesOrderLineId: "sol_1005_01",
      purchaseLineSalesAllocationId: "plsa_2015_01",
      remaining: "1",
    }),
    summary: "线下服务完成，等待服务履约事实登记",
    impact: "登记后才可进入客户验收",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_receipt_02",
    operationType: "RECEIPT",
    priority: 55,
    dueAt: "2026-08-02T10:00:00+08:00",
    dueLabel: "明天 10:00",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "info",
    responsibleLabel: "仓储 · 周航",
    sourceVersion: "v1",
    subjectHash: "sha_ff_receipt_02_v1",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2018",
      purchaseNo: "CG20260405001",
      purchaseRevisionId: "pr_2018_v1",
      salesOrderId: "so_1008",
      salesOrderNo: "XS20260401009",
      salesRevisionId: "sr_1008_v1",
      supplierLabel: "北辰包装",
      customerLabel: "远景智造",
      warehouseId: "wh_north_2",
      warehouseLabel: "华北二号仓",
    },
    gate: {
      state: "BLOCKED",
      message: "先款净核销不足，禁止入库过账。请先至 W12 登记有效付款。",
      effectivePaidAmount: "5000.00",
      requiredAmount: "28000.00",
    },
    lines: [
      {
        lineId: "ffl_r02",
        salesOrderLineId: "sol_1008_01",
        purchaseRevisionLineId: "prl_2018_01",
        itemName: "定制礼盒外壳",
        skuCode: "SKU-BOX-N",
        unitCode: "件",
        orderedQuantity: "500",
        remainingQuantity: "500",
        purchaseLineSalesAllocationId: "plsa_2018_01",
      },
    ],
    draft: receiptDraft("wh_north_2", "华北二号仓", {
      purchaseRevisionLineId: "prl_2018_01",
      remaining: "500",
    }),
    summary: "到货待入库 · 付款门禁阻塞",
    impact: "门禁未满足时正式过账将被服务端拒绝",
    allowedActions: ["SAVE", "DEFER"],
    actionBlockers: [
      {
        action: "POST",
        code: "PREPAYMENT_BLOCKED",
        message: "付款门禁未满足，不能过账入库",
      },
    ],
  },
  {
    workItemId: "wi_ff_ship_02",
    operationType: "WAREHOUSE_SHIP",
    priority: 50,
    dueAt: "2026-08-02T16:00:00+08:00",
    dueLabel: "明天 16:00",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "info",
    responsibleLabel: "仓储 · 周航",
    sourceVersion: "v1",
    subjectHash: "sha_ff_ship_02_v1",
    editVersion: 1,
    source: {
      salesOrderId: "so_1008",
      salesOrderNo: "XS20260401009",
      salesRevisionId: "sr_1008_v1",
      customerLabel: "远景智造",
      warehouseId: "wh_north_2",
      warehouseLabel: "华北二号仓",
    },
    gate: {
      state: "NOT_APPLICABLE",
      message: "公司仓发以有效预占与可用库存为门禁。",
    },
    lines: [
      {
        lineId: "ffl_s02",
        salesOrderLineId: "sol_1008_02",
        itemName: "内衬缓冲包",
        skuCode: "SKU-PAD-N",
        unitCode: "件",
        orderedQuantity: "300",
        remainingQuantity: "120",
        stockReservationId: "rsv_1008_02",
        reservedQuantity: "120",
        availableOnHand: "200",
      },
    ],
    draft: shipDraft("wh_north_2", "华北二号仓", {
      salesOrderLineId: "sol_1008_02",
      stockReservationId: "rsv_1008_02",
      remaining: "120",
    }),
    summary: "部分预占待仓发",
    impact: "发货量不得超过本销售明细有效预占",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
]
