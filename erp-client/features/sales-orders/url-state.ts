import type {
  SalesOrderNatureFilter,
  SalesOrderOriginFilter,
  SalesOrderStatusFilter,
  SalesOrderStatusValue,
  SalesOrderSummaryFilter,
} from "@/features/sales-orders/filter-orders"
import { SALES_ORDER_STATUS_OPTIONS } from "@/features/sales-orders/filter-orders"
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
  "mine",
  "createdByMe",
  "exception",
]

const ORIGINS: SalesOrderOriginFilter[] = ["all", "erp", "mall"]

const STATUSES: SalesOrderStatusFilter[] = [
  "all",
  ...SALES_ORDER_STATUS_OPTIONS.map((option) => option.value),
]

/** 旧链接兼容：中文业务词映射回稳定枚举码。 */
const STATUS_ALIASES: Record<string, SalesOrderStatusValue> = {
  待二次确认: "awaiting_confirm",
  待销售处理: "awaiting_sales",
  待销售领导审批: "awaiting_sales_lead",
  待运营审批: "awaiting_ops",
  履约中: "fulfilling",
  已生效: "effective",
  已关闭: "closed",
  草稿: "draft",
  已作废: "voided",
}

const DIRECTIONS = ["asc", "desc"] as const

const codec = createUrlStateCodec<SalesOrdersUrlState>([
  { key: "q", name: "search", type: "string", trim: true, aliases: ["search"] },
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
  { key: "status", type: "enum", values: STATUSES, defaultValue: "all", aliases: ["statusCode"], normalize: (raw) => STATUS_ALIASES[raw] ?? raw },
  { key: "page", type: "number", defaultValue: 1 },
  { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
  { key: "sort", type: "string" },
  { key: "dir", type: "enum", values: DIRECTIONS },
])

export const parseSalesOrdersSearchParams = codec.parse
export const buildSalesOrdersSearchParams = codec.build
