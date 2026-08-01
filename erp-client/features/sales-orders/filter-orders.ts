import type { SalesOrderListItem } from "@/features/sales-orders/types"

export type SalesOrderNatureFilter = "all" | "physical_service" | "card_voucher"
export type SalesOrderSummaryFilter =
  | "all"
  | "pending"
  | "inProgress"
  | "pendingCollection"
  | "fulfillmentException"
  | "mallCollab"
export type SalesOrderOwnerFilter = "all" | SalesOrderListItem["ownerSystem"]
export type SalesOrderOriginFilter = "all" | SalesOrderListItem["originSystem"]
export type SalesOrderStatusFilter =
  | "all"
  | "待二次确认"
  | "待销售处理"
  | "待销售领导审批"
  | "待运营审批"
  | "履约中"
  | "已生效"
  | "已关闭"
  | "草稿"
  | "已作废"

export function matchesSalesOrderSearch(
  order: SalesOrderListItem,
  query: string
): boolean {
  if (!query) return true
  const q = query.trim().toLowerCase()
  return (
    order.documentNumber.toLowerCase().includes(q) ||
    order.customerName.toLowerCase().includes(q) ||
    order.contractNumber.toLowerCase().includes(q) ||
    order.ownerName.toLowerCase().includes(q)
  )
}

export function filterSalesOrders(
  orders: readonly SalesOrderListItem[],
  options: {
    search?: string
    natureFilter?: SalesOrderNatureFilter
    summaryFilter?: SalesOrderSummaryFilter
    ownerFilter?: SalesOrderOwnerFilter
    originFilter?: SalesOrderOriginFilter
    statusFilter?: SalesOrderStatusFilter
  }
): SalesOrderListItem[] {
  const {
    search = "",
    natureFilter = "all",
    summaryFilter = "all",
    ownerFilter = "all",
    originFilter = "all",
    statusFilter = "all",
  } = options

  return orders.filter((order) => {
    if (natureFilter !== "all" && order.nature !== natureFilter) return false
    if (ownerFilter !== "all" && order.ownerSystem !== ownerFilter) return false
    if (originFilter !== "all" && order.originSystem !== originFilter) {
      return false
    }
    if (statusFilter !== "all" && order.primaryStatus.label !== statusFilter) {
      return false
    }
    if (summaryFilter === "pending") {
      if (
        ![
          "待二次确认",
          "待销售处理",
          "待销售领导审批",
          "待运营审批",
          "草稿",
        ].includes(order.primaryStatus.label)
      ) {
        return false
      }
    }
    if (summaryFilter === "inProgress") {
      if (
        !["履约中", "已生效"].includes(order.primaryStatus.label)
      ) {
        return false
      }
    }
    if (summaryFilter === "pendingCollection") {
      if (
        order.collection.label === "已结清" ||
        order.primaryStatus.label === "草稿" ||
        order.primaryStatus.label === "已作废"
      ) {
        return false
      }
      if (
        order.collection.label !== "未收" &&
        order.collection.label !== "部分回款" &&
        order.collection.label !== "待复核"
      ) {
        return false
      }
    }
    if (summaryFilter === "fulfillmentException") {
      if (
        order.fulfillment.label !== "部分履约" &&
        order.fulfillment.tone !== "warning"
      ) {
        return false
      }
    }
    if (summaryFilter === "mallCollab") {
      if (order.ownerSystem !== "mall" && order.originSystem !== "mall") {
        return false
      }
      if (
        order.collection.label !== "待复核" &&
        order.invoicing.label !== "待复核" &&
        order.primaryStatus.label !== "已作废"
      ) {
        // mall-owned in progress or mapping-sensitive
        if (order.ownerSystem !== "mall") return false
      }
    }
    return matchesSalesOrderSearch(order, search)
  })
}

export function salesOrderSummaryLabels(
  summaryFilter: SalesOrderSummaryFilter
): string {
  switch (summaryFilter) {
    case "pending":
      return "待处理"
    case "inProgress":
      return "进行中"
    case "pendingCollection":
      return "待收款"
    case "fulfillmentException":
      return "履约异常"
    case "mallCollab":
      return "商城协同"
    default:
      return "全部指标"
  }
}

export function computeSalesOrderMetrics(orders: readonly SalesOrderListItem[]) {
  return {
    total: orders.length,
    pending: orders.filter((o) =>
      [
        "待二次确认",
        "待销售处理",
        "待销售领导审批",
        "待运营审批",
        "草稿",
      ].includes(o.primaryStatus.label)
    ).length,
    inProgress: orders.filter((o) =>
      ["履约中", "已生效"].includes(o.primaryStatus.label)
    ).length,
    pendingCollection: orders.filter(
      (o) =>
        o.primaryStatus.label !== "草稿" &&
        o.primaryStatus.label !== "已作废" &&
        (o.collection.label === "未收" ||
          o.collection.label === "部分回款" ||
          o.collection.label === "待复核")
    ).length,
    fulfillmentException: orders.filter(
      (o) =>
        o.fulfillment.label === "部分履约" || o.fulfillment.tone === "warning"
    ).length,
    mallCollab: orders.filter(
      (o) =>
        o.ownerSystem === "mall" ||
        o.collection.label === "待复核" ||
        o.invoicing.label === "待复核"
    ).length,
  }
}
