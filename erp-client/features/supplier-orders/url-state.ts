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
}

const VIEWS = new Set<ListView>(["actionable", "all", "recent_completed"])
const ROLES = new Set<DemoRole>([
  "procurement",
  "cs",
  "ops",
  "finance",
  "admin",
])
const FULFILLMENT_SET = new Set<string>(FULFILLMENT_STATUSES)
const CANCEL_SET = new Set<string>(CANCEL_STATUSES)
const REFUND_SET = new Set<string>(REFUND_STATUSES)
const SECTION_SET = new Set<string>(SECTIONS)

export function parseSupplierOrdersSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): SupplierOrdersUrlState {
  const viewRaw = searchParams.get("view")
  const view: ListView =
    viewRaw && VIEWS.has(viewRaw as ListView)
      ? (viewRaw as ListView)
      : "actionable"

  const q = searchParams.get("q") ?? undefined
  const supplierId = searchParams.get("supplierId") ?? undefined

  const ffRaw = searchParams.get("fulfillmentStatus")
  const fulfillmentStatuses = ffRaw
    ? ffRaw
        .split(",")
        .map((s) => s.trim())
        .filter(
          (s): s is SupplierFulfillmentStatus => FULFILLMENT_SET.has(s)
        )
    : undefined
  const fulfillmentStatusList =
    fulfillmentStatuses && fulfillmentStatuses.length > 0
      ? fulfillmentStatuses
      : undefined

  const cancelRaw = searchParams.get("cancelStatus")
  const cancelStatus =
    cancelRaw && CANCEL_SET.has(cancelRaw)
      ? (cancelRaw as CancelStatus)
      : undefined

  const refundRaw = searchParams.get("refundStatus")
  const refundStatus =
    refundRaw && REFUND_SET.has(refundRaw)
      ? (refundRaw as RefundStatus)
      : undefined

  const paidFrom = searchParams.get("paidFrom") ?? undefined
  const paidTo = searchParams.get("paidTo") ?? undefined

  const pageRaw = Number(searchParams.get("page") ?? "1")
  const page =
    Number.isFinite(pageRaw) && pageRaw >= 1 ? Math.floor(pageRaw) : 1
  const pageSizeRaw = Number(searchParams.get("pageSize") ?? "50")
  const pageSize =
    Number.isFinite(pageSizeRaw) && pageSizeRaw >= 1
      ? Math.min(100, Math.floor(pageSizeRaw))
      : 50

  const preview =
    searchParams.get("preview") ??
    searchParams.get("supplierOrderId") ??
    undefined

  const roleRaw = searchParams.get("role") ?? searchParams.get("demoRole")
  const role: DemoRole =
    roleRaw && ROLES.has(roleRaw as DemoRole)
      ? (roleRaw as DemoRole)
      : "procurement"

  const sectionRaw = searchParams.get("section")
  const section: OrderSection =
    sectionRaw && SECTION_SET.has(sectionRaw)
      ? (sectionRaw as OrderSection)
      : "overview"

  const workItemId = searchParams.get("workItemId") ?? undefined
  const from = searchParams.get("from") ?? undefined
  const sourceId =
    searchParams.get("sourceId") ??
    searchParams.get("mallOrderId") ??
    undefined

  return {
    view,
    q,
    supplierId,
    fulfillmentStatuses: fulfillmentStatusList,
    cancelStatus,
    refundStatus,
    paidFrom,
    paidTo,
    page,
    pageSize,
    preview,
    role,
    section,
    workItemId,
    from,
    sourceId,
  }
}

export function buildSupplierOrdersSearchParams(
  state: SupplierOrdersUrlState
): string {
  const params = new URLSearchParams()
  if (state.view !== "actionable") params.set("view", state.view)
  if (state.q?.trim()) params.set("q", state.q.trim())
  if (state.supplierId) params.set("supplierId", state.supplierId)
  if (state.fulfillmentStatuses?.length) {
    params.set("fulfillmentStatus", state.fulfillmentStatuses.join(","))
  }
  if (state.cancelStatus) params.set("cancelStatus", state.cancelStatus)
  if (state.refundStatus) params.set("refundStatus", state.refundStatus)
  if (state.paidFrom) params.set("paidFrom", state.paidFrom)
  if (state.paidTo) params.set("paidTo", state.paidTo)
  if (state.page > 1) params.set("page", String(state.page))
  if (state.pageSize !== 50) params.set("pageSize", String(state.pageSize))
  if (state.preview) params.set("preview", state.preview)
  if (state.role !== "procurement") params.set("role", state.role)
  if (state.section !== "overview") params.set("section", state.section)
  if (state.workItemId) params.set("workItemId", state.workItemId)
  if (state.from) params.set("from", state.from)
  if (state.sourceId) params.set("sourceId", state.sourceId)
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}
