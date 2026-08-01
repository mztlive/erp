/**
 * W12 session-mock API：queryFn / mutationFn 纯函数。
 * 正式余额、净分配与门禁结论均来自 mock 投影，前端不重算覆盖。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  AllocationSessionView,
  AllocationTrack,
  FormalSubmitResult,
  PayableDetailView,
  PostInvoiceInput,
  PostPaymentInput,
  ReverseInvoiceInput,
  ReversePaymentInput,
  SaveAllocationDraftInput,
  SupplierAccountsListView,
  SupplierAccountsQuery,
} from "@/features/supplier-payables/types"
import {
  getW12AllocationDraft,
  getW12PayableDetail,
  openW12AllocationSession,
  postW12Invoice,
  postW12Payment,
  queryW12SupplierAccounts,
  resolveW12Unknown,
  reverseW12Invoice,
  reverseW12Payment,
  saveW12AllocationDraft,
  setW12PolicyState,
  revokeW12Permission,
  restoreW12Permission,
} from "@/mock/session-state"

export async function fetchSupplierAccounts(
  query: SupplierAccountsQuery
): Promise<SupplierAccountsListView> {
  await mockDelay()
  return queryW12SupplierAccounts(query)
}

export async function fetchPayableDetail(
  payableAccountId: string
): Promise<PayableDetailView | null> {
  await mockDelay()
  return getW12PayableDetail(payableAccountId)
}

export async function fetchAllocationSession(input: {
  track: AllocationTrack
  supplierId: string
  draftSessionId?: string
  purchaseOrderId?: string
  returnTo?: string
  fromWorkspace?: string
  existingPaymentId?: string
  existingInvoiceId?: string
  preselectPayableAccountId?: string
}): Promise<AllocationSessionView> {
  await mockDelay()
  return openW12AllocationSession(input)
}

export async function saveAllocationDraft(
  input: SaveAllocationDraftInput
): Promise<{ savedAt: string }> {
  await mockDelay(40)
  return saveW12AllocationDraft(input)
}

export async function getAllocationDraft(draftSessionId: string) {
  await mockDelay(20)
  return getW12AllocationDraft(draftSessionId)
}

export async function submitPayment(
  input: PostPaymentInput
): Promise<FormalSubmitResult> {
  await mockDelay(120)
  return postW12Payment(input)
}

export async function submitInvoice(
  input: PostInvoiceInput
): Promise<FormalSubmitResult> {
  await mockDelay(120)
  return postW12Invoice(input)
}

export async function reversePayment(
  input: ReversePaymentInput
): Promise<FormalSubmitResult> {
  await mockDelay(100)
  return reverseW12Payment(input)
}

export async function reverseInvoice(
  input: ReverseInvoiceInput
): Promise<FormalSubmitResult> {
  await mockDelay(100)
  return reverseW12Invoice(input)
}

export async function resolveUnknownResult(
  idempotencyKey: string
): Promise<FormalSubmitResult | null> {
  await mockDelay(80)
  return resolveW12Unknown(idempotencyKey)
}

/** Demo helpers for acceptance of policy/permission states */
export async function demoSetPolicyState(
  state: "AVAILABLE" | "MISSING" | "STALE"
): Promise<void> {
  await mockDelay(20)
  setW12PolicyState(state)
}

export async function demoRevokePermission(): Promise<void> {
  await mockDelay(20)
  revokeW12Permission()
}

export async function demoRestorePermission(): Promise<void> {
  await mockDelay(20)
  restoreW12Permission()
}
