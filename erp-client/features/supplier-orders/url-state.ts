import type {
  CancelStatus,
  DemoRole,
  ListView,
  OrderSection,
  RefundStatus,
  SupplierFulfillmentStatus,
} from "@/features/supplier-orders/types"
import {
  CANCEL_STATUSES,
  FULFILLMENT_STATUSES,
  REFUND_STATUSES,
  SECTIONS,
} from "@/features/supplier-orders/types"
import { createUrlStateCodec } from "@/lib/url-state"

export type SupplierOrdersUrlState = {
  view: ListView
  q?: string
  supplierId?: string
  fulfillmentStatuses?: SupplierFulfillmentStatus[]
  cancelStatus?: CancelStatus
  refundStatus?: RefundStatus
  paidFrom?: string
  paidTo?: string
  page: number
  pageSize: number
  preview?: string
  role: DemoRole
  section: OrderSection
  workItemId?: string
  from?: string
  sourceId?: string
  sort?: string
  dir?: "asc" | "desc"
}

const VIEWS = ["actionable", "all", "recent_completed"] as const
const ROLES = ["procurement", "cs", "ops", "finance", "admin"] as const

const codec = createUrlStateCodec<SupplierOrdersUrlState>([
  { key: "view", type: "enum", values: VIEWS, defaultValue: "actionable" },
  { key: "q", type: "string", trim: true },
  { key: "supplierId", type: "string" },
  {
    key: "fulfillmentStatus",
    name: "fulfillmentStatuses",
    type: "array",
    values: FULFILLMENT_STATUSES,
  },
  { key: "cancelStatus", type: "enum", values: CANCEL_STATUSES },
  { key: "refundStatus", type: "enum", values: REFUND_STATUSES },
  { key: "paidFrom", type: "string" },
  { key: "paidTo", type: "string" },
  { key: "page", type: "number", defaultValue: 1 },
  { key: "pageSize", type: "number", defaultValue: 50, min: 1, max: 100 },
  { key: "preview", type: "string", aliases: ["supplierOrderId"] },
  {
    key: "role",
    type: "enum",
    values: ROLES,
    defaultValue: "procurement",
    aliases: ["demoRole"],
  },
  { key: "section", type: "enum", values: SECTIONS, defaultValue: "overview" },
  { key: "workItemId", type: "string" },
  { key: "from", type: "string" },
  { key: "sourceId", type: "string", aliases: ["mallOrderId"] },
  { key: "sort", type: "string" },
  { key: "dir", type: "enum", values: ["asc", "desc"] },
])

export const parseSupplierOrdersSearchParams = codec.parse
export const buildSupplierOrdersSearchParams = codec.build
