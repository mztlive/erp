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
  serviceLocation: "客户现场 · 地址已打码",
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
      message: "货款已到，可以收货。",
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
    summary: "货到了，等着点收入库",
    impact: "不入库，后面发不了货，客户验收也会拖",
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
      message: "发货看的是给这单留的货和现有库存，跟货款无关。",
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
    summary: "货已经留好了，等发货",
    impact: "客户要求今天送到，晚了会影响验收",
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
      message: "货款已到，可以让供应商发货。",
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
    summary: "供应商直接发给客户，不走自己仓库",
    impact: "确认后销售才能去登记验收；客户签收不等于验收",
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
      purchaseOrderId: "po_04",
      purchaseNo: "CG20260325008",
      purchaseRevisionId: "pr_2012_v1",
      salesOrderId: "so_1004",
      salesOrderNo: "XS20260329006",
      salesRevisionId: "sr_1004_v1",
      supplierLabel: "云启数字权益",
      customerLabel: "星河传媒",
    },
    gate: {
      state: "SATISFIED",
      message: "货款已到，可以交付。",
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
    summary: "等确认交付结果 · 卡号卡密不会存进系统",
    impact: "填了「失败」就不能改，要重做得新开一条",
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
      message: "货款已到，可以确认服务完成。",
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
    summary: "服务做完了，等着确认",
    impact: "确认后销售才能去登记验收",
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
      message: "还差一部分货款没到，暂时不能收货。请先去供应商往来登记付款。",
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
    summary: "货到了，但货款还没到，收不了",
    impact: "货款没补齐前，确认入库会被系统拒绝",
    allowedActions: ["SAVE", "DEFER"],
    actionBlockers: [
      {
        action: "POST",
        code: "PREPAYMENT_BLOCKED",
        message: "货款还没到，不能确认入库",
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
      message: "发货看的是给这单留的货和现有库存。",
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
    summary: "有一部分货已留好，等发货",
    impact: "发货数量不能超过为这单留的货",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_recv_03",
    operationType: "RECEIPT",
    priority: 55,
    dueAt: "2026-08-01T16:30:00+08:00",
    dueLabel: "今天 16:30",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "warning",
    responsibleLabel: "仓储 · 王磊",
    sourceVersion: "v1",
    subjectHash: "sha_ff_recv_03_v1",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_2004",
      purchaseNo: "CG20260330022",
      purchaseRevisionId: "pr_2004_v1",
      salesOrderId: "so_1005",
      salesOrderNo: "XS20260329041",
      salesRevisionId: "sr_1005_v1",
      supplierLabel: "北方仓储服务有限公司",
      customerLabel: "长风物流集团",
      warehouseId: "wh_north_2",
      warehouseLabel: "华北二号仓",
    },
    gate: {
      state: "SATISFIED",
      message: "货款已到，可以收货。",
    },
    lines: [
      {
        lineId: "ffl_r03",
        salesOrderLineId: "sol_1005_01",
        purchaseRevisionLineId: "prl_2004_01",
        itemName: "折叠周转箱",
        skuCode: "SKU-BOX-60",
        unitCode: "个",
        orderedQuantity: "300",
        remainingQuantity: "300",
      },
    ],
    draft: receiptDraft("wh_north_2", "华北二号仓", {
      purchaseRevisionLineId: "prl_2004_01",
      remaining: "300",
    }),
    summary: "货到了，等着点收入库",
    impact: "不入库，后面发不了货",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
  {
    workItemId: "wi_ff_ship_03",
    operationType: "WAREHOUSE_SHIP",
    priority: 50,
    dueAt: "2026-08-02T09:30:00+08:00",
    dueLabel: "明天 09:30",
    overdue: false,
    statusLabel: "待处理",
    statusTone: "warning",
    responsibleLabel: "仓储 · 王磊",
    sourceVersion: "v1",
    subjectHash: "sha_ff_ship_03_v1",
    editVersion: 1,
    source: {
      purchaseOrderId: "po_05",
      purchaseNo: "CG20260324003",
      salesOrderId: "so_1006",
      salesOrderNo: "XS20260329052",
      salesRevisionId: "sr_1006_v1",
      supplierLabel: "北方仓储服务有限公司",
      customerLabel: "长风物流集团",
      warehouseId: "wh_north_2",
      warehouseLabel: "华北二号仓",
    },
    gate: {
      state: "NOT_APPLICABLE",
      message: "发货看的是给这单留的货和现有库存。",
    },
    lines: [
      {
        lineId: "ffl_s03",
        salesOrderLineId: "sol_1006_01",
        itemName: "防潮托盘",
        skuCode: "SKU-PLT-12",
        unitCode: "个",
        orderedQuantity: "150",
        remainingQuantity: "150",
        stockReservationId: "rsv_1006_01",
        reservedQuantity: "150",
        availableOnHand: "180",
      },
    ],
    draft: shipDraft("wh_north_2", "华北二号仓", {
      salesOrderLineId: "sol_1006_01",
      stockReservationId: "rsv_1006_01",
      remaining: "150",
    }),
    summary: "货已经留好了，等发货",
    impact: "客户约的明天上午到",
    allowedActions: ["POST", "SAVE", "DEFER"],
    actionBlockers: [],
  },
]
