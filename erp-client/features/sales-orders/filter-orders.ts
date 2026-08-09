import type { SalesOrderListItem } from "@/features/sales-orders/types"

export type SalesOrderNatureFilter = "all" | "physical_service" | "card_voucher"
export type SalesOrderSummaryFilter =
  | "all"
  | "pending"
  | "inProgress"
  | "pendingCollection"
  | "fulfillmentException"
  | "mallCollab"
export type SalesOrderOriginFilter = "all" | SalesOrderListItem["originSystem"]

/** 主状态筛选：URL 使用稳定枚举码，中文映射集中在本文件，禁止三处重复维护。 */
export const SALES_ORDER_STATUS_OPTIONS = [
  { value: "awaiting_confirm", label: "待二次确认" },
  { value: "awaiting_sales", label: "待销售处理" },
  { value: "awaiting_sales_lead", label: "待销售领导审批" },
  { value: "awaiting_ops", label: "待运营审批" },
  { value: "fulfilling", label: "履约中" },
  { value: "effective", label: "已生效" },
  { value: "closed", label: "已关闭" },
  { value: "draft", label: "草稿" },
  { value: "voided", label: "已作废" },
] as const

export type SalesOrderStatusValue =
  (typeof SALES_ORDER_STATUS_OPTIONS)[number]["value"]

export type SalesOrderStatusFilter = "all" | SalesOrderStatusValue

const STATUS_LABEL_BY_VALUE = new Map<string, string>(
  SALES_ORDER_STATUS_OPTIONS.map((option) => [option.value, option.label])
)

/** 主状态中文名（列表渲染 / 高级筛选文案共用）。 */
export function salesOrderStatusLabel(
  status: SalesOrderStatusFilter
): string {
  if (status === "all") return "全部状态"
  return STATUS_LABEL_BY_VALUE.get(status) ?? status
}

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

function statusMatches(
  code: string,
  filter: SalesOrderStatusFilter
): boolean {
  if (filter === "all") return true
  return code === filter
}

/** “待处理”指标覆盖的阶段码：任一审核轨在途。 */
const PENDING_STAGE_CODES: readonly SalesOrderStatusValue[] = [
  "awaiting_confirm",
  "awaiting_sales",
  "awaiting_sales_lead",
  "awaiting_ops",
]

/** “进行中”指标覆盖的阶段码：已生效（含履约中）。 */
const IN_PROGRESS_STAGE_CODES: readonly SalesOrderStatusValue[] = [
  "fulfilling",
  "effective",
]

export function filterSalesOrders(
  orders: readonly SalesOrderListItem[],
  options: {
    search?: string
    natureFilter?: SalesOrderNatureFilter
    summaryFilter?: SalesOrderSummaryFilter
    originFilter?: SalesOrderOriginFilter
    statusFilter?: SalesOrderStatusFilter
  }
): SalesOrderListItem[] {
  const {
    search = "",
    natureFilter = "all",
    summaryFilter = "all",
    originFilter = "all",
    statusFilter = "all",
  } = options

  return orders.filter((order) => {
    if (natureFilter !== "all" && order.nature !== natureFilter) return false
    if (originFilter !== "all" && order.originSystem !== originFilter) {
      return false
    }
    if (!statusMatches(order.primaryStatus.code, statusFilter)) {
      return false
    }
    if (summaryFilter === "pending") {
      if (!PENDING_STAGE_CODES.includes(order.primaryStatus.code as SalesOrderStatusValue)) {
        return false
      }
    }
    if (summaryFilter === "inProgress") {
      if (!IN_PROGRESS_STAGE_CODES.includes(order.primaryStatus.code as SalesOrderStatusValue)) {
        return false
      }
    }
    if (summaryFilter === "pendingCollection") {
      if (
        order.collection.label === "已结清" ||
        order.primaryStatus.code === "draft" ||
        order.primaryStatus.code === "voided"
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
      if (order.originSystem !== "mall") return false
      if (
        order.collection.label !== "待复核" &&
        order.invoicing.label !== "待复核" &&
        order.primaryStatus.code !== "voided"
      ) {
        return false
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
      PENDING_STAGE_CODES.includes(o.primaryStatus.code as SalesOrderStatusValue)
    ).length,
    inProgress: orders.filter((o) =>
      IN_PROGRESS_STAGE_CODES.includes(o.primaryStatus.code as SalesOrderStatusValue)
    ).length,
    pendingCollection: orders.filter(
      (o) =>
        o.primaryStatus.code !== "draft" &&
        o.primaryStatus.code !== "voided" &&
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
        o.originSystem === "mall" ||
        o.collection.label === "待复核" ||
        o.invoicing.label === "待复核"
    ).length,
  }
}
