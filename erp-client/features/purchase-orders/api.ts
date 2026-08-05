/**
 * W08 采购单 session-mock API：queryFn / mutationFn 纯函数。
 * draftEditToken 仅存会话内存，不进入列表查询 View；处理权仅存会话内存。
 */

import { mockDelay } from "@/lib/mock-delay"
import { compareDecimal } from "@/lib/fixed-decimal"
import type {
  CreatePurchaseOrderFromBasisInput,
  FormalActionResponse,
  PurchaseCreationBasis,
  PurchaseOrderCenterView,
  PurchaseOrderListItem,
  PurchaseOrderMetricFilter,
  PurchaseOrderStatusFilter,
  ReviewPurchaseOrderInput,
  SavePurchaseOrderDraftInput,
  SubmitPurchaseOrderInput,
  ViewerRole,
} from "@/features/purchase-orders/types"
import {
  acquireW08DraftEditToken,
  createW08FromBasis,
  getW08PurchaseOrderCenter,
  listW08CreationBases,
  listW08PurchaseOrders,
  queryW08IdempotentResult,
  reviewW08PurchaseOrder,
  saveW08PurchaseOrderDraft,
  startW08PurchaseChange,
  submitW08PurchaseOrder,
  WorkItemMockError,
} from "@/mock/session-state"

export type PurchaseOrderListQuery = {
  role?: ViewerRole
  q?: string
  status?: PurchaseOrderStatusFilter
  metric?: PurchaseOrderMetricFilter
  page?: number
  pageSize?: number
  sortBy?: string
  sortDir?: "asc" | "desc"
}

export type PurchaseOrderListResult = {
  rows: PurchaseOrderListItem[]
  total: number
  page: number
  pageSize: number
  metrics: Array<{ key: string; label: string; count: number; detail: string }>
  freshness: { updatedAt: string; state: "fresh" }
}

const PURCHASE_ORDER_DEFAULT_PAGE_SIZE = 20
const PURCHASE_ORDER_MAX_PAGE_SIZE = 100

function listDisplayNo(row: PurchaseOrderListItem): string {
  return row.purchaseNo ?? row.draftLabel ?? "采购单（未编号）"
}

function matchesListFilter(
  row: PurchaseOrderListItem,
  query: PurchaseOrderListQuery
): boolean {
  const status = query.status ?? "all"
  if (status !== "all" && row.status !== status) return false
  const metric = query.metric ?? "all"
  if (metric === "draft" && row.status !== "DRAFT") return false
  if (metric === "review" && row.status !== "PENDING_REVIEW") return false
  if (
    metric === "fulfill" &&
    !(
      (row.status === "EFFECTIVE" || row.status === "PARTIAL") &&
      row.fulfillmentProgress !== "完成"
    )
  ) {
    return false
  }
  if (metric === "gate_blocked" && row.paymentGate !== "BLOCKED") return false
  const q = query.q?.trim().toLowerCase()
  if (!q) return true
  const hay = [
    row.purchaseNo,
    row.draftLabel,
    row.supplierName,
    row.salesOrderNo,
    row.ownerName,
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase()
  return hay.includes(q)
}

function listSortComparator(
  sortBy: string | undefined,
  sortDir: "asc" | "desc" | undefined
):
  | ((a: PurchaseOrderListItem, b: PurchaseOrderListItem) => number)
  | null {
  if (!sortBy || !sortDir) return null
  switch (sortBy) {
    case "document":
      return (a, b) => listDisplayNo(a).localeCompare(listDisplayNo(b))
    case "source":
      return (a, b) => a.salesOrderNo.localeCompare(b.salesOrderNo)
    case "amount":
      return (a, b) =>
        a.costMasked || b.costMasked
          ? 0
          : compareDecimal(a.grossAmount, b.grossAmount, 2)
    case "owner":
      return (a, b) => a.ownerName.localeCompare(b.ownerName)
    default:
      return null
  }
}

function sortPurchaseOrders(
  rows: PurchaseOrderListItem[],
  query: PurchaseOrderListQuery
): PurchaseOrderListItem[] {
  const primary = listSortComparator(query.sortBy, query.sortDir)
  const byUpdatedAt = (a: PurchaseOrderListItem, b: PurchaseOrderListItem) =>
    b.updatedAt.localeCompare(a.updatedAt)
  const comparator: (
    a: PurchaseOrderListItem,
    b: PurchaseOrderListItem
  ) => number = primary
    ? query.sortDir === "desc"
      ? (a, b) => -primary(a, b)
      : primary
    : byUpdatedAt
  return [...rows].sort((a, b) => {
    const primaryResult = comparator(a, b)
    if (primaryResult !== 0) return primaryResult
    const updatedAtResult = byUpdatedAt(a, b)
    if (updatedAtResult !== 0) return updatedAtResult
    return a.purchaseOrderId.localeCompare(b.purchaseOrderId)
  })
}

function buildPurchaseOrderMetrics(rows: PurchaseOrderListItem[]) {
  return [
    {
      key: "all",
      label: "全部采购单",
      count: rows.length,
      detail: "当前数据范围",
    },
    {
      key: "pending_create",
      label: "可建单依据",
      count: listW08CreationBases().filter((b) => !b.consumed).length,
      detail: "采购二次确认固定结果",
    },
    {
      key: "draft",
      label: "草稿",
      count: rows.filter((r) => r.status === "DRAFT").length,
      detail: "可继续编辑",
    },
    {
      key: "review",
      label: "待财务审核",
      count: rows.filter((r) => r.status === "PENDING_REVIEW").length,
      detail: "财务闸门",
    },
    {
      key: "fulfill",
      label: "待履约",
      count: rows.filter(
        (r) =>
          (r.status === "EFFECTIVE" || r.status === "PARTIAL") &&
          r.fulfillmentProgress !== "完成"
      ).length,
      detail: "含门禁阻塞",
    },
    {
      key: "gate_blocked",
      label: "先款门禁阻塞",
      count: rows.filter((r) => r.paymentGate === "BLOCKED").length,
      detail: "需有效付款",
    },
  ]
}

export async function fetchPurchaseOrders(
  query: PurchaseOrderListQuery = {}
): Promise<PurchaseOrderListResult> {
  await mockDelay()
  const all = listW08PurchaseOrders(query.role ?? "procurement")
  const filtered = sortPurchaseOrders(
    all.filter((row) => matchesListFilter(row, query)),
    query
  )
  const total = filtered.length
  const pageSize = Math.min(
    Math.max(1, query.pageSize ?? PURCHASE_ORDER_DEFAULT_PAGE_SIZE),
    PURCHASE_ORDER_MAX_PAGE_SIZE
  )
  const maxPage = Math.max(1, Math.ceil(total / pageSize))
  const page = Math.min(Math.max(1, Math.floor(query.page ?? 1)), maxPage)
  const start = (page - 1) * pageSize
  return {
    rows: filtered.slice(start, start + pageSize),
    total,
    page,
    pageSize,
    metrics: buildPurchaseOrderMetrics(all),
    freshness: { updatedAt: new Date().toISOString(), state: "fresh" },
  }
}

export async function fetchPurchaseOrderExportData(
  query: PurchaseOrderListQuery = {}
): Promise<PurchaseOrderListItem[]> {
  await mockDelay(60)
  const all = listW08PurchaseOrders(query.role ?? "procurement")
  return sortPurchaseOrders(
    all.filter((row) => matchesListFilter(row, query)),
    query
  )
}

export async function fetchPurchaseOrderCenter(
  purchaseOrderId: string,
  role: ViewerRole = "procurement"
): Promise<PurchaseOrderCenterView | null> {
  await mockDelay(80)
  return getW08PurchaseOrderCenter(purchaseOrderId, role)
}

export async function fetchCreationBases(): Promise<
  readonly PurchaseCreationBasis[]
> {
  await mockDelay(60)
  return listW08CreationBases()
}

export async function acquireDraftEditToken(purchaseOrderId: string): Promise<{
  draftEditToken: string
  lockVersion: number
}> {
  await mockDelay(40)
  try {
    return acquireW08DraftEditToken(purchaseOrderId)
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      throw new Error(error.message)
    }
    throw error
  }
}

export async function savePurchaseOrderDraft(
  input: SavePurchaseOrderDraftInput & { paymentTermLabel: string }
): Promise<
  FormalActionResponse<{
    lockVersion: number
    draftContentHash: string
    totals: { gross: string; net: string; tax: string }
  }>
> {
  await mockDelay(100)
  try {
    const data = saveW08PurchaseOrderDraft({
      purchaseOrderId: input.purchaseOrderId,
      expectedLockVersion: input.expectedLockVersion,
      draftEditToken: input.draftEditToken,
      paymentTermCode: input.paymentTermCode,
      paymentTermLabel: input.paymentTermLabel,
      linePatches: input.lines,
      idempotencyKey: input.idempotencyKey,
      simulateConflict: input.simulateConflict,
    })
    return {
      status: "succeeded",
      data: {
        lockVersion: data.lockVersion,
        draftContentHash: data.draftContentHash,
        totals: data.totals,
      },
      reference: `SAVED-V${data.lockVersion}`,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return {
        status: "failed",
        message: error.message,
        code: error.code,
      }
    }
    throw error
  }
}

export async function submitPurchaseOrderForReview(
  input: SubmitPurchaseOrderInput
): Promise<
  FormalActionResponse<{
    submissionId: string
    submissionNo: string
    subjectHash: string
    workItemId: string
    purchaseNo: string
    lockVersion: number
  }>
> {
  await mockDelay(120)
  try {
    const data = submitW08PurchaseOrder(input)
    return {
      status: "succeeded",
      data,
      reference: `SUB-${data.submissionNo}`,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return {
        status: "failed",
        message: error.message,
        code: error.code,
      }
    }
    throw error
  }
}

export async function reviewPurchaseOrder(
  input: ReviewPurchaseOrderInput
): Promise<
  FormalActionResponse<{
    reviewResult: "APPROVED" | "REJECTED"
    revisionId?: string
    revisionNo?: number
    payableOpenAmount?: string
    lockVersion: number
    reference: string
  }>
> {
  await mockDelay(140)
  try {
    const data = reviewW08PurchaseOrder({
      purchaseOrderId: input.purchaseOrderId,
      submissionId: input.submissionId,
      workItemId: input.workItemId,
      expectedLockVersion: input.expectedLockVersion,
      reviewResult: input.reviewResult,
      reasonCode: input.reasonCode,
      comment: input.comment,
      idempotencyKey: input.idempotencyKey,
    })
    return {
      status: "succeeded",
      data,
      reference: `REVIEW-V${data.lockVersion}`,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return {
        status: "failed",
        message: error.message,
        code: error.code,
      }
    }
    throw error
  }
}

export async function startPurchaseChange(input: {
  purchaseOrderId: string
  expectedLockVersion: number
  idempotencyKey: string
}): Promise<
  FormalActionResponse<{ changeId: string; baseRevisionNo: number }>
> {
  await mockDelay(100)
  try {
    const data = startW08PurchaseChange(input)
    return {
      status: "succeeded",
      data,
      reference: `CHANGE-V${data.baseRevisionNo}`,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return {
        status: "failed",
        message: error.message,
        code: error.code,
      }
    }
    throw error
  }
}

export async function createPurchaseOrderFromBasis(
  input: CreatePurchaseOrderFromBasisInput
): Promise<
  FormalActionResponse<{
    purchaseOrderId: string
    draftLabel: string
    lockVersion: number
  }>
> {
  await mockDelay(120)
  try {
    const data = createW08FromBasis(input)
    return {
      status: "succeeded",
      data,
      reference: data.draftLabel,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return {
        status: "failed",
        message: error.message,
        code: error.code,
      }
    }
    throw error
  }
}

export async function queryPurchaseOrderActionResult(
  idempotencyKey: string
): Promise<unknown | null> {
  await mockDelay(50)
  return queryW08IdempotentResult(idempotencyKey)
}
