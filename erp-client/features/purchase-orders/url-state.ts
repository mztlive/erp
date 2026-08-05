import { createUrlStateCodec } from "@/lib/url-state"
import type {
  PurchaseOrderMetricFilter,
  PurchaseOrderStatusFilter,
} from "@/features/purchase-orders/types"

export type PurchaseOrdersUrlState = {
  q?: string
  status: PurchaseOrderStatusFilter
  metric: PurchaseOrderMetricFilter
  page: number
  pageSize: number
  sort?: string
  basisId?: string
}

const STATUS_VALUES: readonly PurchaseOrderStatusFilter[] = [
  "all",
  "DRAFT",
  "PENDING_REVIEW",
  "EFFECTIVE",
  "PARTIAL",
  "COMPLETED",
]

const METRIC_VALUES: readonly PurchaseOrderMetricFilter[] = [
  "all",
  "pending_create",
  "draft",
  "review",
  "fulfill",
  "gate_blocked",
]

const codec = createUrlStateCodec<PurchaseOrdersUrlState>([
  { key: "q", type: "string", trim: true },
  { key: "status", type: "enum", values: STATUS_VALUES, defaultValue: "all" },
  { key: "metric", type: "enum", values: METRIC_VALUES, defaultValue: "all" },
  { key: "page", type: "number", defaultValue: 1, min: 1 },
  { key: "pageSize", type: "number", defaultValue: 20, min: 1, max: 100 },
  { key: "sort", type: "string" },
  { key: "basisId", type: "string" },
])

export const parsePurchaseOrdersSearchParams = codec.parse
export const buildPurchaseOrdersSearchParams = codec.build
