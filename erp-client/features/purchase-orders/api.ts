/**
 * W08 采购单 session-mock API：queryFn / mutationFn 纯函数。
 * draftEditToken / claimToken 仅存会话内存，不进入列表查询 View。
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  CreatePurchaseOrderFromBasisInput,
  FormalActionResponse,
  PurchaseCreationBasis,
  PurchaseOrderCenterView,
  PurchaseOrderListItem,
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

export type PurchaseOrderListResult = {
  rows: PurchaseOrderListItem[]
  metrics: Array<{ key: string; label: string; count: number; detail: string }>
  freshness: { updatedAt: string; state: "fresh" }
}

export async function fetchPurchaseOrders(
  role: ViewerRole = "procurement"
): Promise<PurchaseOrderListResult> {
  await mockDelay()
  const rows = listW08PurchaseOrders(role)
  const metrics = [
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
      detail: "W07 固定结果",
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
  return {
    rows,
    metrics,
    freshness: { updatedAt: new Date().toISOString(), state: "fresh" },
  }
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
      simulateUnknown: input.simulateUnknown,
    })
    return {
      status: "succeeded",
      data: {
        lockVersion: data.lockVersion,
        draftContentHash: data.draftContentHash,
        totals: data.totals,
      },
      reference: `SAVE-${input.purchaseOrderId}-${data.lockVersion}`,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      if (error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
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
      reference: data.submissionId,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      if (error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
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
      simulateUnknown: input.simulateUnknown,
    })
    return {
      status: "succeeded",
      data,
      reference: data.reference,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      if (error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
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
      reference: data.changeId,
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
      reference: data.purchaseOrderId,
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
