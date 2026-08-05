import type {
  SalesOrderNatureFilter,
  SalesOrderOriginFilter,
  SalesOrderStatusFilter,
  SalesOrderSummaryFilter,
} from "@/features/sales-orders/filter-orders"
import { createUrlStateCodec } from "@/lib/url-state"

export type SalesOrdersUrlState = {
  search?: string
  nature: SalesOrderNatureFilter
  summary: SalesOrderSummaryFilter
  origin: SalesOrderOriginFilter
  status: SalesOrderStatusFilter
  page: number
  pageSize: number
  sort?: string
  dir?: "asc" | "desc"
}

const NATURES: SalesOrderNatureFilter[] = [
  "all",
  "physical_service",
  "card_voucher",
]

const SUMMARIES: SalesOrderSummaryFilter[] = [
  "all",
  "pending",
  "inProgress",
  "pendingCollection",
  "fulfillmentException",
  "mallCollab",
]

const ORIGINS: SalesOrderOriginFilter[] = ["all", "erp", "mall"]

const STATUSES: SalesOrderStatusFilter[] = [
  "all",
  "待二次确认",
  "待销售处理",
  "待销售领导审批",
  "待运营审批",
  "履约中",
  "已生效",
  "已关闭",
  "草稿",
  "已作废",
]

const DIRECTIONS = ["asc", "desc"] as const

const codec = createUrlStateCodec<SalesOrdersUrlState>([
  { key: "search", type: "string", trim: true },
  {
    key: "nature",
    type: "enum",
    values: NATURES,
    defaultValue: "all",
    aliases: ["businessType"],
    normalize: (raw) =>
      raw === "voucher"
        ? "card_voucher"
        : raw === "goods_service"
          ? "physical_service"
          : raw,
  },
  { key: "summary", type: "enum", values: SUMMARIES, defaultValue: "all" },
  { key: "origin", type: "enum", values: ORIGINS, defaultValue: "all" },
  { key: "status", type: "enum", values: STATUSES, defaultValue: "all" },
  { key: "page", type: "number", defaultValue: 1 },
  { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
  { key: "sort", type: "string" },
  { key: "dir", type: "enum", values: DIRECTIONS },
])

export const parseSalesOrdersSearchParams = codec.parse
export const buildSalesOrdersSearchParams = codec.build
