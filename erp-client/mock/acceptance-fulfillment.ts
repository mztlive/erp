/**
 * W06 可验收履约事实基线（演示）。
 * 净可验收量由 session-state 在 APPLY/REVERSE 后重算，不在此文件写死最终 eligible。
 */

import type { FulfillmentFactType } from "@/features/sales-orders/acceptance-types"

export type BaselineFulfillmentFact = {
  fulfillmentLineId: string
  fulfillmentFactType: FulfillmentFactType
  fulfillmentNo: string
  salesOrderId: string
  salesOrderLineId: string
  lineNo: number
  itemSnapshot: string
  unitCode: string
  occurredAt: string
  /** 有效履约数量（已扣除履约侧冲正） */
  netSuccessfulQuantity: string
  /** 基线已验收分配（演示预置）；会话内 APPLY/REVERSE 叠加在其上 */
  baselineAcceptedAllocated: string
  carrier?: string
  trackingNo?: string
}

/** so_1002 为导航演示单：至少两条不同履约来源，可同单多次验收 */
export const BASELINE_FULFILLMENT_FACTS: readonly BaselineFulfillmentFact[] = [
  {
    fulfillmentLineId: "ff_so1002_df_01",
    fulfillmentFactType: "SUPPLIER_DIRECT",
    fulfillmentNo: "DF20260401001",
    salesOrderId: "so_1002",
    salesOrderLineId: "li_1",
    lineNo: 1,
    itemSnapshot: "员工慰问水果箱 · SKU-FRUIT-12",
    unitCode: "箱",
    occurredAt: "2026-04-01T10:20:00+08:00",
    netSuccessfulQuantity: "150",
    baselineAcceptedAllocated: "0",
    carrier: "顺丰速运",
    trackingNo: "SF1044281901",
  },
  {
    fulfillmentLineId: "ff_so1002_wh_01",
    fulfillmentFactType: "WAREHOUSE_SHIP",
    fulfillmentNo: "FH20260403012",
    salesOrderId: "so_1002",
    salesOrderLineId: "li_1",
    lineNo: 1,
    itemSnapshot: "员工慰问水果箱 · SKU-FRUIT-12",
    unitCode: "箱",
    occurredAt: "2026-04-03T15:40:00+08:00",
    netSuccessfulQuantity: "100",
    baselineAcceptedAllocated: "40",
    carrier: "德邦物流",
    trackingNo: "DP88291003",
  },
  {
    fulfillmentLineId: "ff_so1002_wh_02",
    fulfillmentFactType: "WAREHOUSE_SHIP",
    fulfillmentNo: "FH20260405008",
    salesOrderId: "so_1002",
    salesOrderLineId: "li_2",
    lineNo: 2,
    itemSnapshot: "康复营养礼包 · SKU-CARE-03",
    unitCode: "套",
    occurredAt: "2026-04-05T09:10:00+08:00",
    netSuccessfulQuantity: "30",
    baselineAcceptedAllocated: "0",
    carrier: "京东物流",
    trackingNo: "JD640019283",
  },
  {
    fulfillmentLineId: "ff_so1002_svc_01",
    fulfillmentFactType: "SERVICE",
    fulfillmentNo: "FW20260406002",
    salesOrderId: "so_1002",
    salesOrderLineId: "li_2",
    lineNo: 2,
    itemSnapshot: "康复营养礼包 · 到场布置服务",
    unitCode: "套",
    occurredAt: "2026-04-06T14:00:00+08:00",
    netSuccessfulQuantity: "20",
    baselineAcceptedAllocated: "0",
  },
  {
    fulfillmentLineId: "ff_so1004_el_01",
    fulfillmentFactType: "ELECTRONIC",
    fulfillmentNo: "ED20260328001",
    salesOrderId: "so_1004",
    salesOrderLineId: "li_1",
    lineNo: 1,
    itemSnapshot: "工作日午餐兑换券包 · SKU-MEAL-30",
    unitCode: "份",
    occurredAt: "2026-03-28T11:00:00+08:00",
    netSuccessfulQuantity: "500",
    baselineAcceptedAllocated: "0",
  },
  {
    fulfillmentLineId: "ff_so1005_wh_01",
    fulfillmentFactType: "WAREHOUSE_SHIP",
    fulfillmentNo: "FH20260220003",
    salesOrderId: "so_1005",
    salesOrderLineId: "li_1",
    lineNo: 1,
    itemSnapshot: "元宵团圆礼盒 · SKU-LAN-01",
    unitCode: "套",
    occurredAt: "2026-02-20T16:00:00+08:00",
    netSuccessfulQuantity: "120",
    baselineAcceptedAllocated: "120",
  },
]

export function listBaselineFactsForOrder(
  salesOrderId: string
): BaselineFulfillmentFact[] {
  return BASELINE_FULFILLMENT_FACTS.filter(
    (fact) => fact.salesOrderId === salesOrderId
  )
}
