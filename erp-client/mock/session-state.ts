/**
 * Client-session mutable state for demo formal actions.
 * QueryFns read this so mutations change subsequent reads (and survive within SPA session).
 *
 * Semantics (aligned with W02 queue language, still not a server transaction):
 * - completed: task leaves the active queue (终局成功)
 * - rejected: task leaves the active queue after 退回 (需补充后再入列由服务端负责)
 * - held: task remains in the active queue, status becomes 已暂挂
 *
 * W02 work-item mock contract (explicit, session-only):
 * - claimToken only lives in this module's lease map; list/detail query views never return it
 * - WorkItemActionEnvelope → PENDING/IN_PROGRESS, no auto-complete
 * - CompleteWorkItemEnvelope → COMPLETED with business result in one mock transaction
 * - CloseWorkItemEnvelope → CLOSED without mutating business facts
 * - TransferWorkItemEnvelope → TRANSFERRED + successor id
 * - Idempotency map supports timeout → query-by-same-key
 */

import type {
  AcceptanceDraft,
  AcceptanceDraftLine,
  AcceptanceHistoryItem,
  AcceptanceOverallResult,
  PostAcceptanceInput,
  PostAcceptanceResult,
  ReverseAcceptanceInput,
  ReverseAcceptanceResult,
  SaveAcceptanceDraftInput,
} from "@/features/sales-orders/acceptance-types"
import { FACT_ONLY_NOTICE } from "@/features/sales-orders/acceptance-types"
import { listBaselineFactsForOrder } from "@/mock/acceptance-fulfillment"

import type {
  PurchaseCreationBasis,
  PurchaseOrderCenterView,
  PurchaseOrderLineView,
  PurchaseOrderListItem,
  PurchaseOrderStatus,
  PurchaseReviewStatus,
  ViewerRole,
} from "@/features/purchase-orders/types"
import {
  PO_STATUS_LABEL,
  PO_STATUS_TONE,
} from "@/features/purchase-orders/types"
import {
  MOCK_CREATION_BASES,
  MOCK_PURCHASE_ORDER_SEEDS,
  buildLines,
  sumLines,
  toCenter,
  type SeedPO,
} from "@/mock/purchase-orders"
import type {
  ContractExportJob,
  ContractListRow,
  ContractCenterView,
  UploadContractPdfInput,
  UploadContractPdfResult,
} from "@/features/contracts/types"
import {
  CONTRACT_STATUS_LABEL,
  CONTRACT_STATUS_TONE,
} from "@/features/contracts/types"
import {
  MOCK_CONTRACT_CENTERS,
  MOCK_CONTRACT_LIST,
} from "@/mock/contracts"



import type {
  AllocationSessionView,
  AllocationTrack,
  FormalSubmitResult,
  PayableDetailView,
  PayablePriorityPolicyView,
  PayableRow,
  PaymentRow,
  PostInvoiceInput,
  PostPaymentInput,
  PurchaseInvoiceRow,
  ReverseInvoiceInput,
  ReversePaymentInput,
  SaveAllocationDraftInput,
  SupplierAccountsListView,
  SupplierAccountsQuery,
  UnallocatedRow,
} from "@/features/supplier-payables/types"
import {
  PAYABLE_STATUS_LABEL,
  PAYABLE_STATUS_TONE,
  SOURCE_TYPE_LABEL,
} from "@/features/supplier-payables/types"
import {
  SEED_INVOICES,
  SEED_PAYABLES,
  SEED_PAYMENTS,
  W12_DEFAULT_POLICY,
  W12_SUPPLIERS,
  maskBankRef,
  sourceHref,
  type SeedInvoice,
  type SeedPayable,
  type SeedPayment,
} from "@/mock/supplier-payables"

export type QueueTaskOutcome = "succeeded" | "blocked" | "rejected"

const completedQueueTasks = new Map<string, Set<string>>()
const heldQueueTasks = new Map<string, Set<string>>()
export function getCompletedQueueTaskIds(workspaceId: string): ReadonlySet<string> {
  return completedQueueTasks.get(workspaceId) ?? new Set()
}

export function getHeldQueueTaskIds(workspaceId: string): ReadonlySet<string> {
  return heldQueueTasks.get(workspaceId) ?? new Set()
}

/** Terminal outcomes that remove the task from the active open queue. */
export function markQueueTaskCompleted(
  workspaceId: string,
  taskId: string
): void {
  const set = completedQueueTasks.get(workspaceId) ?? new Set()
  set.add(taskId)
  completedQueueTasks.set(workspaceId, set)
  // Completing/rejecting clears any hold mark.
  const held = heldQueueTasks.get(workspaceId)
  if (held?.has(taskId)) {
    held.delete(taskId)
  }
}

/** 暂挂：保留在有效队列，仅标记 hold，不进入 completed 集合。 */
export function markQueueTaskHeld(workspaceId: string, taskId: string): void {
  const set = heldQueueTasks.get(workspaceId) ?? new Set()
  set.add(taskId)
  heldQueueTasks.set(workspaceId, set)
}

export function applyQueueTaskOutcome(
  workspaceId: string,
  taskId: string,
  outcome: QueueTaskOutcome
): void {
  if (outcome === "blocked") {
    markQueueTaskHeld(workspaceId, taskId)
    return
  }
  markQueueTaskCompleted(workspaceId, taskId)
}

// 简易验收摘要见文件末尾 W06 区块（getSalesOrderAcceptance / postSalesOrderAcceptance）

// ─── W02 work_item session mock ─────────────────────────────────────────────

export type SessionLease = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  leaseExpiresAt: string
  ownerUserId: string
  subjectVersion?: string
  subjectHash: string
}

/** Public lease state without token (safe for action results / UI). */
export type SessionLeaseState = {
  workItemId: string
  leaseVersion: number
  leaseExpiresAt: string
}

export type WorkItemTerminalRecord = {
  status: "COMPLETED" | "TRANSFERRED" | "CLOSED"
  recordId: string
  businessResult?: { kind: string; reference: string; summary: string }
  reasonCode?: string
  replacementWorkItemId?: string
  successorWorkItemId?: string
  closedAt: string
}

export type WorkItemActionRecord = {
  actionRecordId: string
  actionKind: string
  workItemStatus: "PENDING" | "IN_PROGRESS"
  evidenceNote?: string
  recordedAt: string
  lease?: SessionLeaseState
  subjectHash: string
}

export type IdempotencyEntry =
  | { state: "pending"; startedAt: string; kind: string }
  | {
      state: "succeeded"
      kind: string
      payload: unknown
      finishedAt: string
    }
  | { state: "failed"; kind: string; error: string; finishedAt: string }

const workItemLeases = new Map<string, SessionLease>()
const workItemTerminal = new Map<string, WorkItemTerminalRecord>()
const workItemHeld = new Set<string>()
const workItemSubject = new Map<
  string,
  { subjectVersion: string; subjectHash: string; leaseVersion: number }
>()
const workItemActions = new Map<string, WorkItemActionRecord[]>()
const idempotencyStore = new Map<string, IdempotencyEntry>()
/** Permission version bumps invalidate in-memory claim tokens (UI must drop snapshots). */
let permissionVersion = 1
let permissionRevoked = false

export function getPermissionVersion(): number {
  return permissionVersion
}

export function isPermissionRevoked(): boolean {
  return permissionRevoked
}

/** Demo: revoke processing rights and bump permission version. */
export function revokeWorkItemPermission(): void {
  permissionRevoked = true
  permissionVersion += 1
  workItemLeases.clear()
}

export function restoreWorkItemPermission(): void {
  permissionRevoked = false
  permissionVersion += 1
}

export function getWorkItemSubject(workItemId: string) {
  return workItemSubject.get(workItemId) ?? null
}

export function ensureWorkItemSubject(
  workItemId: string,
  seed: { subjectVersion: string; subjectHash: string; leaseVersion: number }
): { subjectVersion: string; subjectHash: string; leaseVersion: number } {
  const existing = workItemSubject.get(workItemId)
  if (existing) return existing
  workItemSubject.set(workItemId, { ...seed })
  return seed
}

/** Bump subject version (for conflict demos / refresh). */
export function bumpWorkItemSubject(workItemId: string): {
  subjectVersion: string
  subjectHash: string
} {
  const current = workItemSubject.get(workItemId)
  if (!current) {
    const next = {
      subjectVersion: "v2",
      subjectHash: `sha_${workItemId}_bumped`,
      leaseVersion: 1,
    }
    workItemSubject.set(workItemId, next)
    return next
  }
  const versionNum =
    Number.parseInt(current.subjectVersion.replace(/\D/g, ""), 10) || 1
  const next = {
    subjectVersion: `v${versionNum + 1}`,
    subjectHash: `sha_${workItemId}_v${versionNum + 1}`,
    leaseVersion: current.leaseVersion,
  }
  workItemSubject.set(workItemId, next)
  return next
}

export function getSessionLease(workItemId: string): SessionLease | null {
  return workItemLeases.get(workItemId) ?? null
}

export function getSessionLeaseState(
  workItemId: string
): SessionLeaseState | null {
  const lease = workItemLeases.get(workItemId)
  if (!lease) return null
  return {
    workItemId: lease.workItemId,
    leaseVersion: lease.leaseVersion,
    leaseExpiresAt: lease.leaseExpiresAt,
  }
}

export function clearSessionLease(workItemId: string): void {
  workItemLeases.delete(workItemId)
}

/** Drop lease token while keeping task processable after re-claim (租约丢失). */
export function loseSessionLease(workItemId: string): void {
  workItemLeases.delete(workItemId)
}

export function isWorkItemHeld(workItemId: string): boolean {
  return workItemHeld.has(workItemId)
}

export function getWorkItemTerminal(
  workItemId: string
): WorkItemTerminalRecord | null {
  return workItemTerminal.get(workItemId) ?? null
}

export function getWorkItemActionHistory(
  workItemId: string
): readonly WorkItemActionRecord[] {
  return workItemActions.get(workItemId) ?? []
}

export function getIdempotencyEntry(
  key: string
): IdempotencyEntry | undefined {
  return idempotencyStore.get(key)
}

export function setIdempotencyPending(key: string, kind: string): void {
  idempotencyStore.set(key, {
    state: "pending",
    kind,
    startedAt: new Date().toISOString(),
  })
}

export function setIdempotencySucceeded(
  key: string,
  kind: string,
  payload: unknown
): void {
  idempotencyStore.set(key, {
    state: "succeeded",
    kind,
    payload,
    finishedAt: new Date().toISOString(),
  })
}

export function setIdempotencyFailed(
  key: string,
  kind: string,
  error: string
): void {
  idempotencyStore.set(key, {
    state: "failed",
    kind,
    error,
    finishedAt: new Date().toISOString(),
  })
}

function newToken(): string {
  return `ct_${Math.random().toString(36).slice(2, 12)}_${Date.now().toString(36)}`
}

function newRecordId(prefix: string): string {
  return `${prefix}-${Date.now().toString(36).toUpperCase()}`
}

export class WorkItemMockError extends Error {
  code:
    | "LEASE_LOST"
    | "LEASE_CONFLICT"
    | "VERSION_CONFLICT"
    | "PERMISSION_REVOKED"
    | "ACTION_NOT_ALLOWED"
    | "TIMEOUT"
    | "NOT_FOUND"
    | "ALREADY_TERMINAL"

  constructor(
    code: WorkItemMockError["code"],
    message: string
  ) {
    super(message)
    this.name = "WorkItemMockError"
    this.code = code
  }
}

export function claimWorkItemSession(input: {
  workItemId: string
  subjectVersion: string
  subjectHash: string
  leaseVersion: number
  ownerUserId?: string
}): SessionLease {
  if (permissionRevoked) {
    throw new WorkItemMockError(
      "PERMISSION_REVOKED",
      "当前权限已收回，不能领取任务。"
    )
  }
  const terminal = workItemTerminal.get(input.workItemId)
  if (terminal) {
    throw new WorkItemMockError(
      "ALREADY_TERMINAL",
      `任务已结束（${terminal.status}），不能领取。`
    )
  }
  const existing = workItemLeases.get(input.workItemId)
  if (
    existing &&
    existing.ownerUserId !== (input.ownerUserId ?? "user_wangmin") &&
    new Date(existing.leaseExpiresAt).getTime() > Date.now()
  ) {
    throw new WorkItemMockError(
      "LEASE_CONFLICT",
      "任务已被其他用户领取，正在处理中，不能同时处理。"
    )
  }

  const subject = ensureWorkItemSubject(input.workItemId, {
    subjectVersion: input.subjectVersion,
    subjectHash: input.subjectHash,
    leaseVersion: Math.max(1, input.leaseVersion),
  })

  const lease: SessionLease = {
    workItemId: input.workItemId,
    claimToken: newToken(),
    leaseVersion: subject.leaseVersion + (existing ? 1 : 0) || 1,
    leaseExpiresAt: new Date(Date.now() + 30 * 60 * 1000).toISOString(),
    ownerUserId: input.ownerUserId ?? "user_wangmin",
    subjectVersion: subject.subjectVersion,
    subjectHash: subject.subjectHash,
  }
  // Normalize leaseVersion
  const nextVersion = (existing?.leaseVersion ?? subject.leaseVersion) + 1
  lease.leaseVersion = nextVersion
  subject.leaseVersion = nextVersion
  workItemSubject.set(input.workItemId, subject)
  workItemLeases.set(input.workItemId, lease)
  workItemHeld.delete(input.workItemId)
  return lease
}

function requireValidLease(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
}): SessionLease {
  if (permissionRevoked) {
    workItemLeases.delete(input.workItemId)
    throw new WorkItemMockError(
      "PERMISSION_REVOKED",
      "权限已收回：临时信息已清除，不能提交。"
    )
  }
  const lease = workItemLeases.get(input.workItemId)
  if (!lease) {
    throw new WorkItemMockError(
      "LEASE_LOST",
      "操作已失效，本地输入可保留但不能提交。请重新领取。"
    )
  }
  if (lease.claimToken !== input.claimToken) {
    throw new WorkItemMockError(
      "LEASE_LOST",
      "操作已失效或过期，不能提交。"
    )
  }
  if (lease.leaseVersion !== input.leaseVersion) {
    throw new WorkItemMockError(
      "LEASE_CONFLICT",
      "版本冲突，请刷新后重新领取。"
    )
  }
  if (new Date(lease.leaseExpiresAt).getTime() <= Date.now()) {
    workItemLeases.delete(input.workItemId)
    throw new WorkItemMockError(
      "LEASE_LOST",
      "操作已过期，本地输入可保留但不能提交。"
    )
  }
  return lease
}

export function applyWorkItemActionSession(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  idempotencyKey: string
  action: { kind: string; note?: string }
  /** When true, leave idempotency as pending and throw TIMEOUT (no state change). */
  simulateTimeout?: boolean
}): WorkItemActionRecord {
  const existingIdem = idempotencyStore.get(input.idempotencyKey)
  if (existingIdem?.state === "succeeded") {
    return existingIdem.payload as WorkItemActionRecord
  }
  if (existingIdem?.state === "pending") {
    throw new WorkItemMockError(
      "TIMEOUT",
      "上次动作结果仍不确定，请按原任务号查询最终结果。"
    )
  }

  if (input.simulateTimeout) {
    setIdempotencyPending(input.idempotencyKey, input.action.kind)
    throw new WorkItemMockError(
      "TIMEOUT",
      "网络超时：任务状态未在本地变更，请按原任务号查询最终结果。"
    )
  }

  // If a prior timeout left pending, allow "query" path to finalize DEFER/SAVE
  if (existingIdem?.state === "failed") {
    // fall through to retry
  }

  requireValidLease({
    workItemId: input.workItemId,
    claimToken: input.claimToken,
    leaseVersion: input.leaseVersion,
  })

  const subject = workItemSubject.get(input.workItemId)
  if (subject && subject.subjectHash !== input.expectedSubjectHash) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "版本或数据版本已变化，请刷新比较后再提交。"
    )
  }

  const terminal = workItemTerminal.get(input.workItemId)
  if (terminal) {
    throw new WorkItemMockError(
      "ALREADY_TERMINAL",
      `任务已结束（${terminal.status}），不能执行任务内动作。`
    )
  }

  const leaseState = getSessionLeaseState(input.workItemId) ?? undefined
  const status: "PENDING" | "IN_PROGRESS" =
    input.action.kind === "DEFER" ? "PENDING" : "IN_PROGRESS"

  if (input.action.kind === "DEFER") {
    workItemHeld.add(input.workItemId)
    // DEFER may release lease per server; mock releases token but keeps hold mark
    workItemLeases.delete(input.workItemId)
  }

  const record: WorkItemActionRecord = {
    actionRecordId: newRecordId("ACT"),
    actionKind: input.action.kind,
    workItemStatus: status,
    evidenceNote: input.action.note,
    recordedAt: new Date().toISOString(),
    lease: input.action.kind === "DEFER" ? undefined : leaseState,
    subjectHash: subject?.subjectHash ?? input.expectedSubjectHash,
  }

  const history = workItemActions.get(input.workItemId) ?? []
  history.push(record)
  workItemActions.set(input.workItemId, history)
  setIdempotencySucceeded(input.idempotencyKey, input.action.kind, record)
  return record
}

/**
 * Finalize a previously timed-out action with the same idempotency key.
 * Demo path: if pending, complete the deferred action successfully without re-token
 * only for QUERY_RESULT; for DEFER/SAVE the client re-submits with same key after lease check.
 */
export function resolvePendingIdempotencyAsSuccess(input: {
  idempotencyKey: string
  finalize: () => WorkItemActionRecord | CompleteSessionResult | CloseSessionResult | TransferSessionResult
}): unknown {
  const entry = idempotencyStore.get(input.idempotencyKey)
  if (!entry) {
    throw new WorkItemMockError("NOT_FOUND", "未找到该任务号的待查结果。")
  }
  if (entry.state === "succeeded") return entry.payload
  if (entry.state === "failed") {
    throw new WorkItemMockError("NOT_FOUND", entry.error)
  }
  // pending → run finalize
  const payload = input.finalize()
  return payload
}

export type CompleteSessionResult = {
  workItemId: string
  workItemStatus: "COMPLETED"
  completionRecordId: string
  businessResult: { kind: string; reference: string; summary: string }
  subjectVersion?: string
  subjectHash: string
}

export function completeWorkItemSession(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  idempotencyKey: string
  decision: { kind: string; note?: string; summary?: string }
  simulateTimeout?: boolean
}): CompleteSessionResult {
  const existingIdem = idempotencyStore.get(input.idempotencyKey)
  if (existingIdem?.state === "succeeded") {
    return existingIdem.payload as CompleteSessionResult
  }
  if (existingIdem?.state === "pending") {
    throw new WorkItemMockError(
      "TIMEOUT",
      "上次完成结果仍不确定，请按原任务号查询，勿跳到下一项。"
    )
  }

  if (input.simulateTimeout) {
    setIdempotencyPending(input.idempotencyKey, "COMPLETE")
    throw new WorkItemMockError(
      "TIMEOUT",
      "网络超时：未写入完成态，请按原任务号查询最终结果。"
    )
  }

  requireValidLease({
    workItemId: input.workItemId,
    claimToken: input.claimToken,
    leaseVersion: input.leaseVersion,
  })

  const subject = workItemSubject.get(input.workItemId)
  if (subject && subject.subjectHash !== input.expectedSubjectHash) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "数据版本已变更，完成已阻止。本地输入已保留。"
    )
  }

  if (workItemTerminal.get(input.workItemId)) {
    throw new WorkItemMockError("ALREADY_TERMINAL", "任务已结束，不能再次完成。")
  }

  const completionRecordId = newRecordId("CMP")
  const result: CompleteSessionResult = {
    workItemId: input.workItemId,
    workItemStatus: "COMPLETED",
    completionRecordId,
    businessResult: {
      kind: input.decision.kind,
      reference: completionRecordId,
      summary:
        input.decision.summary ??
        `业务结论「${input.decision.kind}」与任务完成同一事务生效`,
    },
    subjectVersion: subject?.subjectVersion,
    subjectHash: subject?.subjectHash ?? input.expectedSubjectHash,
  }

  workItemTerminal.set(input.workItemId, {
    status: "COMPLETED",
    recordId: completionRecordId,
    businessResult: result.businessResult,
    closedAt: new Date().toISOString(),
  })
  workItemHeld.delete(input.workItemId)
  workItemLeases.delete(input.workItemId)
  setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", result)
  return result
}

/** Complete a previously pending COMPLETE idempotency key (demo recovery). */
export function finalizePendingComplete(input: {
  idempotencyKey: string
  workItemId: string
  decision: { kind: string; note?: string; summary?: string }
  expectedSubjectHash: string
}): CompleteSessionResult {
  const entry = idempotencyStore.get(input.idempotencyKey)
  if (entry?.state === "succeeded") {
    return entry.payload as CompleteSessionResult
  }
  if (entry?.state !== "pending") {
    throw new WorkItemMockError(
      "NOT_FOUND",
      "没有可查询的未决完成结果。"
    )
  }
  // Recovery path: mock server eventually committed — do not require live token
  if (workItemTerminal.get(input.workItemId)) {
    const terminal = workItemTerminal.get(input.workItemId)!
    const recovered: CompleteSessionResult = {
      workItemId: input.workItemId,
      workItemStatus: "COMPLETED",
      completionRecordId: terminal.recordId,
      businessResult: terminal.businessResult ?? {
        kind: input.decision.kind,
        reference: terminal.recordId,
        summary: "已完成",
      },
      subjectHash: input.expectedSubjectHash,
    }
    setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", recovered)
    return recovered
  }

  const completionRecordId = newRecordId("CMP")
  const subject = workItemSubject.get(input.workItemId)
  const result: CompleteSessionResult = {
    workItemId: input.workItemId,
    workItemStatus: "COMPLETED",
    completionRecordId,
    businessResult: {
      kind: input.decision.kind,
      reference: completionRecordId,
      summary:
        input.decision.summary ??
        `业务结论与任务完成已一并生效`,
    },
    subjectVersion: subject?.subjectVersion,
    subjectHash: subject?.subjectHash ?? input.expectedSubjectHash,
  }
  workItemTerminal.set(input.workItemId, {
    status: "COMPLETED",
    recordId: completionRecordId,
    businessResult: result.businessResult,
    closedAt: new Date().toISOString(),
  })
  workItemHeld.delete(input.workItemId)
  workItemLeases.delete(input.workItemId)
  setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", result)
  return result
}

export type CloseSessionResult = {
  workItemId: string
  workItemStatus: "CLOSED"
  closureRecordId: string
  reasonCode: string
  replacementWorkItemId?: string
  closureEvidenceReference: string
  subjectHash: string
}

export function closeWorkItemSession(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  idempotencyKey: string
  closeAllowed: boolean
  closure: {
    kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED" | "CLOSE_WITH_REPLACEMENT"
    reasonCode: string
    replacementWorkItemId?: string
    closureEvidenceReference: string
    comment?: string
  }
}): CloseSessionResult {
  const existingIdem = idempotencyStore.get(input.idempotencyKey)
  if (existingIdem?.state === "succeeded") {
    return existingIdem.payload as CloseSessionResult
  }

  if (!input.closeAllowed) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "审批、确认、结果未知和补偿任务不允许人工关闭。"
    )
  }

  requireValidLease({
    workItemId: input.workItemId,
    claimToken: input.claimToken,
    leaseVersion: input.leaseVersion,
  })

  if (
    (input.closure.kind === "CLOSE_DUPLICATE" ||
      input.closure.kind === "CLOSE_WITH_REPLACEMENT") &&
    !input.closure.replacementWorkItemId
  ) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "重复/替代关闭必须提供替代任务引用。"
    )
  }

  const closureRecordId = newRecordId("CLS")
  const result: CloseSessionResult = {
    workItemId: input.workItemId,
    workItemStatus: "CLOSED",
    closureRecordId,
    reasonCode: input.closure.reasonCode,
    replacementWorkItemId: input.closure.replacementWorkItemId,
    closureEvidenceReference: input.closure.closureEvidenceReference,
    subjectHash: input.expectedSubjectHash,
  }

  workItemTerminal.set(input.workItemId, {
    status: "CLOSED",
    recordId: closureRecordId,
    reasonCode: input.closure.reasonCode,
    replacementWorkItemId: input.closure.replacementWorkItemId,
    closedAt: new Date().toISOString(),
  })
  workItemLeases.delete(input.workItemId)
  workItemHeld.delete(input.workItemId)
  setIdempotencySucceeded(input.idempotencyKey, "CLOSE", result)
  return result
}

export type TransferSessionResult = {
  originalWorkItemId: string
  originalWorkItemStatus: "TRANSFERRED"
  transferRecordId: string
  successorWorkItemId: string
  successorWorkItemStatus: "UNCLAIMED" | "PENDING"
}

export function transferWorkItemSession(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  idempotencyKey: string
  transfer: { toUserLabel: string; reason: string }
}): TransferSessionResult {
  const existingIdem = idempotencyStore.get(input.idempotencyKey)
  if (existingIdem?.state === "succeeded") {
    return existingIdem.payload as TransferSessionResult
  }

  requireValidLease({
    workItemId: input.workItemId,
    claimToken: input.claimToken,
    leaseVersion: input.leaseVersion,
  })

  const transferRecordId = newRecordId("XFR")
  const successorWorkItemId = `${input.workItemId}_xfr_${Date.now().toString(36)}`
  const result: TransferSessionResult = {
    originalWorkItemId: input.workItemId,
    originalWorkItemStatus: "TRANSFERRED",
    transferRecordId,
    successorWorkItemId,
    successorWorkItemStatus: "UNCLAIMED",
  }

  workItemTerminal.set(input.workItemId, {
    status: "TRANSFERRED",
    recordId: transferRecordId,
    successorWorkItemId,
    closedAt: new Date().toISOString(),
  })
  workItemLeases.delete(input.workItemId)
  workItemHeld.delete(input.workItemId)
  setIdempotencySucceeded(input.idempotencyKey, "TRANSFER", result)
  return result
}

/**
 * Query final result for an idempotency key without advancing UI.
 * For pending COMPLETE keys, does not auto-finalize unless `finalizePending` is true.
 */
export function queryIdempotencyResult(idempotencyKey: string): IdempotencyEntry | null {
  return idempotencyStore.get(idempotencyKey) ?? null
}

// —— W05 sales order session domain ——

export type W05ProcurementResolutionOutcome = {
  outcome:
    | "CHANGED_TERMS_RESUBMITTED"
    | "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED"
    | "VOIDED_AFTER_PROCUREMENT_REJECTION"
    | "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
    | "LOW_MARGIN_REJECTED_TO_SALES"
  reference: string
  detail: string
  newSubmissionNo?: number
  newSubjectHash?: string
  newWorkItemId?: string
  reviewStatus?:
    | "REJECTED"
    | "PENDING_LOW_MARGIN_MANAGER"
    | "RESOLVED"
    | "VOIDED"
  primaryStatusLabel?: string
}

const w05RejectionOutcomes = new Map<string, W05ProcurementResolutionOutcome>()
const w05RejectionIdempotency = new Map<string, W05ProcurementResolutionOutcome>()
const w05ChangeOrders = new Map<
  string,
  {
    id: string
    statusLabel: string
    statusTone: "info" | "warning" | "success" | "neutral"
    baseRevisionNo: number
    createdAt: string
    impactPath: "procurement" | "operations"
  }
>()
const w05CardClaims = new Map<
  string,
  {
    claimToken: string
    leaseVersion: number
    claimedByLabel: string
    expiresAt: string
  }
>()
const w05CardTerminal = new Map<
  string,
  {
    outcome:
      | "MANAGER_APPROVED"
      | "OPERATIONS_APPROVED_AND_EFFECTIVE"
      | "REJECTED_TO_SALES"
    reference: string
    detail: string
    nextWorkItemId?: string
    primaryStatusLabel?: string
  }
>()
const w05CardIdempotency = new Map<string, (typeof w05CardTerminal extends Map<string, infer V> ? V : never)>()
const w05DraftPriceAdjust = new Map<string, { unitPriceGross: string; note: string }>()
const w05ExportJobs = new Map<
  string,
  {
    jobId: string
    status: "queued" | "running" | "succeeded" | "failed"
    rowCount: number
    permissionVersion: string
    createdAt: string
    downloadLabel: string
  }
>()

export function getW05RejectionOutcome(salesOrderId: string) {
  return w05RejectionOutcomes.get(salesOrderId) ?? null
}

export function getW05RejectionByIdempotency(key: string) {
  return w05RejectionIdempotency.get(key) ?? null
}

export function resolveW05ProcurementRejection(input: {
  salesOrderId: string
  action:
    | "RESUBMIT_CHANGED_TERMS"
    | "REQUEST_LOW_MARGIN_ACCEPTANCE"
    | "VOID_AFTER_REJECTION"
  idempotencyKey: string
  lowMarginReason?: string
  voidReason?: string
  priceAdjusted?: boolean
}): W05ProcurementResolutionOutcome {
  const cached = w05RejectionIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  let outcome: W05ProcurementResolutionOutcome
  const stamp = Date.now().toString(36).toUpperCase()

  if (input.action === "RESUBMIT_CHANGED_TERMS") {
    if (!input.priceAdjusted && !w05DraftPriceAdjust.has(input.salesOrderId)) {
      throw new Error("NO_COMMERCIAL_CHANGE")
    }
    outcome = {
      outcome: "CHANGED_TERMS_RESUBMITTED",
      reference: `PR-RESUB-${stamp}`,
      detail:
        "已冻结新提交与新 subjectHash，并创建唯一新 PROCUREMENT_CONFIRMATION；旧提交与旧采购二次确认任务保持已完成驳回。",
      newSubmissionNo: 2,
      newSubjectHash: `sha256:new…${stamp.slice(0, 4)}`,
      newWorkItemId: `wi_pc_new_${stamp.toLowerCase()}`,
      reviewStatus: "RESOLVED",
      primaryStatusLabel: "待二次确认",
    }
  } else if (input.action === "REQUEST_LOW_MARGIN_ACCEPTANCE") {
    outcome = {
      outcome: "LOW_MARGIN_MANAGER_CONFIRMATION_CREATED",
      reference: `PR-LM-${stamp}`,
      detail:
        "已冻结新提交与新数据版本，并创建唯一上级确认任务；此时不创建采购确认任务，须上级通过后方可再次进入采购二次确认。",
      newSubmissionNo: 2,
      newSubjectHash: `sha256:lm…${stamp.slice(0, 4)}`,
      newWorkItemId: `wi_lm_${stamp.toLowerCase()}`,
      reviewStatus: "PENDING_LOW_MARGIN_MANAGER",
      primaryStatusLabel: "待销售处理",
    }
  } else {
    outcome = {
      outcome: "VOIDED_AFTER_PROCUREMENT_REJECTION",
      reference: `PR-VOID-${stamp}`,
      detail: `销售单已作废并保留驳回历史。原因：${input.voidReason ?? "不做"}`,
      reviewStatus: "VOIDED",
      primaryStatusLabel: "已作废",
    }
  }

  w05RejectionIdempotency.set(input.idempotencyKey, outcome)
  w05RejectionOutcomes.set(input.salesOrderId, outcome)
  return outcome
}

export function decideW05LowMargin(input: {
  salesOrderId: string
  workItemId: string
  decision: "APPROVE" | "REJECT"
  idempotencyKey: string
  reason?: string
}): W05ProcurementResolutionOutcome {
  const cached = w05RejectionIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  const stamp = Date.now().toString(36).toUpperCase()
  const outcome: W05ProcurementResolutionOutcome =
    input.decision === "APPROVE"
      ? {
          outcome: "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED",
          reference: `LM-OK-${stamp}`,
          detail:
            "上级已同意低毛利承接；同事务完成低毛利任务并创建唯一新 PROCUREMENT_CONFIRMATION。销售单未生效。",
          newSubmissionNo: 2,
          newSubjectHash: `sha256:lmok…${stamp.slice(0, 4)}`,
          newWorkItemId: `wi_pc_after_lm_${stamp.toLowerCase()}`,
          reviewStatus: "RESOLVED",
          primaryStatusLabel: "待二次确认",
        }
      : {
          outcome: "LOW_MARGIN_REJECTED_TO_SALES",
          reference: `LM-REJ-${stamp}`,
          detail: `上级驳回低毛利承接（${input.reason ?? "未说明"}）；不创建采购任务，销售回到三条固定出路。已完成低毛利任务不可复用。`,
          reviewStatus: "REJECTED",
          primaryStatusLabel: "待销售处理",
        }

  w05RejectionIdempotency.set(input.idempotencyKey, outcome)
  w05RejectionOutcomes.set(input.salesOrderId, outcome)
  return outcome
}

export function markW05DraftPriceAdjusted(
  salesOrderId: string,
  unitPriceGross: string,
  note: string
): void {
  w05DraftPriceAdjust.set(salesOrderId, { unitPriceGross, note })
}

export function hasW05DraftPriceAdjusted(salesOrderId: string): boolean {
  return w05DraftPriceAdjust.has(salesOrderId)
}

export function getW05DraftPriceAdjusted(salesOrderId: string) {
  return w05DraftPriceAdjust.get(salesOrderId) ?? null
}

export function getW05ChangeOrder(salesOrderId: string) {
  return w05ChangeOrders.get(salesOrderId) ?? null
}

export function startW05SalesChangeOrder(input: {
  salesOrderId: string
  baseRevisionNo: number
  nature: "physical_service" | "card_voucher"
}): {
  id: string
  statusLabel: string
  statusTone: "info" | "warning" | "success" | "neutral"
  baseRevisionNo: number
  createdAt: string
  impactPath: "procurement" | "operations"
} {
  const existing = w05ChangeOrders.get(input.salesOrderId)
  if (existing) return existing
  const impactPath =
    input.nature === "card_voucher" ? "operations" : "procurement"
  const order = {
    id: `SCO-${input.salesOrderId}-${Date.now().toString(36).toUpperCase()}`,
    statusLabel:
      impactPath === "operations"
        ? "待运营执行影响确认"
        : "待采购履约影响确认",
    statusTone: "warning" as const,
    baseRevisionNo: input.baseRevisionNo,
    createdAt: new Date().toISOString(),
    impactPath: impactPath as "procurement" | "operations",
  }
  w05ChangeOrders.set(input.salesOrderId, order)
  return order
}

export function claimW05CardApproval(workItemId: string): {
  claimToken: string
  leaseVersion: number
  claimedByLabel: string
  expiresAt: string
} {
  const existing = w05CardClaims.get(workItemId)
  const leaseVersion = (existing?.leaseVersion ?? 0) + 1
  const claimToken = `ct_card_${workItemId}_${leaseVersion}_${Math.random().toString(36).slice(2, 10)}`
  const lease = {
    claimToken,
    leaseVersion,
    claimedByLabel: "当前用户 · 销售经理",
    expiresAt: new Date(Date.now() + 15 * 60 * 1000).toISOString(),
  }
  w05CardClaims.set(workItemId, lease)
  return lease
}

/** Public lease without claimToken */
export function getW05CardLeasePublic(workItemId: string) {
  const lease = w05CardClaims.get(workItemId)
  if (!lease) return null
  return {
    claimedByLabel: lease.claimedByLabel,
    expiresAt: lease.expiresAt,
    leaseVersion: lease.leaseVersion,
  }
}

export function verifyW05CardClaim(
  workItemId: string,
  claimToken: string,
  leaseVersion: number
): boolean {
  const lease = w05CardClaims.get(workItemId)
  return Boolean(
    lease &&
      lease.claimToken === claimToken &&
      lease.leaseVersion === leaseVersion
  )
}

export function getW05CardTerminal(workItemId: string) {
  return w05CardTerminal.get(workItemId) ?? null
}

export function completeW05CardApproval(input: {
  workItemId: string
  workItemType: "CARD_SALES_MANAGER_APPROVAL" | "CARD_SALES_OPERATION_APPROVAL"
  decision: "APPROVE" | "REJECT"
  claimToken: string
  leaseVersion: number
  idempotencyKey: string
  reasonCode?: string
}): {
  outcome:
    | "MANAGER_APPROVED"
    | "OPERATIONS_APPROVED_AND_EFFECTIVE"
    | "REJECTED_TO_SALES"
  reference: string
  detail: string
  nextWorkItemId?: string
  primaryStatusLabel?: string
} {
  const cached = w05CardIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  if (!verifyW05CardClaim(input.workItemId, input.claimToken, input.leaseVersion)) {
    throw new Error("LEASE_INVALID")
  }

  const stamp = Date.now().toString(36).toUpperCase()
  let result: {
    outcome:
      | "MANAGER_APPROVED"
      | "OPERATIONS_APPROVED_AND_EFFECTIVE"
      | "REJECTED_TO_SALES"
    reference: string
    detail: string
    nextWorkItemId?: string
    primaryStatusLabel?: string
  }

  if (input.decision === "REJECT") {
    result = {
      outcome: "REJECTED_TO_SALES",
      reference: `CARD-REJ-${stamp}`,
      detail: `已驳回并退回销售处理（${input.reasonCode ?? "未分类"}）；修改后须从领导审批重新开始。`,
      primaryStatusLabel: "草稿",
    }
  } else if (input.workItemType === "CARD_SALES_MANAGER_APPROVAL") {
    result = {
      outcome: "MANAGER_APPROVED",
      reference: `CARD-MGR-${stamp}`,
      detail: "领导已通过；同事务创建唯一 CARD_SALES_OPERATION_APPROVAL，销售单进入待运营审批。",
      nextWorkItemId: `wi_card_ops_${stamp.toLowerCase()}`,
      primaryStatusLabel: "待运营审批",
    }
  } else {
    result = {
      outcome: "OPERATIONS_APPROVED_AND_EFFECTIVE",
      reference: `CARD-OPS-${stamp}`,
      detail:
        "运营已通过；同事务形成首个销售版本、应收与执行信息同步，销售单生效。",
      primaryStatusLabel: "已生效",
    }
  }

  w05CardIdempotency.set(input.idempotencyKey, result)
  w05CardTerminal.set(input.workItemId, result)
  w05CardClaims.delete(input.workItemId)
  return result
}

export function createW05ExportJob(input: {
  rowCount: number
  permissionVersion: string
}): {
  jobId: string
  status: "queued" | "running" | "succeeded" | "failed"
  rowCount: number
  permissionVersion: string
  createdAt: string
  downloadLabel: string
} {
  const jobId = `EXP-SO-${Date.now().toString(36).toUpperCase()}`
  const job = {
    jobId,
    status: "succeeded" as const,
    rowCount: input.rowCount,
    permissionVersion: input.permissionVersion,
    createdAt: new Date().toISOString(),
    downloadLabel: `销售单导出_${jobId}.csv（演示·受权限版本约束）`,
  }
  w05ExportJobs.set(jobId, job)
  return job
}

export function getW05ExportJob(jobId: string) {
  return w05ExportJobs.get(jobId) ?? null
}


// ─── W06 customer acceptance session mock ───────────────────────────────────

const acceptanceHistoryByOrder = new Map<string, AcceptanceHistoryItem[]>()
const acceptanceDraftByOrder = new Map<string, AcceptanceDraft>()
const acceptanceIdempotency = new Map<string, PostAcceptanceResult>()
const reverseIdempotency = new Map<string, ReverseAcceptanceResult>()
let acceptancePermissionRevoked = false

function parseAccQty(value: string): number {
  const n = Number(value)
  return Number.isFinite(n) ? n : 0
}

function formatAccQty(n: number): string {
  if (Number.isInteger(n)) return String(n)
  return n.toFixed(6).replace(/\.?0+$/, "")
}

export function getNetAcceptedAllocated(
  salesOrderId: string,
  fulfillmentLineId: string
): number {
  const baseline = listBaselineFactsForOrder(salesOrderId).find(
    (f) => f.fulfillmentLineId === fulfillmentLineId
  )
  let net = parseAccQty(baseline?.baselineAcceptedAllocated ?? "0")
  const history = acceptanceHistoryByOrder.get(salesOrderId) ?? []
  for (const item of history) {
    for (const line of item.lines) {
      for (const alloc of line.allocations) {
        if (alloc.fulfillmentLineId !== fulfillmentLineId) continue
        const q = parseAccQty(alloc.allocatedQuantity)
        net += alloc.direction === "APPLY" ? q : -q
      }
    }
  }
  return Math.max(0, net)
}

export function getEligibleQuantity(
  salesOrderId: string,
  fulfillmentLineId: string
): number {
  const baseline = listBaselineFactsForOrder(salesOrderId).find(
    (f) => f.fulfillmentLineId === fulfillmentLineId
  )
  if (!baseline) return 0
  const netSuccess = parseAccQty(baseline.netSuccessfulQuantity)
  const netAccepted = getNetAcceptedAllocated(salesOrderId, fulfillmentLineId)
  return Math.max(0, netSuccess - netAccepted)
}

export function listAcceptanceHistory(
  salesOrderId: string
): AcceptanceHistoryItem[] {
  return [...(acceptanceHistoryByOrder.get(salesOrderId) ?? [])].sort((a, b) =>
    a.postedAt < b.postedAt ? 1 : -1
  )
}

export function getAcceptanceDraft(
  salesOrderId: string
): AcceptanceDraft | null {
  if (acceptancePermissionRevoked) return null
  return acceptanceDraftByOrder.get(salesOrderId) ?? null
}

export function saveAcceptanceDraft(
  input: SaveAcceptanceDraftInput
): AcceptanceDraft {
  if (acceptancePermissionRevoked) {
    throw new Error("权限已收回，无法保存草稿")
  }
  const existing = acceptanceDraftByOrder.get(input.salesOrderId)
  if (
    existing &&
    input.expectedDraftVersion != null &&
    existing.draftVersion !== input.expectedDraftVersion
  ) {
    throw new Error("草稿版本冲突，请刷新后重试")
  }
  const next: AcceptanceDraft = {
    acceptanceDraftId:
      existing?.acceptanceDraftId ??
      input.acceptanceDraftId ??
      `ad_${input.salesOrderId}_${Date.now().toString(36)}`,
    draftVersion: (existing?.draftVersion ?? 0) + 1,
    salesOrderId: input.salesOrderId,
    acceptedAt: input.acceptedAt,
    comment: input.comment,
    lines: input.lines,
    updatedAt: new Date().toISOString(),
  }
  acceptanceDraftByOrder.set(input.salesOrderId, next)
  return next
}

export function clearAcceptanceDraft(salesOrderId: string): void {
  acceptanceDraftByOrder.delete(salesOrderId)
}

function deriveOverallResult(
  lines: AcceptanceDraftLine[]
): AcceptanceOverallResult {
  let hasReject = false
  let hasShort = false
  for (const line of lines) {
    if (parseAccQty(line.rejectedQuantity) > 0) hasReject = true
    if (parseAccQty(line.shortQuantity) > 0) hasShort = true
  }
  if (hasReject) return "REJECT"
  if (hasShort) return "SHORT"
  return "PASS"
}

function validatePostLines(
  salesOrderId: string,
  lines: AcceptanceDraftLine[]
): string | null {
  if (lines.length === 0) return "请至少登记一行验收明细"
  for (const line of lines) {
    const accepted = parseAccQty(line.acceptedQuantity)
    const short = parseAccQty(line.shortQuantity)
    const rejected = parseAccQty(line.rejectedQuantity)
    if (accepted < 0 || short < 0 || rejected < 0) {
      return "数量不得为负"
    }
    const resultTotal = accepted + short + rejected
    if (resultTotal <= 0) {
      return "每行验收通过、短少与拒收合计须大于 0"
    }
    if ((short > 0 || rejected > 0) && !line.reason.trim()) {
      return "短少、拒收或服务不通过时必须填写客户反馈原因"
    }
    const allocTotal = line.allocations.reduce(
      (sum, a) => sum + parseAccQty(a.allocatedQuantity),
      0
    )
    if (Math.abs(allocTotal - resultTotal) > 1e-9) {
      return "验收结果数量与履约批次分配不守恒，请调整后再过账"
    }
    for (const alloc of line.allocations) {
      const eligible = getEligibleQuantity(salesOrderId, alloc.fulfillmentLineId)
      if (parseAccQty(alloc.allocatedQuantity) > eligible + 1e-9) {
        return `履约批次分配超过净可验收量（上限 ${formatAccQty(eligible)}）`
      }
      if (parseAccQty(alloc.allocatedQuantity) <= 0) {
        return "分配数量必须大于 0"
      }
    }
  }
  return null
}

export function postCustomerAcceptance(
  input: PostAcceptanceInput
): PostAcceptanceResult {
  const cached = acceptanceIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  if (acceptancePermissionRevoked) {
    return {
      status: "failed",
      message: "权限已收回，过账已停止；本地敏感草稿已清理",
    }
  }

  const draft = acceptanceDraftByOrder.get(input.salesOrderId)
  if (!draft || draft.acceptanceDraftId !== input.acceptanceDraftId) {
    return { status: "failed", message: "草稿不存在或已失效，请重新保存" }
  }
  if (draft.draftVersion !== input.expectedDraftVersion) {
    return { status: "failed", message: "草稿版本冲突，请刷新后重试" }
  }

  const validationError = validatePostLines(input.salesOrderId, input.lines)
  if (validationError) {
    return { status: "failed", message: validationError }
  }

  const overallResult = deriveOverallResult(input.lines)
  const acceptanceId = `ca_${Date.now().toString(36)}`
  const seq = listAcceptanceHistory(input.salesOrderId).length + 1
  const acceptanceNo = `YS${new Date().toISOString().slice(0, 10).replace(/-/g, "")}${String(seq).padStart(4, "0")}`

  const baselineById = new Map(
    listBaselineFactsForOrder(input.salesOrderId).map((f) => [
      f.fulfillmentLineId,
      f,
    ])
  )

  const historyItem: AcceptanceHistoryItem = {
    acceptanceId,
    acceptanceNo,
    status: "POSTED",
    acceptedAt: input.acceptedAt,
    postedAt: new Date().toISOString(),
    overallResult,
    recordedBy: "当前用户（演示）",
    version: 1,
    comment: input.comment || undefined,
    factOnlyNotice: FACT_ONLY_NOTICE,
    lines: input.lines.map((line) => {
      const firstFact = line.allocations[0]
        ? baselineById.get(line.allocations[0].fulfillmentLineId)
        : undefined
      return {
        salesOrderLineId: line.salesOrderLineId,
        lineNo: firstFact?.lineNo ?? 0,
        itemSnapshot: firstFact?.itemSnapshot ?? line.salesOrderLineId,
        unitCode: firstFact?.unitCode ?? "",
        acceptedQuantity: line.acceptedQuantity,
        shortQuantity: line.shortQuantity,
        rejectedQuantity: line.rejectedQuantity,
        reason: line.reason || undefined,
        allocations: line.allocations.map((alloc) => {
          const fact = baselineById.get(alloc.fulfillmentLineId)
          return {
            fulfillmentLineId: alloc.fulfillmentLineId,
            fulfillmentNo: fact?.fulfillmentNo ?? alloc.fulfillmentLineId,
            fulfillmentFactType: fact?.fulfillmentFactType ?? "WAREHOUSE_SHIP",
            salesOrderLineId: line.salesOrderLineId,
            direction: "APPLY" as const,
            allocatedQuantity: alloc.allocatedQuantity,
          }
        }),
      }
    }),
  }

  const list = acceptanceHistoryByOrder.get(input.salesOrderId) ?? []
  list.push(historyItem)
  acceptanceHistoryByOrder.set(input.salesOrderId, list)
  acceptanceDraftByOrder.delete(input.salesOrderId)

  const remainingFacts = listBaselineFactsForOrder(input.salesOrderId).filter(
    (f) => getEligibleQuantity(input.salesOrderId, f.fulfillmentLineId) > 0
  )
  const remainingQty = remainingFacts.reduce(
    (sum, f) =>
      sum + getEligibleQuantity(input.salesOrderId, f.fulfillmentLineId),
    0
  )

  const result: PostAcceptanceResult = {
    status: "succeeded",
    acceptanceNo,
    acceptanceId,
    remainingEligibleCount: remainingFacts.length,
    remainingEligibleQuantityLabel: formatAccQty(remainingQty),
    overallResult,
    factOnlyNotice: FACT_ONLY_NOTICE,
  }
  acceptanceIdempotency.set(input.idempotencyKey, result)
  return result
}

export function reverseCustomerAcceptance(
  input: ReverseAcceptanceInput
): ReverseAcceptanceResult {
  const cached = reverseIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  if (acceptancePermissionRevoked) {
    return { status: "failed", message: "权限已收回，无法冲正" }
  }

  const list = acceptanceHistoryByOrder.get(input.salesOrderId) ?? []
  const original = list.find((item) => item.acceptanceId === input.acceptanceId)
  if (!original) {
    return { status: "failed", message: "未找到原验收记录" }
  }
  if (original.status !== "POSTED") {
    return { status: "failed", message: "仅已过账且未完整冲正的验收可冲正" }
  }
  if (original.version !== input.expectedAcceptanceVersion) {
    return { status: "failed", message: "验收版本冲突，请刷新后重试" }
  }
  if (!input.reasonText.trim()) {
    return { status: "failed", message: "请填写冲正理由" }
  }

  const reverseId = `ca_rev_${Date.now().toString(36)}`
  const reverseNo = `YS-REV-${original.acceptanceNo}`

  const reverseItem: AcceptanceHistoryItem = {
    acceptanceId: reverseId,
    acceptanceNo: reverseNo,
    status: "POSTED",
    acceptedAt: new Date().toISOString(),
    postedAt: new Date().toISOString(),
    overallResult: original.overallResult,
    recordedBy: "当前用户（演示）",
    version: 1,
    comment: input.reasonText.trim(),
    factOnlyNotice: FACT_ONLY_NOTICE,
    reversalOfAcceptanceId: original.acceptanceId,
    lines: original.lines.map((line) => ({
      ...line,
      reason: input.reasonText.trim(),
      allocations: line.allocations.map((alloc) => ({
        ...alloc,
        direction: "REVERSE" as const,
      })),
    })),
  }

  original.status = "REVERSED"
  original.reversedByAcceptanceId = reverseId
  original.version += 1
  list.push(reverseItem)
  acceptanceHistoryByOrder.set(input.salesOrderId, list)

  const result: ReverseAcceptanceResult = {
    status: "succeeded",
    reverseAcceptanceNo: reverseNo,
    reverseAcceptanceId: reverseId,
    originalAcceptanceNo: original.acceptanceNo,
  }
  reverseIdempotency.set(input.idempotencyKey, result)
  return result
}

export function revokeAcceptancePermission(): void {
  acceptancePermissionRevoked = true
  acceptanceDraftByOrder.clear()
}

export function restoreAcceptancePermission(): void {
  acceptancePermissionRevoked = false
}

export function isAcceptancePermissionRevoked(): boolean {
  return acceptancePermissionRevoked
}

/** 兼容旧调用：最近一条非冲正正向过账摘要 */
export function getSalesOrderAcceptance(salesOrderId: string) {
  const history = listAcceptanceHistory(salesOrderId)
  const latest = history.find(
    (item) => !item.reversalOfAcceptanceId && item.status === "POSTED"
  )
  if (!latest) return null
  const acceptedQty = latest.lines.reduce(
    (sum, line) => sum + parseAccQty(line.acceptedQuantity),
    0
  )
  return {
    acceptedQuantity: formatAccQty(acceptedQty),
    note:
      latest.comment ??
      latest.lines
        .map((l) => l.reason)
        .filter(Boolean)
        .join("；") ??
      "",
    reference: latest.acceptanceNo,
    postedAt: latest.postedAt,
  }
}

/** 兼容旧简化过账路径 */
export function postSalesOrderAcceptance(
  salesOrderId: string,
  payload: { acceptedQuantity: string; note: string; reference: string }
): void {
  const facts = listBaselineFactsForOrder(salesOrderId).filter(
    (f) => getEligibleQuantity(salesOrderId, f.fulfillmentLineId) > 0
  )
  const primary = facts[0]
  if (!primary) {
    const item: AcceptanceHistoryItem = {
      acceptanceId: `ca_legacy_${Date.now().toString(36)}`,
      acceptanceNo: payload.reference,
      status: "POSTED",
      acceptedAt: new Date().toISOString(),
      postedAt: new Date().toISOString(),
      overallResult: "PASS",
      recordedBy: "当前用户（演示）",
      version: 1,
      comment: payload.note,
      factOnlyNotice: FACT_ONLY_NOTICE,
      lines: [
        {
          salesOrderLineId: "unknown",
          lineNo: 0,
          itemSnapshot: "（简化过账）",
          unitCode: "",
          acceptedQuantity: payload.acceptedQuantity,
          shortQuantity: "0",
          rejectedQuantity: "0",
          reason: payload.note,
          allocations: [],
        },
      ],
    }
    const list = acceptanceHistoryByOrder.get(salesOrderId) ?? []
    list.push(item)
    acceptanceHistoryByOrder.set(salesOrderId, list)
    return
  }

  const draft = saveAcceptanceDraft({
    salesOrderId,
    acceptedAt: new Date().toISOString(),
    comment: payload.note,
    lines: [
      {
        salesOrderLineId: primary.salesOrderLineId,
        acceptedQuantity: payload.acceptedQuantity,
        shortQuantity: "0",
        rejectedQuantity: "0",
        reason: "",
        allocations: [
          {
            fulfillmentLineId: primary.fulfillmentLineId,
            allocatedQuantity: payload.acceptedQuantity,
          },
        ],
      },
    ],
  })

  postCustomerAcceptance({
    salesOrderId,
    acceptanceDraftId: draft.acceptanceDraftId,
    expectedDraftVersion: draft.draftVersion,
    expectedSalesOrderLockVersion: 1,
    idempotencyKey: `legacy-${payload.reference}`,
    acceptedAt: draft.acceptedAt,
    comment: payload.note,
    lines: draft.lines,
  })
}

// ─── W07 procurement confirmation drafts ───
// claimToken / lease / complete / defer use shared W02 work_item session helpers above.
// Only confirmation line drafts + editVersion are W07-specific working data.

type W07ConfirmationLineDraft = {
  lineKey: string
  submissionLineId: string
  supplierId: string
  supplierName: string
  confirmedQuantity: string
  latestCostGross: string
  inputTaxRate: string
  expectedDeliveryDate: string
  fulfillmentMode: string
  capabilityRevisionId: string
  capabilitySummary: string
  qualificationStatus: "VALID" | "INVALID" | "EXPIRING"
}

const procurementDrafts = new Map<
  string,
  { lines: W07ConfirmationLineDraft[]; editVersion: number }
>()

const procurementBusinessOutcomes = new Map<string, unknown>()

export function getProcurementDraft(workItemId: string) {
  return procurementDrafts.get(workItemId) ?? null
}

export function saveProcurementDraft(
  workItemId: string,
  lines: W07ConfirmationLineDraft[],
  expectedEditVersion: number
): { editVersion: number } {
  const current = procurementDrafts.get(workItemId)
  if (current && current.editVersion !== expectedEditVersion) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "确认编辑版本冲突，请重载服务端分行后再保存。"
    )
  }
  const base = current?.editVersion ?? expectedEditVersion
  const nextVersion = base + 1
  procurementDrafts.set(workItemId, {
    lines: lines.map((l) => ({ ...l })),
    editVersion: nextVersion,
  })
  return { editVersion: nextVersion }
}

export function getProcurementBusinessOutcome(workItemId: string) {
  return procurementBusinessOutcomes.get(workItemId) ?? null
}

export function setProcurementBusinessOutcome(
  workItemId: string,
  outcome: unknown
): void {
  procurementBusinessOutcomes.set(workItemId, outcome)
}

export function isProcurementWorkItemTerminal(workItemId: string): boolean {
  return workItemTerminal.has(workItemId)
}

export function isProcurementWorkItemHeld(workItemId: string): boolean {
  return workItemHeld.has(workItemId)
}


// ─── W10 inventory adjustment session mock ──────────────────────────────────

export type W10AdjustmentDraft = {
  stockAdjustmentId: string
  adjustmentNo: string
  balanceId: string
  warehouseId: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  baseUnit: string
  reasonType: "COUNT_GAIN" | "COUNT_LOSS" | "DAMAGE" | "OTHER"
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  note: string
  occurredAt: string
  status:
    | "DRAFT"
    | "PENDING_WAREHOUSE_REVIEW"
    | "PENDING_FINANCE"
    | "POSTED"
    | "REJECTED"
  statusLabel: string
  statusTone: "neutral" | "warning" | "info" | "success" | "destructive"
  operatorLabel: string
  createdAt: string
  balanceLockVersion: number
  editVersion: number
}

export type W10AdjustmentOutcome = {
  kind: "SUBMITTED_FOR_WAREHOUSE_REVIEW"
  stockAdjustmentId: string
  adjustmentNo: string
  nextResponsible: string
  reference: string
  submittedAt: string
  balanceLockVersion: number
}

type W10IdempotencyEntry =
  | { status: "pending" }
  | { status: "succeeded"; outcome: W10AdjustmentOutcome }
  | { status: "failed"; code: string; message: string }

const w10AdjustmentDrafts = new Map<string, W10AdjustmentDraft>()
const w10SubmittedAdjustments = new Map<string, W10AdjustmentDraft>()
const w10Idempotency = new Map<string, W10IdempotencyEntry>()
const w10InFlight = new Set<string>()
/** balanceId → lockVersion bump after concurrent demo conflict */
const w10BalanceLockOverrides = new Map<string, number>()
let w10PermissionRevoked = false
let w10AdjustmentSeq = 100

export function isInventoryPermissionRevoked(): boolean {
  return w10PermissionRevoked
}

export function revokeInventoryPermission(): void {
  w10PermissionRevoked = true
}

export function restoreInventoryPermission(): void {
  w10PermissionRevoked = false
}

export function getInventoryBalanceLockVersion(
  balanceId: string,
  seedVersion: number
): number {
  return w10BalanceLockOverrides.get(balanceId) ?? seedVersion
}

/** Demo: simulate concurrent balance change without silent overwrite. */
export function bumpInventoryBalanceLock(balanceId: string, seedVersion: number): number {
  const current = getInventoryBalanceLockVersion(balanceId, seedVersion)
  const next = current + 1
  w10BalanceLockOverrides.set(balanceId, next)
  return next
}

export function listW10SessionAdjustments(): readonly W10AdjustmentDraft[] {
  return [
    ...w10SubmittedAdjustments.values(),
    ...w10AdjustmentDrafts.values(),
  ]
}

export function getW10AdjustmentDraft(
  stockAdjustmentId: string
): W10AdjustmentDraft | null {
  return (
    w10AdjustmentDrafts.get(stockAdjustmentId) ??
    w10SubmittedAdjustments.get(stockAdjustmentId) ??
    null
  )
}

export function createW10AdjustmentDraft(input: {
  balanceId: string
  warehouseId: string
  warehouseName: string
  skuId: string
  skuCode: string
  skuName: string
  baseUnit: string
  balanceLockVersion: number
  operatorLabel?: string
}): W10AdjustmentDraft {
  if (w10PermissionRevoked) {
    throw new WorkItemMockError(
      "PERMISSION_REVOKED",
      "库存调整权限已收回，无法创建草稿。"
    )
  }
  const stockAdjustmentId = `adj_sess_${++w10AdjustmentSeq}`
  const adjustmentNo = `TZ-DRAFT-${String(w10AdjustmentSeq).padStart(4, "0")}`
  const draft: W10AdjustmentDraft = {
    stockAdjustmentId,
    adjustmentNo,
    balanceId: input.balanceId,
    warehouseId: input.warehouseId,
    warehouseName: input.warehouseName,
    skuId: input.skuId,
    skuCode: input.skuCode,
    skuName: input.skuName,
    baseUnit: input.baseUnit,
    reasonType: "COUNT_LOSS",
    reasonTypeLabel: "盘亏",
    direction: "decrease",
    quantity: "",
    note: "",
    occurredAt: new Date().toISOString().slice(0, 16),
    status: "DRAFT",
    statusLabel: "草稿",
    statusTone: "neutral",
    operatorLabel: input.operatorLabel ?? "仓储·当前用户",
    createdAt: new Date().toISOString(),
    balanceLockVersion: input.balanceLockVersion,
    editVersion: 1,
  }
  w10AdjustmentDrafts.set(stockAdjustmentId, draft)
  return { ...draft }
}

export function saveW10AdjustmentDraft(input: {
  stockAdjustmentId: string
  expectedEditVersion: number
  reasonType: W10AdjustmentDraft["reasonType"]
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  note: string
  occurredAt: string
}): W10AdjustmentDraft {
  const draft = w10AdjustmentDrafts.get(input.stockAdjustmentId)
  if (!draft) {
    throw new WorkItemMockError("NOT_FOUND", "调整草稿不存在或已提交。")
  }
  if (draft.editVersion !== input.expectedEditVersion) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "草稿版本冲突，请重载后再保存。"
    )
  }
  const next: W10AdjustmentDraft = {
    ...draft,
    reasonType: input.reasonType,
    reasonTypeLabel: input.reasonTypeLabel,
    direction: input.direction,
    quantity: input.quantity,
    note: input.note,
    occurredAt: input.occurredAt,
    editVersion: draft.editVersion + 1,
  }
  w10AdjustmentDrafts.set(input.stockAdjustmentId, next)
  return { ...next }
}

export function submitW10Adjustment(input: {
  stockAdjustmentId: string
  expectedBalanceLockVersion: number
  seedBalanceLockVersion: number
  reasonType: W10AdjustmentDraft["reasonType"]
  reasonTypeLabel: string
  direction: "increase" | "decrease"
  quantity: string
  note: string
  occurredAt: string
  idempotencyKey: string
  forceUnknown?: boolean
  /** operator cannot review own submission — segregation marker */
  operatorUserId?: string
}):
  | { status: "succeeded"; outcome: W10AdjustmentOutcome }
  | { status: "failed"; code: string; message: string; latestLockVersion?: number }
  | { status: "unknown"; message: string; idempotencyKey: string } {
  const cached = w10Idempotency.get(input.idempotencyKey)
  if (cached?.status === "succeeded") {
    w10InFlight.delete(input.idempotencyKey)
    return { status: "succeeded", outcome: cached.outcome }
  }
  if (cached?.status === "failed") {
    return {
      status: "failed",
      code: cached.code,
      message: cached.message,
    }
  }

  if (input.forceUnknown || w10InFlight.has(input.idempotencyKey)) {
    w10InFlight.add(input.idempotencyKey)
    w10Idempotency.set(input.idempotencyKey, { status: "pending" })
    return {
      status: "unknown",
      message:
        "处理结果尚未确定。请勿假定余额已变化，停留当前调整并按原任务号查询。",
      idempotencyKey: input.idempotencyKey,
    }
  }

  if (w10PermissionRevoked) {
    const failed = {
      status: "failed" as const,
      code: "PERMISSION_REVOKED",
      message: "提交时权限已收回，余额与草稿均未变更。",
    }
    w10Idempotency.set(input.idempotencyKey, failed)
    return failed
  }

  const draft = w10AdjustmentDrafts.get(input.stockAdjustmentId)
  if (!draft) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      message: "调整草稿不存在。",
    }
  }

  const currentLock = getInventoryBalanceLockVersion(
    draft.balanceId,
    input.seedBalanceLockVersion
  )
  if (currentLock !== input.expectedBalanceLockVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      message: `余额已变化（服务端 lockVersion=${currentLock}），未静默覆盖。请刷新基线后重新校验再提交。`,
      latestLockVersion: currentLock,
    }
  }

  const qty = Number(input.quantity)
  if (!Number.isFinite(qty) || qty <= 0) {
    return {
      status: "failed",
      code: "VALIDATION",
      message: "调整数量必须为正数。",
    }
  }

  // 岗位分离：经办提交后进入待仓储复核，不可自复核过账
  const adjustmentNo = `TZ-${new Date().toISOString().slice(0, 10).replaceAll("-", "")}-${String(w10AdjustmentSeq).padStart(3, "0")}`
  const submitted: W10AdjustmentDraft = {
    ...draft,
    reasonType: input.reasonType,
    reasonTypeLabel: input.reasonTypeLabel,
    direction: input.direction,
    quantity: input.quantity,
    note: input.note,
    occurredAt: input.occurredAt,
    adjustmentNo,
    status: "PENDING_WAREHOUSE_REVIEW",
    statusLabel: "待仓储复核",
    statusTone: "warning",
    editVersion: draft.editVersion + 1,
  }
  w10AdjustmentDrafts.delete(input.stockAdjustmentId)
  w10SubmittedAdjustments.set(input.stockAdjustmentId, submitted)

  const outcome: W10AdjustmentOutcome = {
    kind: "SUBMITTED_FOR_WAREHOUSE_REVIEW",
    stockAdjustmentId: input.stockAdjustmentId,
    adjustmentNo,
    nextResponsible: "仓储复核（非经办本人）",
    reference: `ADJ-SUB-${adjustmentNo}`,
    submittedAt: new Date().toISOString(),
    balanceLockVersion: currentLock,
  }
  w10Idempotency.set(input.idempotencyKey, {
    status: "succeeded",
    outcome,
  })
  return { status: "succeeded", outcome }
}

export function resolveW10AdjustmentUnknown(input: {
  idempotencyKey: string
  settle?: boolean
  settlePayload?: Parameters<typeof submitW10Adjustment>[0]
}):
  | { status: "succeeded"; outcome: W10AdjustmentOutcome }
  | { status: "failed"; code: string; message: string; latestLockVersion?: number }
  | { status: "unknown"; message: string; idempotencyKey: string } {
  const cached = w10Idempotency.get(input.idempotencyKey)
  if (cached?.status === "succeeded") {
    w10InFlight.delete(input.idempotencyKey)
    return { status: "succeeded", outcome: cached.outcome }
  }
  if (cached?.status === "failed") {
    w10InFlight.delete(input.idempotencyKey)
    return {
      status: "failed",
      code: cached.code,
      message: cached.message,
    }
  }
  if (input.settle && input.settlePayload) {
    w10InFlight.delete(input.idempotencyKey)
    w10Idempotency.delete(input.idempotencyKey)
    return submitW10Adjustment({
      ...input.settlePayload,
      forceUnknown: false,
      idempotencyKey: input.idempotencyKey,
    })
  }
  if (w10InFlight.has(input.idempotencyKey)) {
    return {
      status: "unknown",
      message: "仍在处理中，处理结果待确认。余额未本地修改。",
      idempotencyKey: input.idempotencyKey,
    }
  }
  return {
    status: "failed",
    code: "NO_PENDING",
    message: "未找到该任务号对应的处理中请求",
  }
}

export type W10ExportJob = {
  jobId: string
  status: "queued" | "running" | "succeeded" | "failed"
  total: number
  completed: number
  filterSummary: string
  createdAt: string
  downloadLabel?: string
}

const w10ExportJobs = new Map<string, W10ExportJob>()
let w10ExportSeq = 0

export function createW10ExportJob(input: {
  total: number
  filterSummary: string
}): W10ExportJob {
  const jobId = `exp-w10-${++w10ExportSeq}`
  const job: W10ExportJob = {
    jobId,
    status: "queued",
    total: input.total,
    completed: 0,
    filterSummary: input.filterSummary,
    createdAt: new Date().toISOString(),
  }
  w10ExportJobs.set(jobId, job)
  // Advance demo job asynchronously within session
  globalThis.setTimeout(() => {
    const current = w10ExportJobs.get(jobId)
    if (!current) return
    w10ExportJobs.set(jobId, {
      ...current,
      status: "running",
      completed: Math.ceil(current.total / 2),
    })
  }, 400)
  globalThis.setTimeout(() => {
    const current = w10ExportJobs.get(jobId)
    if (!current) return
    w10ExportJobs.set(jobId, {
      ...current,
      status: "succeeded",
      completed: current.total,
      downloadLabel: `库存台账导出-${jobId}.csv`,
    })
  }, 1200)
  return job
}

export function getW10ExportJob(jobId: string): W10ExportJob | null {
  return w10ExportJobs.get(jobId) ?? null
}

// ─── W04 contract session mock ──────────────────────────────────────────────


const w04CreatedContracts = new Map<string, ContractListRow>()
const w04CenterOverrides = new Map<string, ContractCenterView>()
const w04UploadIdempotency = new Map<string, UploadContractPdfResult>()
const w04ExportJobs = new Map<string, ContractExportJob>()

export function listW04Contracts(): ContractListRow[] {
  const base = [...MOCK_CONTRACT_LIST]
  for (const created of w04CreatedContracts.values()) {
    if (!base.some((r) => r.contractId === created.contractId)) {
      base.unshift(created)
    }
  }
  return base.map((row) => {
    const override = w04CenterOverrides.get(row.contractId)
    if (!override) return row
    return {
      ...row,
      status: override.status,
      statusLabel: override.statusLabel,
      statusTone: override.statusTone,
      revisionNo: override.currentRevision.revisionNo,
      allowedActions: override.allowedActions,
      actionBlockers: override.actionBlockers,
    }
  })
}

export function getW04ContractCenter(
  contractId: string
): ContractCenterView | null {
  const override = w04CenterOverrides.get(contractId)
  if (override) {
    return { ...override, queriedAt: new Date().toISOString() }
  }
  const base = MOCK_CONTRACT_CENTERS[contractId]
  if (!base) return null
  return { ...base, queriedAt: new Date().toISOString() }
}

/** 上传签署合同 PDF 并归档为可引用的首个不可变版本。 */
export function uploadW04ContractPdf(
  input: UploadContractPdfInput
): UploadContractPdfResult {
  const cached = w04UploadIdempotency.get(input.idempotencyKey)
  if (cached) return cached

  const contractNo = input.contractNo.trim()
  if (
    listW04Contracts().some(
      (row) => row.contractNo.toLocaleLowerCase() === contractNo.toLocaleLowerCase()
    )
  ) {
    throw new Error("CONTRACT_NO_EXISTS")
  }
  if (input.validTo < input.validFrom) {
    throw new Error("CONTRACT_VALIDITY_INVALID")
  }

  const stamp = Date.now().toString(36).toUpperCase()
  const contractId = `ct_upload_${stamp.toLowerCase()}`
  const revisionId = `${contractId}_v1`
  const uploadedAt = new Date().toISOString()
  const knownCustomer = MOCK_CONTRACT_LIST.find(
    (row) => row.customer.customerId === input.customerId
  )?.customer
  const customerId = input.customerId || `cu_upload_${stamp.toLowerCase()}`
  const customerNo = knownCustomer?.customerNo ?? `KH-U${stamp.slice(-4)}`
  const settlementPartyId = `sp_upload_${stamp.toLowerCase()}`
  const result: UploadContractPdfResult = {
    contractId,
    contractNo,
    revisionId,
    revisionNo: 1,
    uploadedAt,
    fileName: input.pdfFile.name,
    reference: `CT-UPLOAD-${stamp}`,
  }

  const row: ContractListRow = {
    contractId,
    contractNo,
    customer: {
      customerId,
      customerNo,
      displayName: input.customerName.trim(),
    },
    settlementParty: {
      partyId: settlementPartyId,
      displayName: input.settlementPartyName.trim(),
    },
    status: "EFFECTIVE",
    statusLabel: CONTRACT_STATUS_LABEL.EFFECTIVE,
    statusTone: CONTRACT_STATUS_TONE.EFFECTIVE,
    revisionNo: 1,
    signedAt: input.signedAt,
    validFrom: input.validFrom,
    validTo: input.validTo,
    expiringWithin30Days:
      new Date(input.validTo).getTime() - Date.now() <= 30 * 24 * 60 * 60 * 1000,
    salesOrderCount: 0,
    activeSalesOrderCount: 0,
    ownerLabel: "当前用户 · 当前客户负责人",
    ownerKind: "current_customer_owner",
    allowedActions: ["PRINT", "CREATE_SALES_ORDER", "TERMINATE"],
    actionBlockers: [],
  }
  w04CreatedContracts.set(contractId, row)
  const center: ContractCenterView = {
    contractId,
    contractNo,
    status: "EFFECTIVE",
    statusLabel: CONTRACT_STATUS_LABEL.EFFECTIVE,
    statusTone: CONTRACT_STATUS_TONE.EFFECTIVE,
    lockVersion: 1,
    customer: {
      id: customerId,
      displayName: input.customerName.trim(),
      reference: customerNo,
    },
    ownerLabel: row.ownerLabel,
    ownerKind: row.ownerKind,
    currentRevision: {
      revisionId,
      revisionNo: 1,
      settlementParty: {
        id: settlementPartyId,
        displayName: input.settlementPartyName.trim(),
      },
      paymentTermSnapshot: {
        label: input.paymentTerms.trim(),
        description: "随签署合同 PDF 归档的付款条件摘要。",
      },
      invoiceRequirementSnapshot: {
        titleType: "合同约定",
        contentSummary: "完整开票约定以当前合同 PDF 为准。",
      },
      validFrom: input.validFrom,
      validTo: input.validTo,
      signedAt: input.signedAt,
      effectiveAt: uploadedAt.slice(0, 16).replace("T", " "),
      termsSummary: "签署合同 PDF 已归档；销售单引用时固定本版本。",
    },
    attachments: [
      {
        id: `fa_${stamp.toLowerCase()}`,
        name: input.pdfFile.name,
        contentType: "application/pdf",
        revisionNo: 1,
        uploadedBy: "当前用户",
        uploadedAt: uploadedAt.slice(0, 16).replace("T", " "),
        securityState: "done",
        canDownload: true,
      },
    ],
    relatedSalesOrders: [],
    revisionTimeline: [
      {
        revisionId,
        revisionNo: 1,
        validFrom: input.validFrom,
        validTo: input.validTo,
        changeReason: "上传签署合同 PDF",
        effectiveAt: uploadedAt.slice(0, 16).replace("T", " "),
        isCurrent: true,
      },
    ],
    auditTimeline: [
      {
        id: `au_upload_${stamp}`,
        action: "UPLOAD_CONTRACT_PDF",
        actorLabel: "当前用户",
        at: uploadedAt.slice(0, 16).replace("T", " "),
        summary: `上传并归档合同 PDF v1：${input.pdfFile.name}`,
      },
    ],
    allowedActions: row.allowedActions,
    actionBlockers: [],
    sourceAsOf: uploadedAt,
    relatedSalesOrdersAsOf: uploadedAt,
    queriedAt: uploadedAt,
    selectableForNewSalesOrder: true,
  }
  w04CenterOverrides.set(contractId, center)
  w04UploadIdempotency.set(input.idempotencyKey, result)
  return result
}

/** 把新建销售单关联回合同档案；重复调用不重复计数。 */
export function linkW04SalesOrder(input: {
  contractId: string
  salesOrderId: string
  documentNumber: string
  natureLabel: string
  contractRevisionNo: number
  statusLabel: string
  statusTone: ContractCenterView["statusTone"]
  amountGross: string
}) {
  const center = getW04ContractCenter(input.contractId)
  if (!center) return
  if (
    center.relatedSalesOrders.some(
      (order) => order.salesOrderId === input.salesOrderId
    )
  ) {
    return
  }

  const now = new Date().toISOString()
  w04CenterOverrides.set(input.contractId, {
    ...center,
    relatedSalesOrders: [
      {
        salesOrderId: input.salesOrderId,
        documentNumber: input.documentNumber,
        natureLabel: input.natureLabel,
        contractRevisionNo: input.contractRevisionNo,
        primaryStatus: {
          label: input.statusLabel,
          tone: input.statusTone,
        },
        amountGross: input.amountGross,
        fulfillmentLabel: "未开始",
        collectionLabel: "未收",
        invoicingLabel: "未开",
      },
      ...center.relatedSalesOrders,
    ],
    relatedSalesOrdersAsOf: now,
    queriedAt: now,
  })

  const created = w04CreatedContracts.get(input.contractId)
  if (created) {
    w04CreatedContracts.set(input.contractId, {
      ...created,
      salesOrderCount: created.salesOrderCount + 1,
      activeSalesOrderCount: created.activeSalesOrderCount + 1,
    })
  }
}

export function createW04ExportJob(input: {
  rowCount: number
  permissionVersion: string
  filterSnapshotLabel: string
}): ContractExportJob {
  const jobId = `EXP-CT-${Date.now().toString(36).toUpperCase()}`
  const job: ContractExportJob = {
    jobId,
    status: "succeeded",
    rowCount: input.rowCount,
    permissionVersion: input.permissionVersion,
    filterSnapshotLabel: input.filterSnapshotLabel,
    createdAt: new Date().toISOString(),
    downloadLabel: `合同导出_${jobId}.csv（服务端筛选结果·重新鉴权）`,
  }
  w04ExportJobs.set(jobId, job)
  return job
}

export function getW04ExportJob(jobId: string) {
  return w04ExportJobs.get(jobId) ?? null
}

// ─── W09 fulfillment operations session mock ───────────────────────────────

const fulfillmentDrafts = new Map<
  string,
  { draft: unknown; editVersion: number }
>()
const fulfillmentBusinessOutcomes = new Map<string, unknown>()

export function getFulfillmentDraft(workItemId: string) {
  return fulfillmentDrafts.get(workItemId) ?? null
}

export function saveFulfillmentDraft(
  workItemId: string,
  draft: unknown,
  expectedEditVersion: number
): { editVersion: number } {
  const current = fulfillmentDrafts.get(workItemId)
  if (current && current.editVersion !== expectedEditVersion) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "履约草稿编辑版本冲突，请重载后再保存。"
    )
  }
  const base = current?.editVersion ?? expectedEditVersion
  const nextVersion = base + 1
  fulfillmentDrafts.set(workItemId, {
    draft: structuredClone(draft),
    editVersion: nextVersion,
  })
  return { editVersion: nextVersion }
}

export function getFulfillmentBusinessOutcome(workItemId: string) {
  return fulfillmentBusinessOutcomes.get(workItemId) ?? null
}

export function setFulfillmentBusinessOutcome(
  workItemId: string,
  outcome: unknown
): void {
  fulfillmentBusinessOutcomes.set(workItemId, outcome)
}

export function isFulfillmentWorkItemTerminal(workItemId: string): boolean {
  return workItemTerminal.has(workItemId)
}

export function isFulfillmentWorkItemHeld(workItemId: string): boolean {
  return workItemHeld.has(workItemId)
}

// ─── W08 purchase order session mock ─────────────────────────────────────────
// draftEditToken lives only here (not URL). Review uses simplified claimToken mock.

type W08DraftOverlay = {
  paymentTermCode: string
  paymentTermLabel: string
  lines: PurchaseOrderLineView[]
  lockVersion: number
  draftContentHash: string
}

type W08ObjectOverlay = {
  status: PurchaseOrderStatus
  reviewStatus: PurchaseReviewStatus
  purchaseNo?: string
  draftLabel?: string
  revisionNo?: number
  lockVersion: number
  currentSubmissionId?: string
  currentRevisionId?: string
  subjectHash?: string
  submittedBy?: string
  submittedAt?: string
  contentSource: "DRAFT" | "SUBMISSION" | "REVISION"
  lines?: PurchaseOrderLineView[]
  paymentTermCode?: string
  paymentTermLabel?: string
  payable?: {
    payableOpenAmount: string
    paidAllocatedAmount: string
    purchaseInvoiceAllocatedAmount: string
  }
  paymentProgress?: string
  invoiceProgress?: string
  fulfillmentProgress?: string
  prepayment?: SeedPO["prepayment"]
  paymentGate?: SeedPO["paymentGate"]
  workflowExtra?: SeedPO["workflow"]
  changesExtra?: SeedPO["changes"]
  reviewWorkItem?: SeedPO["reviewWorkItem"] | null
  allowedActions?: string[]
  actionBlockers?: SeedPO["actionBlockers"]
  activeChangeId?: string
}

const w08Drafts = new Map<string, W08DraftOverlay>()
const w08Objects = new Map<string, W08ObjectOverlay>()
const w08Created = new Map<string, SeedPO>()
const w08ConsumedBases = new Set<string>(
  MOCK_CREATION_BASES.filter((b) => b.consumed).map((b) => b.basisId)
)
const w08DraftTokens = new Map<
  string,
  { token: string; lockVersion: number }
>()
const w08Idempotency = new Map<string, unknown>()
const w08PendingUnknown = new Set<string>()
const w08ChangeWorkcopies = new Map<
  string,
  { changeId: string; baseRevisionNo: number; createdAt: string }
>()

function cloneSeed(seed: SeedPO): SeedPO {
  return {
    ...seed,
    lines: seed.lines.map((l) => ({ ...l })),
    changes: seed.changes.map((c) => ({ ...c })),
    workflow: seed.workflow.map((w) => ({ ...w })),
    actionBlockers: seed.actionBlockers.map((b) => ({ ...b })),
    allowedActions: [...seed.allowedActions],
    prepayment: { ...seed.prepayment },
    fulfillment: { ...seed.fulfillment },
    payable: seed.payable ? { ...seed.payable } : undefined,
    reviewWorkItem: seed.reviewWorkItem
      ? { ...seed.reviewWorkItem }
      : undefined,
  }
}

function resolveSeed(purchaseOrderId: string): SeedPO | null {
  const created = w08Created.get(purchaseOrderId)
  if (created) return cloneSeed(created)
  const base = MOCK_PURCHASE_ORDER_SEEDS.find(
    (s) => s.purchaseOrderId === purchaseOrderId
  )
  return base ? cloneSeed(base) : null
}

function recomputeLine(
  line: PurchaseOrderLineView,
  patch: {
    quantity?: string
    unitCostGross?: string
    inputTaxRate: string
    logisticsFeeReason?: string
  }
): PurchaseOrderLineView {
  const rate = Number(patch.inputTaxRate)
  const unit = Number(patch.unitCostGross ?? line.unitCostGross)
  if (line.lineType === "LOGISTICS_FEE") {
    const gross = Math.round(unit * 100) / 100
    const net = Math.round((gross / (1 + rate)) * 100) / 100
    const tax = Math.round((gross - net) * 100) / 100
    return {
      ...line,
      unitCostGross: unit.toFixed(2),
      inputTaxRate: patch.inputTaxRate,
      logisticsFeeReason: patch.logisticsFeeReason ?? line.logisticsFeeReason,
      grossAmount: gross.toFixed(2),
      netAmount: net.toFixed(2),
      taxAmount: tax.toFixed(2),
    }
  }
  const qty = Number(patch.quantity ?? line.quantity ?? "0")
  const gross = Math.round(qty * unit * 100) / 100
  const net = Math.round((gross / (1 + rate)) * 100) / 100
  const tax = Math.round((gross - net) * 100) / 100
  return {
    ...line,
    quantity: qty.toFixed(0),
    unitCostGross: unit.toFixed(4),
    inputTaxRate: patch.inputTaxRate,
    grossAmount: gross.toFixed(2),
    netAmount: net.toFixed(2),
    taxAmount: tax.toFixed(2),
  }
}

function applyCostMask(
  view: PurchaseOrderCenterView,
  role: ViewerRole
): PurchaseOrderCenterView {
  if (role !== "sales" && role !== "warehouse") return view
  return {
    ...view,
    currentContent: {
      ...view.currentContent,
      costMasked: true,
      totals: { gross: "•••", net: "•••", tax: "•••" },
      lines: view.currentContent.lines.map((l) => ({
        ...l,
        unitCostGross: "•••",
        grossAmount: "•••",
        netAmount: "•••",
        taxAmount: "•••",
      })),
    },
    payableSummary: view.payableSummary
      ? {
          payableOpenAmount: "•••",
          paidAllocatedAmount: "•••",
          purchaseInvoiceAllocatedAmount:
            role === "warehouse"
              ? "•••"
              : view.payableSummary.purchaseInvoiceAllocatedAmount,
        }
      : undefined,
    fieldVisibility: {
      grossAmount: "masked",
      netAmount: "masked",
      taxAmount: "masked",
      unitCostGross: "masked",
      supplierAccount: role === "warehouse" ? "hidden" : "masked",
    },
  }
}

function applyCostMaskList(
  item: PurchaseOrderListItem,
  role: ViewerRole
): PurchaseOrderListItem {
  if (role !== "sales" && role !== "warehouse") return item
  return {
    ...item,
    costMasked: true,
    grossAmount: "•••",
    netAmount: "•••",
    taxAmount: "•••",
  }
}

export function listW08PurchaseOrders(
  role: ViewerRole = "procurement"
): PurchaseOrderListItem[] {
  const ids = new Set<string>([
    ...MOCK_PURCHASE_ORDER_SEEDS.map((s) => s.purchaseOrderId),
    ...w08Created.keys(),
  ])
  const rows: PurchaseOrderListItem[] = []
  for (const id of ids) {
    const center = getW08PurchaseOrderCenter(id, role)
    if (!center) continue
    rows.push(
      applyCostMaskList(
        {
          purchaseOrderId: center.identity.purchaseOrderId,
          purchaseNo: center.identity.purchaseNo,
          draftLabel: center.identity.draftLabel,
          revisionNo: center.identity.revisionNo,
          status: center.identity.status,
          statusLabel: center.identity.statusLabel,
          statusTone: center.identity.statusTone,
          reviewStatus: center.identity.reviewStatus,
          reviewLabel: center.identity.reviewLabel,
          salesOrderId: center.header.salesOrderId,
          salesOrderNo: center.header.salesOrderNo,
          supplierId: center.header.supplierId,
          supplierName: center.header.supplierSnapshot,
          purchaseType: center.header.purchaseType,
          fulfillmentResponsibility: center.header.fulfillmentResponsibility,
          paymentTermCode: center.header.paymentTermCode,
          paymentTermLabel: center.header.paymentTermLabel,
          ownerName: center.header.ownerName,
          grossAmount: center.currentContent.totals.gross,
          netAmount: center.currentContent.totals.net,
          taxAmount: center.currentContent.totals.tax,
          costMasked: center.currentContent.costMasked,
          paymentProgress: center.progress.payment,
          invoiceProgress: center.progress.invoice,
          fulfillmentProgress: center.progress.fulfillment,
          paymentGate: center.progress.prepaymentGate.state,
          expectedDate: center.header.expectedDate,
          updatedAt:
            center.workflow[0]?.at ??
            new Date().toISOString().slice(0, 16).replace("T", " "),
          allowedActions: center.allowedActions,
          actionBlockers: center.actionBlockers,
        },
        role
      )
    )
  }
  rows.sort((a, b) => b.updatedAt.localeCompare(a.updatedAt))
  return rows
}

export function getW08PurchaseOrderCenter(
  purchaseOrderId: string,
  role: ViewerRole = "procurement"
): PurchaseOrderCenterView | null {
  const seed = resolveSeed(purchaseOrderId)
  if (!seed) return null

  const overlay = w08Objects.get(purchaseOrderId)
  const draft = w08Drafts.get(purchaseOrderId)
  let merged: SeedPO = { ...seed }

  if (overlay) {
    merged = {
      ...merged,
      status: overlay.status,
      reviewStatus: overlay.reviewStatus,
      purchaseNo: overlay.purchaseNo ?? merged.purchaseNo,
      draftLabel: overlay.draftLabel ?? merged.draftLabel,
      revisionNo: overlay.revisionNo ?? merged.revisionNo,
      lockVersion: overlay.lockVersion,
      currentSubmissionId:
        overlay.currentSubmissionId ?? merged.currentSubmissionId,
      currentRevisionId:
        overlay.currentRevisionId ?? merged.currentRevisionId,
      subjectHash: overlay.subjectHash ?? merged.subjectHash,
      submittedBy: overlay.submittedBy ?? merged.submittedBy,
      submittedAt: overlay.submittedAt ?? merged.submittedAt,
      contentSource: overlay.contentSource,
      paymentTermCode: overlay.paymentTermCode ?? merged.paymentTermCode,
      paymentTermLabel: overlay.paymentTermLabel ?? merged.paymentTermLabel,
      payable: overlay.payable ?? merged.payable,
      paymentProgress: overlay.paymentProgress ?? merged.paymentProgress,
      invoiceProgress: overlay.invoiceProgress ?? merged.invoiceProgress,
      fulfillmentProgress:
        overlay.fulfillmentProgress ?? merged.fulfillmentProgress,
      prepayment: overlay.prepayment ?? merged.prepayment,
      paymentGate: overlay.paymentGate ?? merged.paymentGate,
      workflow: [
        ...(overlay.workflowExtra ?? []),
        ...merged.workflow,
      ],
      changes: [
        ...(overlay.changesExtra ?? []),
        ...merged.changes,
      ],
      reviewWorkItem:
        overlay.reviewWorkItem === null
          ? undefined
          : (overlay.reviewWorkItem ?? merged.reviewWorkItem),
      allowedActions: overlay.allowedActions ?? merged.allowedActions,
      actionBlockers: overlay.actionBlockers ?? merged.actionBlockers,
    }
    if (overlay.lines) {
      // lines stored as PurchaseOrderLineView — map back minimally for toCenter via draft path
    }
  }

  let center = toCenter(merged)

  if (draft && (merged.status === "DRAFT" || overlay?.status === "DRAFT")) {
    const totals = sumLines(draft.lines)
    center = {
      ...center,
      identity: {
        ...center.identity,
        lockVersion: draft.lockVersion,
        status: "DRAFT",
        statusLabel: PO_STATUS_LABEL.DRAFT,
        statusTone: PO_STATUS_TONE.DRAFT,
      },
      header: {
        ...center.header,
        paymentTermCode: draft.paymentTermCode,
        paymentTermLabel: draft.paymentTermLabel,
      },
      currentContent: {
        source: "DRAFT",
        version: draft.lockVersion,
        subjectHash: undefined,
        lines: draft.lines,
        totals,
        costMasked: false,
      },
      allocations: draft.lines
        .filter((l) => l.lineType === "ITEM_SERVICE")
        .map((l) => ({
          lineId: l.lineId,
          salesOrderLineLabel: l.salesAllocationLabel ?? l.itemName,
          allocatedQuantity: l.quantity ?? "0",
        })),
    }
  } else if (overlay?.lines) {
    const totals = sumLines(overlay.lines)
    center = {
      ...center,
      currentContent: {
        ...center.currentContent,
        lines: overlay.lines,
        totals,
      },
    }
  }

  const change = w08ChangeWorkcopies.get(purchaseOrderId)
  if (change) {
    center = {
      ...center,
      changes: [
        {
          changeId: change.changeId,
          label: "采购变更工作副本（进行中）",
          statusLabel: "编辑中",
          tone: "warning",
          baseRevisionNo: change.baseRevisionNo,
        },
        ...center.changes,
      ],
      allowedActions: center.allowedActions.filter((a) => a !== "START_CHANGE"),
      actionBlockers: [
        ...center.actionBlockers.filter((b) => b.action !== "START_CHANGE"),
        {
          action: "START_CHANGE",
          code: "CHANGE_IN_PROGRESS",
          message: "已有进行中的采购变更工作副本",
        },
      ],
    }
  }

  return applyCostMask(center, role)
}

export function acquireW08DraftEditToken(purchaseOrderId: string): {
  draftEditToken: string
  lockVersion: number
} {
  const center = getW08PurchaseOrderCenter(purchaseOrderId)
  if (!center) {
    throw new WorkItemMockError("NOT_FOUND", "采购单不存在")
  }
  const editable =
    center.identity.status === "DRAFT" ||
    center.identity.reviewStatus === "REJECTED"
  if (!editable) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "仅草稿或被驳回待修改可进入编辑"
    )
  }
  const token = `det_${purchaseOrderId}_${Math.random().toString(36).slice(2, 10)}`
  w08DraftTokens.set(purchaseOrderId, {
    token,
    lockVersion: center.identity.lockVersion,
  })
  return { draftEditToken: token, lockVersion: center.identity.lockVersion }
}

function requireDraftToken(
  purchaseOrderId: string,
  draftEditToken: string,
  expectedLockVersion: number
) {
  const held = w08DraftTokens.get(purchaseOrderId)
  if (!held || held.token !== draftEditToken) {
    throw new WorkItemMockError(
      "LEASE_LOST",
      "编辑已失效，输入已保留，请重新进入编辑。"
    )
  }
  const center = getW08PurchaseOrderCenter(purchaseOrderId)
  if (!center) throw new WorkItemMockError("NOT_FOUND", "采购单不存在")
  if (center.identity.lockVersion !== expectedLockVersion) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "采购单版本冲突，请刷新后比较差异再保存。"
    )
  }
}

export function saveW08PurchaseOrderDraft(input: {
  purchaseOrderId: string
  expectedLockVersion: number
  draftEditToken: string
  paymentTermCode: string
  paymentTermLabel: string
  linePatches: Array<{
    lineId: string
    lineType: "ITEM_SERVICE" | "LOGISTICS_FEE"
    quantity?: string
    unitCostGross?: string
    inputTaxRate: string
    logisticsFeeReason?: string
  }>
  idempotencyKey: string
  simulateConflict?: boolean
  simulateUnknown?: boolean
}): {
  lockVersion: number
  draftContentHash: string
  lines: PurchaseOrderLineView[]
  totals: { gross: string; net: string; tax: string }
} {
  const existing = w08Idempotency.get(input.idempotencyKey)
  if (existing) return existing as ReturnType<typeof saveW08PurchaseOrderDraft>

  if (w08PendingUnknown.has(input.idempotencyKey)) {
    throw new WorkItemMockError(
      "TIMEOUT",
      "上次保存结果仍不确定，请按原任务号查询。"
    )
  }

  if (input.simulateUnknown) {
    w08PendingUnknown.add(input.idempotencyKey)
    throw new WorkItemMockError(
      "TIMEOUT",
      "网络超时：草稿保存结果未知，输入已保留，请查询最终结果。"
    )
  }

  if (input.simulateConflict) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "采购单版本冲突，服务端已有更新，本地输入已保留。"
    )
  }

  requireDraftToken(
    input.purchaseOrderId,
    input.draftEditToken,
    input.expectedLockVersion
  )

  const center = getW08PurchaseOrderCenter(input.purchaseOrderId)
  if (!center) throw new WorkItemMockError("NOT_FOUND", "采购单不存在")

  const baseLines = center.currentContent.lines
  const nextLines = baseLines.map((line) => {
    const patch = input.linePatches.find((p) => p.lineId === line.lineId)
    if (!patch) return line
    return recomputeLine(line, patch)
  })
  const totals = sumLines(nextLines)
  const lockVersion = input.expectedLockVersion + 1
  const draftContentHash = `dch_${input.purchaseOrderId}_v${lockVersion}`

  w08Drafts.set(input.purchaseOrderId, {
    paymentTermCode: input.paymentTermCode,
    paymentTermLabel: input.paymentTermLabel,
    lines: nextLines,
    lockVersion,
    draftContentHash,
  })

  const prev = w08Objects.get(input.purchaseOrderId)
  w08Objects.set(input.purchaseOrderId, {
    status: "DRAFT",
    reviewStatus: center.identity.reviewStatus === "REJECTED" ? "REJECTED" : "NONE",
    purchaseNo: center.identity.purchaseNo,
    draftLabel: center.identity.draftLabel,
    lockVersion,
    contentSource: "DRAFT",
    paymentTermCode: input.paymentTermCode,
    paymentTermLabel: input.paymentTermLabel,
    lines: nextLines,
    allowedActions: ["EDIT", "SUBMIT", "VOID", "OPEN_CENTER"],
    actionBlockers: [
      {
        action: "REVIEW",
        code: "NOT_SUBMITTED",
        message: "草稿尚未提交，无审核任务",
      },
    ],
    workflowExtra: prev?.workflowExtra,
    changesExtra: prev?.changesExtra,
  })

  w08DraftTokens.set(input.purchaseOrderId, {
    token: input.draftEditToken,
    lockVersion,
  })

  const result = { lockVersion, draftContentHash, lines: nextLines, totals }
  w08Idempotency.set(input.idempotencyKey, result)
  return result
}

export function queryW08IdempotentResult(idempotencyKey: string): unknown | null {
  if (w08PendingUnknown.has(idempotencyKey) && !w08Idempotency.has(idempotencyKey)) {
    // finalize pending save/submit as failed-unknown until retry
    return { pending: true }
  }
  return w08Idempotency.get(idempotencyKey) ?? null
}

export function submitW08PurchaseOrder(input: {
  purchaseOrderId: string
  expectedLockVersion: number
  expectedDraftContentHash: string
  draftEditToken: string
  idempotencyKey: string
  simulateUnknown?: boolean
}): {
  submissionId: string
  submissionNo: string
  subjectHash: string
  workItemId: string
  lockVersion: number
  purchaseNo: string
} {
  const existing = w08Idempotency.get(input.idempotencyKey)
  if (existing) return existing as ReturnType<typeof submitW08PurchaseOrder>

  if (input.simulateUnknown) {
    w08PendingUnknown.add(input.idempotencyKey)
    throw new WorkItemMockError(
      "TIMEOUT",
      "提交结果未知：不得切到待审核态，请按原任务号查询。"
    )
  }

  requireDraftToken(
    input.purchaseOrderId,
    input.draftEditToken,
    input.expectedLockVersion
  )

  const draft = w08Drafts.get(input.purchaseOrderId)
  const center = getW08PurchaseOrderCenter(input.purchaseOrderId)
  if (!center) throw new WorkItemMockError("NOT_FOUND", "采购单不存在")

  if (draft && draft.draftContentHash !== input.expectedDraftContentHash) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "草稿数据版本不匹配，请保存后再提交。"
    )
  }

  const lockVersion = input.expectedLockVersion + 1
  const submissionId = `posub_${input.purchaseOrderId}_v${lockVersion}`
  const subjectHash = `sha256:${input.purchaseOrderId}…s${lockVersion}`
  const workItemId = `wi_po_review_${input.purchaseOrderId}`
  const purchaseNo =
    center.identity.purchaseNo ??
    `CG${new Date().toISOString().slice(0, 10).replaceAll("-", "")}${String(
      Math.floor(Math.random() * 90) + 10
    )}`

  const lines = (draft?.lines ?? center.currentContent.lines).map((line) => ({
    ...line,
  }))
  const totals = sumLines(lines)

  w08Objects.set(input.purchaseOrderId, {
    status: "PENDING_REVIEW",
    reviewStatus: "PENDING",
    purchaseNo,
    draftLabel: undefined,
    lockVersion,
    currentSubmissionId: submissionId,
    subjectHash,
    submittedBy: "当前用户 · 采购",
    submittedAt: new Date().toISOString().slice(0, 16).replace("T", " "),
    contentSource: "SUBMISSION",
    lines,
    paymentTermCode: draft?.paymentTermCode ?? center.header.paymentTermCode,
    paymentTermLabel: draft?.paymentTermLabel ?? center.header.paymentTermLabel,
    paymentProgress: "未付",
    invoiceProgress: "未收",
    fulfillmentProgress: "未开始",
    paymentGate:
      (draft?.paymentTermCode ?? center.header.paymentTermCode).startsWith(
        "PREPAY"
      )
        ? "BLOCKED"
        : "NOT_APPLICABLE",
    prepayment: (draft?.paymentTermCode ?? center.header.paymentTermCode).startsWith(
      "PREPAY"
    )
      ? {
          state: "BLOCKED",
          message: "先款条件未满足，审核通过后仍须有效付款",
          required: totals.gross,
          allocated: "0.00",
          gap: totals.gross,
        }
      : {
          state: "NOT_APPLICABLE",
          message: "后款条件，无先款门禁",
          required: "0.00",
          allocated: "0.00",
          gap: "0.00",
        },
    reviewWorkItem: {
      workItemId,
      subjectHash,
      subjectVersion: `sub:${lockVersion}`,
      submittedBy: "当前用户 · 采购",
    },
    allowedActions: ["OPEN_CENTER", "REVIEW", "PRINT"],
    actionBlockers: [
      {
        action: "EDIT",
        code: "PENDING_REVIEW",
        message: "已提交审核，不可编辑；驳回后可改",
      },
      {
        action: "FULFILL",
        code: "NOT_EFFECTIVE",
        message: "采购单尚未生效",
      },
    ],
    workflowExtra: [
      {
        id: `wf_sub_${lockVersion}`,
        actionLabel: "提交财务审核",
        actorLabel: "当前用户 · 采购",
        at: new Date().toISOString().slice(0, 16).replace("T", " "),
      },
    ],
  })

  w08Drafts.delete(input.purchaseOrderId)
  w08DraftTokens.delete(input.purchaseOrderId)

  const result = {
    submissionId,
    submissionNo: `SUB-${lockVersion}`,
    subjectHash,
    workItemId,
    lockVersion,
    purchaseNo,
  }
  w08Idempotency.set(input.idempotencyKey, result)
  return result
}

export function reviewW08PurchaseOrder(input: {
  purchaseOrderId: string
  submissionId: string
  workItemId: string
  expectedLockVersion: number
  reviewResult: "APPROVED" | "REJECTED"
  reasonCode?: string
  comment?: string
  idempotencyKey: string
  /** 岗位分离：提交人不可自审 */
  actorLabel?: string
  simulateUnknown?: boolean
}): {
  reviewResult: "APPROVED" | "REJECTED"
  revisionId?: string
  revisionNo?: number
  payableOpenAmount?: string
  lockVersion: number
  reference: string
} {
  const existing = w08Idempotency.get(input.idempotencyKey)
  if (existing) return existing as ReturnType<typeof reviewW08PurchaseOrder>

  if (input.simulateUnknown) {
    w08PendingUnknown.add(input.idempotencyKey)
    throw new WorkItemMockError(
      "TIMEOUT",
      "审核结果未知：不得本地生效，请查询最终结果。"
    )
  }

  const center = getW08PurchaseOrderCenter(input.purchaseOrderId)
  if (!center) throw new WorkItemMockError("NOT_FOUND", "采购单不存在")
  if (center.identity.lockVersion !== input.expectedLockVersion) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "采购数据已变更，请刷新当前提交后再审。"
    )
  }
  if (center.identity.currentSubmissionId !== input.submissionId) {
    throw new WorkItemMockError(
      "VERSION_CONFLICT",
      "提交已失效，请打开当前提交。"
    )
  }
  if (center.identity.status !== "PENDING_REVIEW") {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "当前状态不可审核"
    )
  }

  const submittedBy = center.header.submittedBy ?? ""
  const actor = input.actorLabel ?? "财务 · 周敏"
  if (submittedBy && actor.includes(submittedBy.replace(/·.*/, "").trim())) {
    // soft separation: if same person name substring
  }
  if (submittedBy === actor) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "岗位分离：审核人不得为该提交经办人"
    )
  }

  const lockVersion = input.expectedLockVersion + 1
  const reference = `REV-${input.purchaseOrderId}-${lockVersion}`
  const totals = center.currentContent.totals
  const now = new Date().toISOString().slice(0, 16).replace("T", " ")

  if (input.reviewResult === "APPROVED") {
    const revisionId = `porev_${input.purchaseOrderId}_v1`
    const prepay = center.header.paymentTermCode.startsWith("PREPAY")
    w08Objects.set(input.purchaseOrderId, {
      status: "EFFECTIVE",
      reviewStatus: "APPROVED",
      purchaseNo: center.identity.purchaseNo,
      revisionNo: 1,
      lockVersion,
      currentSubmissionId: input.submissionId,
      currentRevisionId: revisionId,
      subjectHash: center.identity.subjectHash,
      submittedBy: center.header.submittedBy,
      submittedAt: center.header.submittedAt,
      contentSource: "REVISION",
      lines: [...center.currentContent.lines],
      paymentTermCode: center.header.paymentTermCode,
      paymentTermLabel: center.header.paymentTermLabel,
      payable: {
        payableOpenAmount: totals.gross,
        paidAllocatedAmount: "0.00",
        purchaseInvoiceAllocatedAmount: "0.00",
      },
      paymentProgress: "未付",
      invoiceProgress: "未收",
      fulfillmentProgress: "未开始",
      paymentGate: prepay ? "BLOCKED" : "NOT_APPLICABLE",
      prepayment: prepay
        ? {
            state: "BLOCKED",
            message: "先款条件未满足，禁止履约",
            required: totals.gross,
            allocated: "0.00",
            gap: totals.gross,
          }
        : {
            state: "NOT_APPLICABLE",
            message: "后款条件，无先款门禁",
            required: "0.00",
            allocated: "0.00",
            gap: "0.00",
          },
      reviewWorkItem: null,
      allowedActions: prepay
        ? ["OPEN_CENTER", "PAY", "START_CHANGE", "PRINT"]
        : ["OPEN_CENTER", "FULFILL", "PAY", "START_CHANGE", "PRINT"],
      actionBlockers: prepay
        ? [
            {
              action: "FULFILL",
              code: "PREPAYMENT_GATE",
              message: "先款门禁未满足，请先完成有效付款核销",
            },
          ]
        : [],
      workflowExtra: [
        {
          id: `wf_appr_${lockVersion}`,
          actionLabel: "财务审核通过",
          actorLabel: actor,
          at: now,
          comment: input.comment,
        },
      ],
    })

    const result = {
      reviewResult: "APPROVED" as const,
      revisionId,
      revisionNo: 1,
      payableOpenAmount: totals.gross,
      lockVersion,
      reference,
    }
    w08Idempotency.set(input.idempotencyKey, result)
    return result
  }

  // REJECTED — restore draft, complete review task, no successor work_item
  const short = Math.random().toString(36).slice(2, 6)
  w08Objects.set(input.purchaseOrderId, {
    status: "DRAFT",
    reviewStatus: "REJECTED",
    purchaseNo: center.identity.purchaseNo,
    draftLabel: `采购草稿 · 驳回重开 · ${short}`,
    lockVersion,
    currentSubmissionId: undefined,
    subjectHash: undefined,
    submittedBy: center.header.submittedBy,
    submittedAt: center.header.submittedAt,
    contentSource: "DRAFT",
    lines: [...center.currentContent.lines],
    paymentTermCode: center.header.paymentTermCode,
    paymentTermLabel: center.header.paymentTermLabel,
    reviewWorkItem: null,
    allowedActions: ["EDIT", "SUBMIT", "OPEN_CENTER"],
    actionBlockers: [
      {
        action: "REVIEW",
        code: "NOT_SUBMITTED",
        message: "需重新提交后才会创建新的审核任务",
      },
    ],
    workflowExtra: [
      {
        id: `wf_rej_${lockVersion}`,
        actionLabel: "财务驳回",
        actorLabel: actor,
        at: now,
        comment:
          input.comment ??
          (input.reasonCode ? `原因：${input.reasonCode}` : undefined),
      },
    ],
  })
  w08Drafts.set(input.purchaseOrderId, {
    paymentTermCode: center.header.paymentTermCode,
    paymentTermLabel: center.header.paymentTermLabel,
    lines: [...center.currentContent.lines],
    lockVersion,
    draftContentHash: `dch_${input.purchaseOrderId}_v${lockVersion}`,
  })

  const result = {
    reviewResult: "REJECTED" as const,
    lockVersion,
    reference,
  }
  w08Idempotency.set(input.idempotencyKey, result)
  return result
}

export function startW08PurchaseChange(input: {
  purchaseOrderId: string
  expectedLockVersion: number
  idempotencyKey: string
}): { changeId: string; baseRevisionNo: number } {
  const existing = w08Idempotency.get(input.idempotencyKey)
  if (existing) return existing as { changeId: string; baseRevisionNo: number }

  const center = getW08PurchaseOrderCenter(input.purchaseOrderId)
  if (!center) throw new WorkItemMockError("NOT_FOUND", "采购单不存在")
  if (
    center.identity.status !== "EFFECTIVE" &&
    center.identity.status !== "PARTIAL"
  ) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "仅已生效采购单可发起变更"
    )
  }
  if (w08ChangeWorkcopies.has(input.purchaseOrderId)) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "已有进行中的采购变更"
    )
  }
  if (center.identity.lockVersion !== input.expectedLockVersion) {
    throw new WorkItemMockError("VERSION_CONFLICT", "版本冲突，请刷新后重试")
  }

  const changeId = `poc_${input.purchaseOrderId}_${Date.now().toString(36)}`
  const baseRevisionNo = center.identity.revisionNo ?? 1
  const payload = { changeId, baseRevisionNo }
  w08ChangeWorkcopies.set(input.purchaseOrderId, {
    ...payload,
    createdAt: new Date().toISOString(),
  })
  w08Idempotency.set(input.idempotencyKey, payload)
  return payload
}

export function listW08CreationBases(): PurchaseCreationBasis[] {
  return MOCK_CREATION_BASES.map((b) => ({
    ...b,
    consumed: w08ConsumedBases.has(b.basisId) || b.consumed,
    lines: b.lines.map((l) => ({ ...l })),
  }))
}

export function createW08FromBasis(input: {
  basisId: string
  idempotencyKey: string
}): {
  purchaseOrderId: string
  draftLabel: string
  lockVersion: number
} {
  const existing = w08Idempotency.get(input.idempotencyKey)
  if (existing) return existing as ReturnType<typeof createW08FromBasis>

  let basis = listW08CreationBases().find((b) => b.basisId === input.basisId)
  // W07 通过后生成的 pcb_{confirmationId}：演示中若未预置种子，按固定拆单键合成可消费依据
  if (!basis && input.basisId.startsWith("pcb_")) {
    if (w08ConsumedBases.has(input.basisId)) {
      throw new WorkItemMockError(
        "ACTION_NOT_ALLOWED",
        "该采购创建依据已被消费，不可重复建单"
      )
    }
    basis = {
      basisId: input.basisId,
      salesOrderId: "so_from_w07",
      salesOrderNo: "XS-FROM-W07",
      salesSubmissionId: "sub_from_w07",
      salesSubmissionNo: 1,
      supplierId: "sup_hd",
      supplierName: "华东优选供应链有限公司",
      purchaseType: "PHYSICAL",
      fulfillmentResponsibility: "WAREHOUSE",
      paymentTermCode: "POSTPAY_NET15",
      paymentTermLabel: "货到 15 天",
      lines: [
        {
          procurementConfirmationLineId: `${input.basisId}_line1`,
          itemName: "W07 确认分行 · 礼包",
          itemSku: "SKU-W07-1",
          quantity: "50",
          unit: "套",
          unitCostGross: "400.0000",
          inputTaxRate: "0.13",
          expectedDeliveryDate: "2026-04-15",
          salesAllocationLabel: "销售行 · 二次确认礼包 ×50",
        },
      ],
      estimatedGross: "20000.00",
      consumed: false,
    }
  }
  if (!basis) {
    throw new WorkItemMockError("NOT_FOUND", "采购创建依据不存在")
  }
  if (w08ConsumedBases.has(basis.basisId) || basis.consumed) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "该采购创建依据已被消费，不可重复建单"
    )
  }
  if (basis.lines.length === 0) {
    throw new WorkItemMockError(
      "ACTION_NOT_ALLOWED",
      "创建依据无可覆盖确认分行"
    )
  }

  // 拆单维度已在依据上固定：1 SO × 1 supplier × 1 type × 1 terms × 1 responsibility
  const purchaseOrderId = `po_new_${Date.now().toString(36)}`
  const short = purchaseOrderId.slice(-4)
  const draftLabel = `采购草稿 · ${short}`
  const lines = buildLines(
    basis.lines.map((l, index) => ({
      lineId: `${purchaseOrderId}_l${index + 1}`,
      lineType: "ITEM_SERVICE" as const,
      procurementConfirmationLineId: l.procurementConfirmationLineId,
      itemName: l.itemName,
      itemSku: l.itemSku,
      quantity: l.quantity,
      unit: l.unit,
      unitCostGross: l.unitCostGross,
      inputTaxRate: l.inputTaxRate,
      expectedDeliveryDate: l.expectedDeliveryDate,
      salesAllocationLabel: l.salesAllocationLabel,
    }))
  )

  const seed: SeedPO = {
    purchaseOrderId,
    draftLabel,
    status: "DRAFT",
    reviewStatus: "NONE",
    salesOrderId: basis.salesOrderId,
    salesOrderNo: basis.salesOrderNo,
    supplierId: basis.supplierId,
    supplierName: basis.supplierName,
    purchaseType: basis.purchaseType,
    fulfillmentResponsibility: basis.fulfillmentResponsibility,
    paymentTermCode: basis.paymentTermCode,
    paymentTermLabel: basis.paymentTermLabel,
    ownerName: "当前用户 · 采购",
    paymentProgress: "—",
    invoiceProgress: "—",
    fulfillmentProgress: "—",
    paymentGate: "NOT_APPLICABLE",
    prepayment: {
      state: "NOT_APPLICABLE",
      message: "草稿阶段无门禁",
      required: "0.00",
      allocated: "0.00",
      gap: "0.00",
    },
    expectedDate: basis.lines[0]?.expectedDeliveryDate,
    updatedAt: new Date().toISOString().slice(0, 16).replace("T", " "),
    lockVersion: 1,
    creationBasisId: basis.basisId,
    contentSource: "DRAFT",
    lines: lines.map((l) => ({
      lineId: l.lineId,
      lineType: l.lineType,
      procurementConfirmationLineId: l.procurementConfirmationLineId,
      itemName: l.itemName,
      itemSku: l.itemSku,
      quantity: l.quantity,
      unit: l.unit,
      unitCostGross: l.unitCostGross,
      inputTaxRate: l.inputTaxRate,
      expectedDeliveryDate: l.expectedDeliveryDate,
      salesAllocationLabel: l.salesAllocationLabel,
    })),
    fulfillment: {
      progressLabel: "未开始",
      progressTone: "neutral",
      inboundQty: "0",
      shippedQty: "0",
      remainingQty: basis.lines
        .reduce((s, l) => s + Number(l.quantity), 0)
        .toFixed(0),
    },
    changes: [],
    workflow: [
      {
        id: `wf_create_${purchaseOrderId}`,
        actionLabel: "消费创建依据建单",
        actorLabel: "当前用户 · 采购",
        at: new Date().toISOString().slice(0, 16).replace("T", " "),
        comment: `依据 ${basis.basisId} · 无采购建单任务`,
      },
    ],
    allowedActions: ["EDIT", "SUBMIT", "VOID", "OPEN_CENTER"],
    actionBlockers: [
      {
        action: "REVIEW",
        code: "NOT_SUBMITTED",
        message: "草稿尚未提交",
      },
    ],
  }

  w08Created.set(purchaseOrderId, seed)
  w08ConsumedBases.add(basis.basisId)
  w08Drafts.set(purchaseOrderId, {
    paymentTermCode: basis.paymentTermCode,
    paymentTermLabel: basis.paymentTermLabel,
    lines,
    lockVersion: 1,
    draftContentHash: `dch_${purchaseOrderId}_v1`,
  })

  const result = { purchaseOrderId, draftLabel, lockVersion: 1 }
  w08Idempotency.set(input.idempotencyKey, result)
  return result
}


// ─── W13 card funds review session mock ─────────────────────────────────────

export type W13FundsOverlay = {
  fundsFactVersion: string
  subjectHash: string
  settledTotal: string
  invoicedTotal: string
  openTotal: string
  openInvoiceableTotal: string
  receiptFacts: Array<{
    receiptId: string
    receiptNo: string
    receivedAt: string
    grossAmount: string
    allocatedToAccount: string
    otherAllocationSummary?: string
    reversed: boolean
  }>
  invoiceFacts: Array<{
    invoiceId: string
    invoiceNo: string
    direction: "BLUE" | "RED"
    issuedAt: string
    grossAmount: string
    netAmount: string
    taxAmount: string
    allocatedToAccount: string
    reversed: boolean
  }>
  evidenceDocumentIds: string[]
  evidenceReferences: string[]
  comment?: string
  /** When true, next complete with stale expected hash will fail (demo toggle) */
  forceHashDriftOnComplete?: boolean
}

export type W13BusinessOutcome = {
  receivableFundsReviewId: string
  receivableAccountId: string
  reviewNo: number
  accountReviewStatus: string
  workflowActionId: string
  operationId: string
  completedAt: string
  reviewResult: "APPROVED" | "REJECTED"
  conclusion: string
  subjectHash: string
  reference: string
  followUpConfiguration?: {
    status: "BLOCKED"
    blockerCode: "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED"
    collaborationMessage: string
    requiredRegistration: Array<"WORK_ITEM_TYPE" | "OWNER_POOL" | "HANDLER_KEY">
  }
}

const w13FundsOverlay = new Map<string, W13FundsOverlay>()
const w13BusinessOutcomes = new Map<string, W13BusinessOutcome>()
const w13ReviewAppend = new Map<
  string,
  Array<{
    reviewId: string
    reviewNo: number
    reviewType: "OPENING" | "SYNC_DELTA"
    reviewResult: "APPROVED" | "REJECTED"
    conclusion: string
    reviewerLabel: string
    completedAt: string
    subjectHashAtReview: string
    predecessorReviewId?: string
    readOnly: true
  }>
>()

export function getW13FundsOverlay(workItemId: string): W13FundsOverlay | null {
  return w13FundsOverlay.get(workItemId) ?? null
}

export function setW13FundsOverlay(
  workItemId: string,
  overlay: W13FundsOverlay
): void {
  w13FundsOverlay.set(workItemId, overlay)
}

export function getW13BusinessOutcome(
  workItemId: string
): W13BusinessOutcome | null {
  return w13BusinessOutcomes.get(workItemId) ?? null
}

export function setW13BusinessOutcome(
  workItemId: string,
  outcome: W13BusinessOutcome
): void {
  w13BusinessOutcomes.set(workItemId, outcome)
}

export function getW13AppendedReviews(workItemId: string) {
  return w13ReviewAppend.get(workItemId) ?? []
}

export function appendW13Review(
  workItemId: string,
  item: {
    reviewId: string
    reviewNo: number
    reviewType: "OPENING" | "SYNC_DELTA"
    reviewResult: "APPROVED" | "REJECTED"
    conclusion: string
    reviewerLabel: string
    completedAt: string
    subjectHashAtReview: string
    predecessorReviewId?: string
  }
): void {
  const list = w13ReviewAppend.get(workItemId) ?? []
  list.push({ ...item, readOnly: true })
  w13ReviewAppend.set(workItemId, list)
}

export function isW13WorkItemTerminal(workItemId: string): boolean {
  return (
    workItemTerminal.has(workItemId) ||
    getCompletedQueueTaskIds("W13").has(workItemId)
  )
}

export function isW13WorkItemHeld(workItemId: string): boolean {
  return workItemHeld.has(workItemId) || getHeldQueueTaskIds("W13").has(workItemId)
}

/** Demo: bump subject hash to simulate external fact change before complete. */
export function bumpW13SubjectHash(workItemId: string, nextHash: string): void {
  const current = workItemSubject.get(workItemId)
  const nextVersion = current
    ? `sv_w13_bump_${Date.now()}`
    : `sv_w13_init_${Date.now()}`
  workItemSubject.set(workItemId, {
    subjectVersion: nextVersion,
    subjectHash: nextHash,
    leaseVersion: current?.leaseVersion ?? 1,
  })
  const lease = workItemLeases.get(workItemId)
  if (lease) {
    workItemLeases.set(workItemId, {
      ...lease,
      subjectHash: nextHash,
      subjectVersion: nextVersion,
    })
  }
  const overlay = w13FundsOverlay.get(workItemId)
  if (overlay) {
    w13FundsOverlay.set(workItemId, {
      ...overlay,
      subjectHash: nextHash,
      forceHashDriftOnComplete: true,
    })
  } else {
    w13FundsOverlay.set(workItemId, {
      fundsFactVersion: `ffv_drift_${Date.now()}`,
      subjectHash: nextHash,
      settledTotal: "0.00",
      invoicedTotal: "0.00",
      openTotal: "0.00",
      openInvoiceableTotal: "0.00",
      receiptFacts: [],
      invoiceFacts: [],
      evidenceDocumentIds: [],
      evidenceReferences: [],
      forceHashDriftOnComplete: true,
    })
  }
}

// ─── W12 supplier payables session mock ──────────────────────────────────────

export class SupplierPayablesMockError extends Error {
  code: string
  constructor(code: string, message: string) {
    super(message)
    this.name = "SupplierPayablesMockError"
    this.code = code
  }
}

function w12Cents(s: string): number {
  const n = Number(s)
  if (!Number.isFinite(n)) return 0
  return Math.round(n * 100)
}

function w12FromCents(c: number): string {
  return (c / 100).toFixed(2)
}

function w12Add(a: string, b: string): string {
  return w12FromCents(w12Cents(a) + w12Cents(b))
}

function w12Sub(a: string, b: string): string {
  return w12FromCents(w12Cents(a) - w12Cents(b))
}

function w12Max0(a: string): string {
  return w12Cents(a) < 0 ? "0.00" : a
}

type W12PayableOverlay = {
  settledTotal?: string
  invoicedTotal?: string
  entryLockVersion?: number
  accountLockVersion?: number
  status?: SeedPayable["status"]
  paymentGate?: SeedPayable["paymentGate"]
}

const w12PayableOverlays = new Map<string, W12PayableOverlay>()
const w12SessionPayments = new Map<string, SeedPayment>()
const w12SessionInvoices = new Map<string, SeedInvoice>()
const w12PaymentNoSeq = { n: 100 }
const w12InvoiceSeq = { n: 100 }
const w12Idempotency = new Map<string, FormalSubmitResult>()
const w12PendingUnknown = new Map<string, FormalSubmitResult>()
const w12AllocationDrafts = new Map<
  string,
  { track: AllocationTrack; supplierId: string; formSnapshot: Record<string, unknown>; savedAt: string }
>()
/** purchaseOrderId → gate projection for W09 re-query */
const w09PaymentGateByPo = new Map<
  string,
  {
    state: "SATISFIED" | "BLOCKED" | "NOT_APPLICABLE"
    message: string
    effectivePaidAmount: string
    requiredAmount: string
  }
>()
let w12PolicyOverride: PayablePriorityPolicyView | null = null
let w12PermissionRevoked = false
let w12WatermarkSeq = 1

function w12NowIso(): string {
  return new Date().toISOString()
}

function w12BumpWatermark(): string {
  return `w12-wm-${++w12WatermarkSeq}-${Date.now()}`
}

export function isW12PermissionRevoked(): boolean {
  return w12PermissionRevoked
}

export function revokeW12Permission(): void {
  w12PermissionRevoked = true
}

export function restoreW12Permission(): void {
  w12PermissionRevoked = false
}

export function setW12PolicyState(
  state: "AVAILABLE" | "MISSING" | "STALE"
): void {
  if (state === "AVAILABLE") {
    w12PolicyOverride = null
    return
  }
  w12PolicyOverride = {
    state,
    mixedAutoAllocationAllowed: false,
    blockerMessage:
      state === "MISSING"
        ? "应付优先级策略未配置：禁止采购单与结算单混合自动分配，请显式逐项选择目标或按来源类型分组。"
        : "应付优先级策略版本已陈旧：请刷新策略后重试，或显式逐项选择目标。",
  }
}

export function getW12PayablePriorityPolicy(): PayablePriorityPolicyView {
  if (w12PolicyOverride) return w12PolicyOverride
  return {
    payablePriorityPolicyId: W12_DEFAULT_POLICY.payablePriorityPolicyId,
    payablePriorityPolicyVersion: W12_DEFAULT_POLICY.payablePriorityPolicyVersion,
    state: "AVAILABLE",
    mixedAutoAllocationAllowed: true,
  }
}

export function getFulfillmentPaymentGateOverride(purchaseOrderId: string) {
  return w09PaymentGateByPo.get(purchaseOrderId) ?? null
}

function resolvePayable(accountId: string): SeedPayable | null {
  const seed = SEED_PAYABLES.find((p) => p.payableAccountId === accountId)
  if (!seed) return null
  const o = w12PayableOverlays.get(accountId)
  if (!o) return { ...seed, paymentGate: seed.paymentGate ? { ...seed.paymentGate } : undefined }
  return {
    ...seed,
    settledTotal: o.settledTotal ?? seed.settledTotal,
    invoicedTotal: o.invoicedTotal ?? seed.invoicedTotal,
    entryLockVersion: o.entryLockVersion ?? seed.entryLockVersion,
    accountLockVersion: o.accountLockVersion ?? seed.accountLockVersion,
    status: o.status ?? seed.status,
    paymentGate: o.paymentGate
      ? { ...o.paymentGate }
      : seed.paymentGate
        ? { ...seed.paymentGate }
        : undefined,
  }
}

function allPayables(): SeedPayable[] {
  return SEED_PAYABLES.map((p) => resolvePayable(p.payableAccountId)!).filter(
    Boolean
  )
}

function allPayments(): SeedPayment[] {
  const session = [...w12SessionPayments.values()]
  const sessionIds = new Set(session.map((p) => p.paymentId))
  const seed = SEED_PAYMENTS.filter((p) => !sessionIds.has(p.paymentId)).map(
    (p) => ({
      ...p,
      allocations: p.allocations.map((a) => ({ ...a })),
    })
  )
  // merge overlays: session may replace seed ids after reverse
  const seedWithOverrides = seed.map((p) => {
    const ov = w12SessionPayments.get(p.paymentId)
    return ov ?? p
  })
  const onlySession = session.filter(
    (p) => !SEED_PAYMENTS.some((s) => s.paymentId === p.paymentId)
  )
  const seedIds = new Set(SEED_PAYMENTS.map((s) => s.paymentId))
  const mergedSeed = SEED_PAYMENTS.map((p) => {
    const ov = w12SessionPayments.get(p.paymentId)
    if (ov) return { ...ov, allocations: ov.allocations.map((a) => ({ ...a })) }
    return { ...p, allocations: p.allocations.map((a) => ({ ...a })) }
  })
  return [...mergedSeed, ...onlySession.filter((p) => !seedIds.has(p.paymentId))]
}

function allInvoices(): SeedInvoice[] {
  const seedIds = new Set(SEED_INVOICES.map((s) => s.invoiceId))
  const mergedSeed = SEED_INVOICES.map((p) => {
    const ov = w12SessionInvoices.get(p.invoiceId)
    if (ov) return { ...ov, allocations: ov.allocations.map((a) => ({ ...a })) }
    return { ...p, allocations: p.allocations.map((a) => ({ ...a })) }
  })
  const onlySession = [...w12SessionInvoices.values()].filter(
    (p) => !seedIds.has(p.invoiceId)
  )
  return [...mergedSeed, ...onlySession]
}

function dueStateOf(dueDate: string): "not_due" | "due_today" | "overdue" {
  const today = "2026-08-01"
  if (dueDate < today) return "overdue"
  if (dueDate === today) return "due_today"
  return "not_due"
}

function dueLabel(state: ReturnType<typeof dueStateOf>): string {
  if (state === "overdue") return "已到期"
  if (state === "due_today") return "今日到期"
  return "未到期"
}

function derivePayableStatus(open: string, gross: string): SeedPayable["status"] {
  if (w12Cents(open) <= 0) return "SETTLED"
  if (w12Cents(open) < w12Cents(gross)) return "PARTIAL"
  return "OPEN"
}

function projectPayableRow(p: SeedPayable): PayableRow {
  const openTotal = w12Max0(w12Sub(p.grossTotal, p.settledTotal))
  const openInvoiceableTotal = w12Max0(w12Sub(p.grossTotal, p.invoicedTotal))
  const status = derivePayableStatus(openTotal, p.grossTotal)
  const ds = dueStateOf(p.dueDate)
  const gate = p.paymentGate
    ? {
        ...p.paymentGate,
        allocated: p.settledTotal,
        gap: gateGap(p.paymentGate.required, p.settledTotal),
        state:
          p.paymentGate.state === "NOT_APPLICABLE"
            ? ("NOT_APPLICABLE" as const)
            : w12Cents(p.settledTotal) >= w12Cents(p.paymentGate.required)
              ? ("SATISFIED" as const)
              : ("BLOCKED" as const),
        message:
          p.paymentGate.state === "NOT_APPLICABLE"
            ? p.paymentGate.message
            : w12Cents(p.settledTotal) >= w12Cents(p.paymentGate.required)
              ? "有效已付净核销已满足先款条件"
              : p.paymentGate.message,
      }
    : undefined

  return {
    payableAccountId: p.payableAccountId,
    supplierId: p.supplierId,
    supplierName: p.supplierName,
    sourceType: p.sourceType,
    sourceTypeLabel: SOURCE_TYPE_LABEL[p.sourceType],
    sourceDocumentId: p.sourceDocumentId,
    sourceDocumentNo: p.sourceDocumentNo,
    sourceHref: sourceHref(p.sourceType, p.sourceDocumentId),
    primaryEntryId: p.primaryEntryId,
    entryLockVersion: p.entryLockVersion,
    accountLockVersion: p.accountLockVersion,
    grossTotal: p.grossTotal,
    settledTotal: p.settledTotal,
    openTotal,
    invoicedTotal: p.invoicedTotal,
    openInvoiceableTotal,
    dueDate: p.dueDate,
    dueState: ds,
    dueStateLabel: dueLabel(ds),
    status,
    statusLabel: PAYABLE_STATUS_LABEL[status],
    statusTone: PAYABLE_STATUS_TONE[status],
    paymentGateSummary: gate,
    allowedActions:
      status === "SETTLED"
        ? (["PREVIEW", "VIEW_SOURCE"] as const)
        : (["PREVIEW", "ALLOCATE_PAYMENT", "ALLOCATE_INVOICE", "VIEW_SOURCE"] as const),
    actionBlockers: [],
  }
}

function gateGap(required: string, allocated: string): string {
  return w12Max0(w12Sub(required, allocated))
}

function paymentAllocatedNet(p: SeedPayment): string {
  let c = 0
  for (const a of p.allocations) {
    if (a.action === "APPLY") c += w12Cents(a.amount)
    else c -= w12Cents(a.amount)
  }
  return w12FromCents(Math.max(0, c))
}

function projectPaymentRow(p: SeedPayment): PaymentRow {
  const allocatedTotal = paymentAllocatedNet(p)
  const unallocatedAmount =
    p.status === "REVERSED"
      ? "0.00"
      : w12Max0(w12Sub(p.amount, allocatedTotal))
  const statusLabel =
    p.status === "POSTED"
      ? "已过账"
      : p.status === "REVERSED"
        ? "已冲正"
        : "草稿"
  const statusTone =
    p.status === "POSTED"
      ? ("success" as const)
      : p.status === "REVERSED"
        ? ("neutral" as const)
        : ("warning" as const)

  const payableById = new Map(allPayables().map((x) => [x.payableAccountId, x]))

  return {
    paymentId: p.paymentId,
    paymentNo: p.paymentNo,
    supplierId: p.supplierId,
    supplierName: p.supplierName,
    paidAt: p.paidAt,
    amount: p.amount,
    bankReferenceMasked: maskBankRef(p.bankReference),
    allocatedTotal,
    unallocatedAmount,
    status: p.status,
    statusLabel,
    statusTone,
    allocations: p.allocations.map((a) => {
      const pa = payableById.get(a.payableAccountId)
      return {
        allocationId: a.allocationId,
        action: a.action,
        payableAccountId: a.payableAccountId,
        payableEntryId: a.payableEntryId,
        sourceType: pa?.sourceType ?? "PURCHASE_ORDER",
        sourceDocumentNo: pa?.sourceDocumentNo ?? a.payableAccountId,
        amount: a.amount,
        occurredAt: a.occurredAt,
        reverseOfAllocationId: a.reverseOfAllocationId,
      }
    }),
    allowedActions:
      p.status === "POSTED"
        ? w12Cents(unallocatedAmount) > 0
          ? (["CONTINUE_ALLOCATE", "REVERSE", "PREVIEW"] as const)
          : (["REVERSE", "PREVIEW"] as const)
        : (["PREVIEW"] as const),
    actionBlockers:
      p.status === "POSTED"
        ? []
        : [
            {
              action: "EDIT",
              code: "POSTED_IMMUTABLE",
              message: "已过账记录不可编辑删除，请通过冲正追加反向记录。",
            },
          ],
    reversedByPaymentId: p.reversedByPaymentId,
    reverseOfPaymentId: p.reverseOfPaymentId,
  }
}

function invoiceAllocatedNet(inv: SeedInvoice): string {
  let c = 0
  for (const a of inv.allocations) {
    if (a.action === "APPLY") c += w12Cents(a.amountGross)
    else c -= w12Cents(a.amountGross)
  }
  return w12FromCents(Math.max(0, c))
}

function projectInvoiceRow(inv: SeedInvoice): PurchaseInvoiceRow {
  const allocatedTotal = invoiceAllocatedNet(inv)
  const unallocatedAmount =
    inv.status === "REVERSED"
      ? "0.00"
      : w12Max0(w12Sub(inv.grossAmount, allocatedTotal))
  const payableById = new Map(allPayables().map((x) => [x.payableAccountId, x]))
  return {
    invoiceId: inv.invoiceId,
    invoiceCode: inv.invoiceCode,
    invoiceNo: inv.invoiceNo,
    invoiceKind: inv.invoiceKind,
    invoiceKindLabel: inv.invoiceKind === "BLUE" ? "蓝票" : "红票",
    supplierId: inv.supplierId,
    supplierName: inv.supplierName,
    invoiceDate: inv.invoiceDate,
    grossAmount: inv.grossAmount,
    netAmount: inv.netAmount,
    taxAmount: inv.taxAmount,
    allocatedTotal,
    unallocatedAmount,
    status: inv.status,
    statusLabel: inv.status === "POSTED" ? "已登记" : "已红冲",
    statusTone: inv.status === "POSTED" ? "success" : "neutral",
    originalInvoiceId: inv.originalInvoiceId,
    allocations: inv.allocations.map((a) => {
      const pa = payableById.get(a.payableAccountId)
      return {
        allocationId: a.allocationId,
        action: a.action,
        payableAccountId: a.payableAccountId,
        sourceType: pa?.sourceType ?? "PURCHASE_ORDER",
        sourceDocumentNo: pa?.sourceDocumentNo ?? a.payableAccountId,
        amountGross: a.amountGross,
        occurredAt: a.occurredAt,
        reverseOfAllocationId: a.reverseOfAllocationId,
      }
    }),
    allowedActions:
      inv.status === "POSTED" && inv.invoiceKind === "BLUE"
        ? w12Cents(unallocatedAmount) > 0
          ? (["CONTINUE_ALLOCATE", "RED_INVOICE", "PREVIEW"] as const)
          : (["RED_INVOICE", "PREVIEW"] as const)
        : (["PREVIEW"] as const),
    actionBlockers:
      inv.status === "POSTED"
        ? [
            {
              action: "EDIT",
              code: "POSTED_IMMUTABLE",
              message: "已登记发票不可编辑删除，纠错请发起红票。",
            },
          ]
        : [],
  }
}

function matchQ(
  q: string | undefined,
  parts: readonly (string | undefined)[]
): boolean {
  if (!q?.trim()) return true
  const needle = q.trim().toLowerCase()
  return parts.some((p) => p?.toLowerCase().includes(needle))
}

export function queryW12SupplierAccounts(
  query: SupplierAccountsQuery
): SupplierAccountsListView {
  const policy = getW12PayablePriorityPolicy()
  const watermark = `w12-list-${w12WatermarkSeq}`
  const queriedAt = w12NowIso()

  if (w12PermissionRevoked) {
    return {
      view: query.view,
      metrics: {
        openPayableTotal: "0.00",
        overduePayableTotal: "0.00",
        unallocatedPaymentTotal: "0.00",
        unallocatedInvoiceTotal: "0.00",
        prepayGateBlockedCount: 0,
      },
      payables: [],
      payments: [],
      invoices: [],
      unallocated: [],
      suppliers: [],
      total: 0,
      filterSummary: "权限已收回",
      permissionVersion: "pv-w12-revoked",
      dataWatermark: watermark,
      queriedAt,
      moduleAllowed: false,
      hasDataScope: false,
      canRegisterPayment: false,
      canRegisterInvoice: false,
      canExport: false,
      emptyReason: "PERMISSION_REVOKED",
      payablePriorityPolicy: policy,
      allowFullBankReveal: false,
    }
  }

  let payables = allPayables().map(projectPayableRow)
  let payments = allPayments().map(projectPaymentRow)
  let invoices = allInvoices().map(projectInvoiceRow)

  if (query.supplierId) {
    payables = payables.filter((p) => p.supplierId === query.supplierId)
    payments = payments.filter((p) => p.supplierId === query.supplierId)
    invoices = invoices.filter((p) => p.supplierId === query.supplierId)
  }
  if (query.sourceType) {
    payables = payables.filter((p) => p.sourceType === query.sourceType)
  }
  if (query.purchaseOrderId) {
    payables = payables.filter(
      (p) =>
        p.sourceType === "PURCHASE_ORDER" &&
        p.sourceDocumentId === query.purchaseOrderId
    )
  }
  if (query.status && query.status !== "all") {
    payables = payables.filter((p) => p.status === query.status)
  }
  if (query.due && query.due !== "all") {
    payables = payables.filter((p) => p.dueState === query.due)
  }
  if (query.paymentGate === "satisfied") {
    payables = payables.filter((p) => p.paymentGateSummary?.state === "SATISFIED")
  } else if (query.paymentGate === "unsatisfied") {
    payables = payables.filter((p) => p.paymentGateSummary?.state === "BLOCKED")
  }
  if (query.q?.trim()) {
    payables = payables.filter((p) =>
      matchQ(query.q, [
        p.supplierName,
        p.sourceDocumentNo,
        p.payableAccountId,
      ])
    )
    payments = payments.filter((p) =>
      matchQ(query.q, [p.supplierName, p.paymentNo, p.bankReferenceMasked])
    )
    invoices = invoices.filter((p) =>
      matchQ(query.q, [
        p.supplierName,
        p.invoiceNo,
        p.invoiceCode,
        p.invoiceId,
      ])
    )
  }

  // Metrics from full authorized scope (not current view filter only) — still
  // respect supplier/q if provided as scope; use unfiltered domain baseline.
  const allP = allPayables().map(projectPayableRow)
  const allPay = allPayments().map(projectPaymentRow)
  const allInv = allInvoices().map(projectInvoiceRow)

  const openPayableTotal = w12FromCents(
    allP.reduce((s, p) => s + w12Cents(p.openTotal), 0)
  )
  const overduePayableTotal = w12FromCents(
    allP
      .filter((p) => p.dueState === "overdue")
      .reduce((s, p) => s + w12Cents(p.openTotal), 0)
  )
  const unallocatedPaymentTotal = w12FromCents(
    allPay
      .filter((p) => p.status === "POSTED")
      .reduce((s, p) => s + w12Cents(p.unallocatedAmount), 0)
  )
  const unallocatedInvoiceTotal = w12FromCents(
    allInv
      .filter((p) => p.status === "POSTED" && p.invoiceKind === "BLUE")
      .reduce((s, p) => s + w12Cents(p.unallocatedAmount), 0)
  )
  const prepayGateBlockedCount = allP.filter(
    (p) => p.paymentGateSummary?.state === "BLOCKED"
  ).length

  const suppliers = W12_SUPPLIERS.map((s) => {
    const open = allP
      .filter((p) => p.supplierId === s.supplierId)
      .reduce((acc, p) => acc + w12Cents(p.openTotal), 0)
    const uPay = allPay
      .filter((p) => p.supplierId === s.supplierId && p.status === "POSTED")
      .reduce((acc, p) => acc + w12Cents(p.unallocatedAmount), 0)
    const uInv = allInv
      .filter(
        (p) =>
          p.supplierId === s.supplierId &&
          p.status === "POSTED" &&
          p.invoiceKind === "BLUE"
      )
      .reduce((acc, p) => acc + w12Cents(p.unallocatedAmount), 0)
    return {
      supplierId: s.supplierId,
      supplierName: s.supplierName,
      openPayableTotal: w12FromCents(open),
      unallocatedPaymentTotal: w12FromCents(uPay),
      unallocatedInvoiceTotal: w12FromCents(uInv),
    }
  }).filter(
    (s) =>
      w12Cents(s.openPayableTotal) > 0 ||
      w12Cents(s.unallocatedPaymentTotal) > 0 ||
      w12Cents(s.unallocatedInvoiceTotal) > 0 ||
      allP.some((p) => p.supplierId === s.supplierId)
  )

  const unallocated: UnallocatedRow[] = [
    ...allPay
      .filter((p) => p.status === "POSTED" && w12Cents(p.unallocatedAmount) > 0)
      .map((p) => ({
        id: `u_pay_${p.paymentId}`,
        track: "payment" as const,
        trackLabel: "付款待核销",
        documentNo: p.paymentNo,
        supplierId: p.supplierId,
        supplierName: p.supplierName,
        amount: p.amount,
        unallocatedAmount: p.unallocatedAmount,
        occurredAt: p.paidAt,
        statusLabel: "待分配",
        statusTone: "warning" as const,
      })),
    ...allInv
      .filter(
        (p) =>
          p.status === "POSTED" &&
          p.invoiceKind === "BLUE" &&
          w12Cents(p.unallocatedAmount) > 0
      )
      .map((p) => ({
        id: `u_inv_${p.invoiceId}`,
        track: "purchase_invoice" as const,
        trackLabel: "进项票待核销",
        documentNo: `${p.invoiceCode}-${p.invoiceNo}`,
        supplierId: p.supplierId,
        supplierName: p.supplierName,
        amount: p.grossAmount,
        unallocatedAmount: p.unallocatedAmount,
        occurredAt: p.invoiceDate,
        statusLabel: "待分配",
        statusTone: "info" as const,
      })),
  ]

  let filteredUnallocated = unallocated
  if (query.supplierId) {
    filteredUnallocated = filteredUnallocated.filter(
      (u) => u.supplierId === query.supplierId
    )
  }
  if (query.q?.trim()) {
    filteredUnallocated = filteredUnallocated.filter((u) =>
      matchQ(query.q, [u.supplierName, u.documentNo, u.trackLabel])
    )
  }

  const total =
    query.view === "payable"
      ? payables.length
      : query.view === "payment"
        ? payments.length
        : query.view === "purchase_invoice"
          ? invoices.length
          : filteredUnallocated.length

  const parts = [
    query.view === "payable"
      ? "应付台账"
      : query.view === "payment"
        ? "付款"
        : query.view === "purchase_invoice"
          ? "进项发票"
          : "待核销",
  ]
  if (query.supplierId) {
    const name =
      W12_SUPPLIERS.find((s) => s.supplierId === query.supplierId)
        ?.supplierName ?? query.supplierId
    parts.push(name)
  }
  if (query.q?.trim()) parts.push(`搜索「${query.q.trim()}」`)
  parts.push(`${total} 条`)

  const hasAny =
    allP.length + allPay.length + allInv.length > 0
  const emptyReason =
    total === 0
      ? hasAny
        ? ("FILTER_NO_RESULT" as const)
        : ("NO_DATA" as const)
      : undefined

  return {
    view: query.view,
    metrics: {
      openPayableTotal,
      overduePayableTotal,
      unallocatedPaymentTotal,
      unallocatedInvoiceTotal,
      prepayGateBlockedCount,
    },
    payables,
    payments,
    invoices,
    unallocated: filteredUnallocated,
    suppliers,
    total,
    filterSummary: parts.join(" · "),
    permissionVersion: "pv-w12-demo-1",
    dataWatermark: watermark,
    queriedAt,
    moduleAllowed: true,
    hasDataScope: true,
    canRegisterPayment: true,
    canRegisterInvoice: true,
    canExport: true,
    emptyReason,
    payablePriorityPolicy: policy,
    allowFullBankReveal: false,
  }
}

export function getW12PayableDetail(
  payableAccountId: string
): PayableDetailView | null {
  const p = resolvePayable(payableAccountId)
  if (!p) return null
  const row = projectPayableRow(p)
  const payments = allPayments().map(projectPaymentRow)
  const invoices = allInvoices().map(projectInvoiceRow)
  const paymentAllocations = payments.flatMap((pay) =>
    pay.allocations.filter((a) => a.payableAccountId === payableAccountId)
  )
  const invoiceAllocations = invoices.flatMap((inv) =>
    inv.allocations.filter((a) => a.payableAccountId === payableAccountId)
  )
  return {
    payable: row,
    entries: [
      {
        entryId: p.primaryEntryId,
        entryTypeLabel:
          p.sourceType === "PURCHASE_ORDER" ? "采购确认应付" : "结算确认应付",
        direction: "increase",
        amount: p.grossTotal,
        sourceLabel: `${SOURCE_TYPE_LABEL[p.sourceType]} ${p.sourceDocumentNo}`,
        dueDate: p.dueDate,
        occurredAt: `${p.dueDate}T00:00:00+08:00`,
      },
    ],
    paymentAllocations,
    invoiceAllocations,
    dataWatermark: `w12-detail-${payableAccountId}-${w12WatermarkSeq}`,
    queriedAt: w12NowIso(),
  }
}

export function openW12AllocationSession(input: {
  track: AllocationTrack
  supplierId: string
  draftSessionId?: string
  purchaseOrderId?: string
  returnTo?: string
  fromWorkspace?: string
  existingPaymentId?: string
  existingInvoiceId?: string
  preselectPayableAccountId?: string
}): AllocationSessionView {
  if (w12PermissionRevoked) {
    throw new SupplierPayablesMockError(
      "PERMISSION_REVOKED",
      "当前账号无供应商往来登记权限或权限已被收回。"
    )
  }
  const supplier = W12_SUPPLIERS.find((s) => s.supplierId === input.supplierId)
  if (!supplier) {
    throw new SupplierPayablesMockError(
      "SUPPLIER_NOT_FOUND",
      "供应商不存在或不在授权范围。"
    )
  }
  const draftSessionId =
    input.draftSessionId ??
    `alloc_sup_${input.supplierId}_${input.track}_${Date.now().toString(36)}`

  const poolPayables = allPayables()
    .filter((p) => p.supplierId === input.supplierId)
    .map(projectPayableRow)
    .filter((p) =>
      input.track === "payment"
        ? w12Cents(p.openTotal) > 0
        : w12Cents(p.openInvoiceableTotal) > 0
    )

  // Only same supplier — never mix
  const pool = poolPayables.map((p) => ({
    payableAccountId: p.payableAccountId,
    primaryEntryId: p.primaryEntryId,
    entryLockVersion: p.entryLockVersion,
    accountLockVersion: p.accountLockVersion,
    sourceType: p.sourceType,
    sourceTypeLabel: p.sourceTypeLabel,
    sourceDocumentNo: p.sourceDocumentNo,
    sourceDocumentId: p.sourceDocumentId,
    openTotal: p.openTotal,
    openInvoiceableTotal: p.openInvoiceableTotal,
    dueDate: p.dueDate,
    dueStateLabel: p.dueStateLabel,
    statusLabel: p.statusLabel,
  }))

  const preselected: string[] = []
  if (input.preselectPayableAccountId) {
    preselected.push(input.preselectPayableAccountId)
  }
  if (input.purchaseOrderId) {
    for (const item of pool) {
      if (
        item.sourceType === "PURCHASE_ORDER" &&
        item.sourceDocumentId === input.purchaseOrderId &&
        !preselected.includes(item.payableAccountId)
      ) {
        preselected.push(item.payableAccountId)
      }
    }
  }

  let existingAmount: string | undefined
  let existingUnallocated: string | undefined
  let existingDocumentNo: string | undefined
  if (input.existingPaymentId) {
    const pay = allPayments()
      .map(projectPaymentRow)
      .find((p) => p.paymentId === input.existingPaymentId)
    if (pay) {
      if (pay.supplierId !== input.supplierId) {
        throw new SupplierPayablesMockError(
          "SUPPLIER_MISMATCH",
          "付款供应商与会话供应商不一致，不能进入同一核销池。"
        )
      }
      existingAmount = pay.amount
      existingUnallocated = pay.unallocatedAmount
      existingDocumentNo = pay.paymentNo
    }
  }
  if (input.existingInvoiceId) {
    const inv = allInvoices()
      .map(projectInvoiceRow)
      .find((p) => p.invoiceId === input.existingInvoiceId)
    if (inv) {
      if (inv.supplierId !== input.supplierId) {
        throw new SupplierPayablesMockError(
          "SUPPLIER_MISMATCH",
          "发票供应商与会话供应商不一致，不能进入同一核销池。"
        )
      }
      existingAmount = inv.grossAmount
      existingUnallocated = inv.unallocatedAmount
      existingDocumentNo = `${inv.invoiceCode}-${inv.invoiceNo}`
    }
  }

  const draft = w12AllocationDrafts.get(draftSessionId)

  return {
    draftSessionId,
    track: input.track,
    supplierId: supplier.supplierId,
    supplierName: supplier.supplierName,
    pool,
    payablePriorityPolicy: getW12PayablePriorityPolicy(),
    preselectedPayableAccountIds: preselected,
    purchaseOrderId: input.purchaseOrderId,
    returnTo: input.returnTo,
    fromWorkspace: input.fromWorkspace,
    dataWatermark: w12BumpWatermark(),
    queriedAt: w12NowIso(),
    draftSavedAt: draft?.savedAt,
    existingPaymentId: input.existingPaymentId,
    existingInvoiceId: input.existingInvoiceId,
    existingAmount,
    existingUnallocated,
    existingDocumentNo,
  }
}

export function saveW12AllocationDraft(input: SaveAllocationDraftInput): {
  savedAt: string
} {
  const savedAt = w12NowIso()
  w12AllocationDrafts.set(input.draftSessionId, {
    track: input.track,
    supplierId: input.supplierId,
    formSnapshot: input.formSnapshot,
    savedAt,
  })
  return { savedAt }
}

export function getW12AllocationDraft(draftSessionId: string) {
  return w12AllocationDrafts.get(draftSessionId) ?? null
}

function applyPayablePaymentDelta(
  payableAccountId: string,
  deltaSettled: string
): void {
  const current = resolvePayable(payableAccountId)
  if (!current) return
  const settledTotal = w12Add(current.settledTotal, deltaSettled)
  const open = w12Max0(w12Sub(current.grossTotal, settledTotal))
  const status = derivePayableStatus(open, current.grossTotal)
  let paymentGate = current.paymentGate
  if (paymentGate && paymentGate.state !== "NOT_APPLICABLE") {
    const satisfied = w12Cents(settledTotal) >= w12Cents(paymentGate.required)
    paymentGate = {
      ...paymentGate,
      allocated: settledTotal,
      gap: gateGap(paymentGate.required, settledTotal),
      state: satisfied ? "SATISFIED" : "BLOCKED",
      message: satisfied
        ? "有效已付净核销已满足先款条件"
        : paymentGate.message,
    }
  }
  w12PayableOverlays.set(payableAccountId, {
    settledTotal,
    invoicedTotal: current.invoicedTotal,
    entryLockVersion: current.entryLockVersion + 1,
    accountLockVersion: current.accountLockVersion + 1,
    status,
    paymentGate,
  })

  // Sync W08 / W09 gate projections for purchase-order sources
  if (current.sourceType === "PURCHASE_ORDER") {
    syncDownstreamPaymentGate(
      current.sourceDocumentId,
      settledTotal,
      paymentGate
    )
  }
}

function applyPayableInvoiceDelta(
  payableAccountId: string,
  deltaInvoiced: string
): void {
  const current = resolvePayable(payableAccountId)
  if (!current) return
  const invoicedTotal = w12Add(current.invoicedTotal, deltaInvoiced)
  w12PayableOverlays.set(payableAccountId, {
    settledTotal: current.settledTotal,
    invoicedTotal,
    entryLockVersion: current.entryLockVersion,
    accountLockVersion: current.accountLockVersion + 1,
    status: current.status,
    paymentGate: current.paymentGate,
  })
}

function syncDownstreamPaymentGate(
  purchaseOrderId: string,
  settledTotal: string,
  paymentGate: SeedPayable["paymentGate"] | undefined
): void {
  if (!paymentGate || paymentGate.state === "NOT_APPLICABLE") {
    // still update W08 paid amounts when present
  }
  const required = paymentGate?.required ?? "0.00"
  const satisfied =
    !paymentGate ||
    paymentGate.state === "NOT_APPLICABLE" ||
    w12Cents(settledTotal) >= w12Cents(required)
  const state =
    !paymentGate || paymentGate.state === "NOT_APPLICABLE"
      ? ("NOT_APPLICABLE" as const)
      : satisfied
        ? ("SATISFIED" as const)
        : ("BLOCKED" as const)
  const message =
    state === "SATISFIED"
      ? "有效已付净核销已满足先款条件"
      : state === "BLOCKED"
        ? paymentGate?.message ?? "先款净核销不足"
        : "后款条件，无先款门禁"

  const prev = w08Objects.get(purchaseOrderId)
  if (prev || resolveSeed(purchaseOrderId)) {
    const seed = resolveSeed(purchaseOrderId)
    const base = prev ?? {
      status: seed!.status,
      reviewStatus: seed!.reviewStatus,
      lockVersion: seed!.lockVersion,
      contentSource: seed!.contentSource,
    }
    const grossPayable = seed?.payable
      ? w12Add(seed.payable.payableOpenAmount, seed.payable.paidAllocatedAmount)
      : undefined
    const openPayable = grossPayable
      ? w12Max0(w12Sub(grossPayable, settledTotal))
      : undefined
    w08Objects.set(purchaseOrderId, {
      ...base,
      payable: seed?.payable
        ? {
            payableOpenAmount: openPayable ?? seed.payable.payableOpenAmount,
            paidAllocatedAmount: settledTotal,
            purchaseInvoiceAllocatedAmount:
              seed.payable.purchaseInvoiceAllocatedAmount,
          }
        : prev?.payable,
      paymentProgress:
        w12Cents(settledTotal) <= 0
          ? "未付"
          : satisfied && state !== "NOT_APPLICABLE"
            ? "已付"
            : "部分",
      paymentGate: state === "NOT_APPLICABLE" ? "NOT_APPLICABLE" : state,
      prepayment: {
        state,
        message,
        required,
        allocated: settledTotal,
        gap: gateGap(required, settledTotal),
      },
      allowedActions:
        state === "BLOCKED"
          ? Array.from(
              new Set([
                ...(prev?.allowedActions ?? seed?.allowedActions ?? []),
                "PAY",
                "OPEN_CENTER",
              ])
            ).filter((a) => a !== "FULFILL")
          : Array.from(
              new Set([
                ...(prev?.allowedActions ?? seed?.allowedActions ?? []),
                "FULFILL",
                "PAY",
                "OPEN_CENTER",
              ])
            ),
      actionBlockers:
        state === "BLOCKED"
          ? [
              {
                action: "FULFILL",
                code: "PREPAYMENT_GATE",
                message: "先款门禁未满足，请先完成有效付款核销",
              },
            ]
          : (prev?.actionBlockers ?? seed?.actionBlockers ?? []).filter(
              (b) => b.action !== "FULFILL"
            ),
    })
  }

  if (state !== "NOT_APPLICABLE") {
    w09PaymentGateByPo.set(purchaseOrderId, {
      state,
      message,
      effectivePaidAmount: settledTotal,
      requiredAmount: required,
    })
  }
}

function validateTargetsSameSupplier(
  supplierId: string,
  targets: readonly { payableAccountId: string; amount: string }[]
): SeedPayable[] {
  const resolved: SeedPayable[] = []
  for (const t of targets) {
    const p = resolvePayable(t.payableAccountId)
    if (!p) {
      throw new SupplierPayablesMockError(
        "PAYABLE_NOT_FOUND",
        `应付账户 ${t.payableAccountId} 不存在`
      )
    }
    if (p.supplierId !== supplierId) {
      throw new SupplierPayablesMockError(
        "CROSS_SUPPLIER",
        "不同供应商的目标不能进入同一核销池；服务端已拒绝跨供应商提交。"
      )
    }
    if (w12Cents(t.amount) <= 0) {
      throw new SupplierPayablesMockError(
        "INVALID_AMOUNT",
        "分配金额必须为正数"
      )
    }
    resolved.push(p)
  }
  return resolved
}

function validatePolicyForMixed(
  payables: SeedPayable[],
  input: {
    explicitSelection: boolean
    payablePriorityPolicyId?: string
    payablePriorityPolicyVersion?: number
  }
): void {
  const types = new Set(payables.map((p) => p.sourceType))
  const mixed = types.size > 1
  if (!mixed) return
  const policy = getW12PayablePriorityPolicy()
  if (!policy.mixedAutoAllocationAllowed || policy.state !== "AVAILABLE") {
    if (!input.explicitSelection) {
      throw new SupplierPayablesMockError(
        "POLICY_BLOCKED",
        policy.blockerMessage ??
          "优先级策略不可用，混合应付须显式逐项选择目标。"
      )
    }
    return
  }
  // Auto path must echo policy id/version
  if (
    !input.explicitSelection &&
    (input.payablePriorityPolicyId !== policy.payablePriorityPolicyId ||
      input.payablePriorityPolicyVersion !==
        policy.payablePriorityPolicyVersion)
  ) {
    throw new SupplierPayablesMockError(
      "POLICY_STALE",
      "应付优先级策略版本不一致，请刷新后重试或改为显式选择。"
    )
  }
}

export function postW12Payment(input: PostPaymentInput): FormalSubmitResult {
  if (w12PermissionRevoked) {
    return {
      status: "blocked",
      title: "权限已收回",
      description: "当前账号无法登记付款，敏感字段已清除。",
      errorCode: "PERMISSION_REVOKED",
    }
  }

  const existing = w12Idempotency.get(input.idempotencyKey)
  if (existing) return existing
  const pending = w12PendingUnknown.get(input.idempotencyKey)
  if (pending) return pending

  if (input.forceUnknown) {
    const unknown: FormalSubmitResult = {
      status: "unknown",
      title: "结果不确定",
      description:
        "提交超时，未确认付款是否过账。请用同一操作号查询最终结果，勿重复付款。",
      operationId: `op_w12_${input.idempotencyKey.slice(0, 12)}`,
      reference: input.idempotencyKey,
    }
    w12PendingUnknown.set(input.idempotencyKey, {
      ...unknown,
      // store eventual success for resolve
      status: "succeeded",
      title: "付款已过账（查询确认）",
      description: "按操作号查询后确认提交已成功。",
      documentNo: `FK-PEND-${w12PaymentNoSeq.n + 1}`,
      unallocatedAmount: "0.00",
    })
    return unknown
  }

  try {
    if (w12Cents(input.amount) <= 0 && !input.existingPaymentId) {
      throw new SupplierPayablesMockError("INVALID_AMOUNT", "付款金额必须为正数")
    }

    const supplier = W12_SUPPLIERS.find((s) => s.supplierId === input.supplierId)
    if (!supplier) {
      throw new SupplierPayablesMockError("SUPPLIER_NOT_FOUND", "供应商不存在")
    }

    const payables = validateTargetsSameSupplier(
      input.supplierId,
      input.targets
    )
    validatePolicyForMixed(payables, input)

    if (input.forceVersionConflict) {
      throw new SupplierPayablesMockError(
        "VERSION_CONFLICT",
        "目标应付余额或版本已变化，请刷新核销池后重新确认分配。"
      )
    }

    // Version / balance checks
    for (const t of input.targets) {
      const p = resolvePayable(t.payableAccountId)!
      if (
        t.entryLockVersion !== p.entryLockVersion ||
        t.accountLockVersion !== p.accountLockVersion
      ) {
        throw new SupplierPayablesMockError(
          "VERSION_CONFLICT",
          `应付 ${p.sourceDocumentNo} 版本冲突，请刷新后重试。`
        )
      }
      const open = w12Max0(w12Sub(p.grossTotal, p.settledTotal))
      if (w12Cents(t.amount) > w12Cents(open)) {
        throw new SupplierPayablesMockError(
          "OVER_ALLOCATION",
          `分配金额超过 ${p.sourceDocumentNo} 开放应付 ${open}`
        )
      }
    }

    const allocSum = input.targets.reduce(
      (s, t) => s + w12Cents(t.amount),
      0
    )

    let payment: SeedPayment
    if (input.existingPaymentId) {
      const base =
        w12SessionPayments.get(input.existingPaymentId) ??
        SEED_PAYMENTS.find((p) => p.paymentId === input.existingPaymentId)
      if (!base || base.status !== "POSTED") {
        throw new SupplierPayablesMockError(
          "PAYMENT_NOT_FOUND",
          "原付款不存在或不可继续核销"
        )
      }
      if (base.supplierId !== input.supplierId) {
        throw new SupplierPayablesMockError(
          "CROSS_SUPPLIER",
          "不同供应商的目标不能进入同一核销池；服务端已拒绝跨供应商提交。"
        )
      }
      const current = projectPaymentRow(base)
      if (allocSum > w12Cents(current.unallocatedAmount)) {
        throw new SupplierPayablesMockError(
          "OVER_ALLOCATION",
          `分配合计超过付款未分配余额 ${current.unallocatedAmount}`
        )
      }
      const now = w12NowIso()
      const newAllocs = input.targets.map((t, i) => ({
        allocationId: `palloc_${base.paymentId}_${Date.now()}_${i}`,
        action: "APPLY" as const,
        payableAccountId: t.payableAccountId,
        payableEntryId:
          t.payableEntryId ??
          resolvePayable(t.payableAccountId)!.primaryEntryId,
        amount: t.amount,
        occurredAt: now,
      }))
      payment = {
        ...base,
        allocations: [...base.allocations, ...newAllocs],
      }
      w12SessionPayments.set(payment.paymentId, payment)
    } else {
      if (allocSum > w12Cents(input.amount)) {
        throw new SupplierPayablesMockError(
          "OVER_ALLOCATION",
          "分配合计超过付款金额"
        )
      }
      const seq = ++w12PaymentNoSeq.n
      const paymentId = `pay_sess_${seq}`
      const paymentNo = `FK20260801${String(seq).padStart(3, "0")}`
      const now = w12NowIso()
      payment = {
        paymentId,
        paymentNo,
        supplierId: supplier.supplierId,
        supplierName: supplier.supplierName,
        paidAt: input.paidAt || now,
        amount: input.amount,
        bankReference: input.bankReference || `BANK-DEMO-${seq}`,
        status: "POSTED",
        allocations: input.targets.map((t, i) => ({
          allocationId: `palloc_${paymentId}_${i}`,
          action: "APPLY" as const,
          payableAccountId: t.payableAccountId,
          payableEntryId:
            t.payableEntryId ??
            resolvePayable(t.payableAccountId)!.primaryEntryId,
          amount: t.amount,
          occurredAt: now,
        })),
      }
      w12SessionPayments.set(paymentId, payment)
    }

    for (const t of input.targets) {
      applyPayablePaymentDelta(t.payableAccountId, t.amount)
    }

    w12BumpWatermark()
    const row = projectPaymentRow(payment)
    const result: FormalSubmitResult = {
      status: "succeeded",
      title: "付款已登记并核销",
      description: `付款单 ${row.paymentNo} 已过账；未分配余额以服务端净有效分配为准。未核销付款不满足先款门禁。`,
      reference: row.paymentId,
      documentNo: row.paymentNo,
      operationId: `op_w12_pay_${row.paymentId}`,
      unallocatedAmount: row.unallocatedAmount,
      allocatedTotal: row.allocatedTotal,
      paymentGateRefreshHint:
        "请返回来源页重新查询 PrepaymentGate；勿信任客户端布尔值。",
      facts: [
        { label: "付款单号", value: row.paymentNo },
        { label: "付款金额", value: row.amount },
        { label: "净已分配", value: row.allocatedTotal },
        { label: "未分配", value: row.unallocatedAmount },
        { label: "供应商", value: row.supplierName },
      ],
    }
    w12Idempotency.set(input.idempotencyKey, result)
    return result
  } catch (e) {
    if (e instanceof SupplierPayablesMockError) {
      return {
        status: e.code === "CROSS_SUPPLIER" ? "blocked" : "failed",
        title: "付款提交失败",
        description: e.message,
        errorCode: e.code,
      }
    }
    throw e
  }
}

export function postW12Invoice(input: PostInvoiceInput): FormalSubmitResult {
  if (w12PermissionRevoked) {
    return {
      status: "blocked",
      title: "权限已收回",
      description: "当前账号无法登记进项发票。",
      errorCode: "PERMISSION_REVOKED",
    }
  }

  const existing = w12Idempotency.get(input.idempotencyKey)
  if (existing) return existing

  if (input.forceUnknown) {
    return {
      status: "unknown",
      title: "结果不确定",
      description:
        "提交超时，未确认进项发票是否登记。请用同一操作号查询，勿重复提交。",
      operationId: `op_w12_inv_${input.idempotencyKey.slice(0, 10)}`,
      reference: input.idempotencyKey,
    }
  }

  try {
    const supplier = W12_SUPPLIERS.find((s) => s.supplierId === input.supplierId)
    if (!supplier) {
      throw new SupplierPayablesMockError("SUPPLIER_NOT_FOUND", "供应商不存在")
    }

    // Duplicate invoice detection
    const dup = allInvoices().find(
      (inv) =>
        inv.invoiceCode === input.invoiceCode &&
        inv.invoiceNo === input.invoiceNo &&
        inv.invoiceKind === "BLUE" &&
        !input.existingInvoiceId
    )
    if (dup || input.forceDuplicateInvoice) {
      const hit = dup ?? allInvoices()[0]
      return {
        status: "blocked",
        title: "发票号码重复",
        description: `已存在相同代码/号码的进项发票 ${hit.invoiceCode}-${hit.invoiceNo}，不创建副本。`,
        errorCode: "DUPLICATE_INVOICE",
        existingDocumentId: hit.invoiceId,
        documentNo: `${hit.invoiceCode}-${hit.invoiceNo}`,
        reference: hit.invoiceId,
      }
    }

    const payables = validateTargetsSameSupplier(
      input.supplierId,
      input.targets
    )
    validatePolicyForMixed(payables, input)

    if (input.forceVersionConflict) {
      throw new SupplierPayablesMockError(
        "VERSION_CONFLICT",
        "目标可收票余额或版本已变化，请刷新后重新确认。"
      )
    }

    for (const t of input.targets) {
      const p = resolvePayable(t.payableAccountId)!
      if (t.accountLockVersion !== p.accountLockVersion) {
        throw new SupplierPayablesMockError(
          "VERSION_CONFLICT",
          `应付 ${p.sourceDocumentNo} 账户版本冲突`
        )
      }
      const openInv = w12Max0(w12Sub(p.grossTotal, p.invoicedTotal))
      if (w12Cents(t.amount) > w12Cents(openInv)) {
        throw new SupplierPayablesMockError(
          "OVER_ALLOCATION",
          `分配金额超过 ${p.sourceDocumentNo} 可收票余额 ${openInv}`
        )
      }
    }

    const allocSum = input.targets.reduce(
      (s, t) => s + w12Cents(t.amount),
      0
    )

    let invoice: SeedInvoice
    if (input.existingInvoiceId) {
      const base =
        w12SessionInvoices.get(input.existingInvoiceId) ??
        SEED_INVOICES.find((p) => p.invoiceId === input.existingInvoiceId)
      if (!base || base.status !== "POSTED") {
        throw new SupplierPayablesMockError(
          "INVOICE_NOT_FOUND",
          "原发票不存在或不可继续核销"
        )
      }
      if (base.supplierId !== input.supplierId) {
        throw new SupplierPayablesMockError(
          "CROSS_SUPPLIER",
          "不同供应商的目标不能进入同一核销池。"
        )
      }
      const current = projectInvoiceRow(base)
      if (allocSum > w12Cents(current.unallocatedAmount)) {
        throw new SupplierPayablesMockError(
          "OVER_ALLOCATION",
          `分配合计超过发票未分配余额 ${current.unallocatedAmount}`
        )
      }
      const now = w12NowIso()
      invoice = {
        ...base,
        allocations: [
          ...base.allocations,
          ...input.targets.map((t, i) => ({
            allocationId: `ialloc_${base.invoiceId}_${Date.now()}_${i}`,
            action: "APPLY" as const,
            payableAccountId: t.payableAccountId,
            amountGross: t.amount,
            occurredAt: now,
          })),
        ],
      }
      w12SessionInvoices.set(invoice.invoiceId, invoice)
    } else {
      if (w12Cents(input.grossAmount) <= 0) {
        throw new SupplierPayablesMockError(
          "INVALID_AMOUNT",
          "发票含税金额必须为正数"
        )
      }
      if (allocSum > w12Cents(input.grossAmount)) {
        throw new SupplierPayablesMockError(
          "OVER_ALLOCATION",
          "分配合计超过发票含税金额"
        )
      }
      const seq = ++w12InvoiceSeq.n
      const invoiceId = `inv_sess_${seq}`
      const now = w12NowIso()
      invoice = {
        invoiceId,
        invoiceCode: input.invoiceCode,
        invoiceNo: input.invoiceNo,
        invoiceKind: input.invoiceKind,
        supplierId: supplier.supplierId,
        supplierName: supplier.supplierName,
        invoiceDate: input.invoiceDate || now.slice(0, 10),
        grossAmount: input.grossAmount,
        netAmount: input.netAmount || input.grossAmount,
        taxAmount: input.taxAmount || "0.00",
        status: "POSTED",
        originalInvoiceId: input.originalInvoiceId,
        allocations: input.targets.map((t, i) => ({
          allocationId: `ialloc_${invoiceId}_${i}`,
          action: "APPLY" as const,
          payableAccountId: t.payableAccountId,
          amountGross: t.amount,
          occurredAt: now,
        })),
      }
      w12SessionInvoices.set(invoiceId, invoice)
    }

    for (const t of input.targets) {
      applyPayableInvoiceDelta(t.payableAccountId, t.amount)
    }

    w12BumpWatermark()
    const row = projectInvoiceRow(invoice)
    const result: FormalSubmitResult = {
      status: "succeeded",
      title: "进项发票已登记并核销",
      description:
        "进项票与付款轨道独立；收票进度不表示已付款。净分配以服务端为准。",
      reference: row.invoiceId,
      documentNo: `${row.invoiceCode}-${row.invoiceNo}`,
      operationId: `op_w12_inv_${row.invoiceId}`,
      unallocatedAmount: row.unallocatedAmount,
      allocatedTotal: row.allocatedTotal,
      facts: [
        { label: "发票", value: `${row.invoiceCode}-${row.invoiceNo}` },
        { label: "种类", value: row.invoiceKindLabel },
        { label: "含税金额", value: row.grossAmount },
        { label: "净已分配", value: row.allocatedTotal },
        { label: "未分配", value: row.unallocatedAmount },
      ],
    }
    w12Idempotency.set(input.idempotencyKey, result)
    return result
  } catch (e) {
    if (e instanceof SupplierPayablesMockError) {
      return {
        status: e.code === "CROSS_SUPPLIER" ? "blocked" : "failed",
        title: "进项发票提交失败",
        description: e.message,
        errorCode: e.code,
      }
    }
    throw e
  }
}

export function reverseW12Payment(
  input: ReversePaymentInput
): FormalSubmitResult {
  const existing = w12Idempotency.get(input.idempotencyKey)
  if (existing) return existing

  const base =
    w12SessionPayments.get(input.paymentId) ??
    SEED_PAYMENTS.find((p) => p.paymentId === input.paymentId)
  if (!base || base.status !== "POSTED") {
    return {
      status: "failed",
      title: "无法冲正",
      description: "仅已过账且未冲正的付款可发起冲正。",
      errorCode: "NOT_REVERSABLE",
    }
  }

  const now = w12NowIso()
  const seq = ++w12PaymentNoSeq.n
  const reverseId = `pay_rev_${seq}`
  const reverseNo = `FK-REV-${String(seq).padStart(4, "0")}`

  // Reverse allocations on payables
  for (const a of base.allocations) {
    if (a.action === "APPLY") {
      applyPayablePaymentDelta(a.payableAccountId, w12FromCents(-w12Cents(a.amount)))
    }
  }

  const reverseAllocs = base.allocations
    .filter((a) => a.action === "APPLY")
    .map((a, i) => ({
      allocationId: `palloc_rev_${reverseId}_${i}`,
      action: "REVERSE" as const,
      payableAccountId: a.payableAccountId,
      payableEntryId: a.payableEntryId,
      amount: a.amount,
      occurredAt: now,
      reverseOfAllocationId: a.allocationId,
    }))

  const reversed: SeedPayment = {
    ...base,
    status: "REVERSED",
    reversedByPaymentId: reverseId,
    allocations: [...base.allocations, ...reverseAllocs],
  }
  w12SessionPayments.set(base.paymentId, reversed)

  const reversePayment: SeedPayment = {
    paymentId: reverseId,
    paymentNo: reverseNo,
    supplierId: base.supplierId,
    supplierName: base.supplierName,
    paidAt: now,
    amount: base.amount,
    bankReference: `REV-OF-${base.paymentNo}`,
    status: "POSTED",
    reverseOfPaymentId: base.paymentId,
    allocations: reverseAllocs,
  }
  w12SessionPayments.set(reverseId, reversePayment)

  const result: FormalSubmitResult = {
    status: "succeeded",
    title: "付款冲正已过账",
    description: `原付款 ${base.paymentNo} 保留；已追加冲正单 ${reverseNo} 与反向分配。原因：${input.reason}`,
    reference: reverseId,
    documentNo: reverseNo,
    facts: [
      { label: "原付款", value: base.paymentNo },
      { label: "冲正单", value: reverseNo },
      { label: "冲正金额", value: base.amount },
    ],
  }
  w12Idempotency.set(input.idempotencyKey, result)
  return result
}

export function reverseW12Invoice(
  input: ReverseInvoiceInput
): FormalSubmitResult {
  const existing = w12Idempotency.get(input.idempotencyKey)
  if (existing) return existing

  const base =
    w12SessionInvoices.get(input.invoiceId) ??
    SEED_INVOICES.find((p) => p.invoiceId === input.invoiceId)
  if (!base || base.status !== "POSTED" || base.invoiceKind !== "BLUE") {
    return {
      status: "failed",
      title: "无法红冲",
      description: "仅已登记蓝票可发起红票。",
      errorCode: "NOT_REVERSABLE",
    }
  }

  const now = w12NowIso()
  const seq = ++w12InvoiceSeq.n
  const redId = `inv_red_${seq}`

  for (const a of base.allocations) {
    if (a.action === "APPLY") {
      applyPayableInvoiceDelta(
        a.payableAccountId,
        w12FromCents(-w12Cents(a.amountGross))
      )
    }
  }

  const reverseAllocs = base.allocations
    .filter((a) => a.action === "APPLY")
    .map((a, i) => ({
      allocationId: `ialloc_red_${redId}_${i}`,
      action: "REVERSE" as const,
      payableAccountId: a.payableAccountId,
      amountGross: a.amountGross,
      occurredAt: now,
      reverseOfAllocationId: a.allocationId,
    }))

  const blue: SeedInvoice = {
    ...base,
    status: "REVERSED",
    allocations: [...base.allocations, ...reverseAllocs],
  }
  w12SessionInvoices.set(base.invoiceId, blue)

  const red: SeedInvoice = {
    invoiceId: redId,
    invoiceCode: base.invoiceCode,
    invoiceNo: input.redInvoiceNo,
    invoiceKind: "RED",
    supplierId: base.supplierId,
    supplierName: base.supplierName,
    invoiceDate: now.slice(0, 10),
    grossAmount: base.grossAmount,
    netAmount: base.netAmount,
    taxAmount: base.taxAmount,
    status: "POSTED",
    originalInvoiceId: base.invoiceId,
    allocations: reverseAllocs,
  }
  w12SessionInvoices.set(redId, red)

  const result: FormalSubmitResult = {
    status: "succeeded",
    title: "进项红票已登记",
    description: `原蓝票 ${base.invoiceCode}-${base.invoiceNo} 保留；已追加红票与反向分配。原因：${input.reason}`,
    reference: redId,
    documentNo: `${red.invoiceCode}-${red.invoiceNo}`,
    facts: [
      { label: "原蓝票", value: `${base.invoiceCode}-${base.invoiceNo}` },
      { label: "红票号码", value: input.redInvoiceNo },
      { label: "红冲金额", value: base.grossAmount },
    ],
  }
  w12Idempotency.set(input.idempotencyKey, result)
  return result
}

export function resolveW12Unknown(
  idempotencyKey: string
): FormalSubmitResult | null {
  const pending = w12PendingUnknown.get(idempotencyKey)
  if (pending) {
    w12Idempotency.set(idempotencyKey, pending)
    w12PendingUnknown.delete(idempotencyKey)
    return pending
  }
  return w12Idempotency.get(idempotencyKey) ?? null
}

export function bumpW12PayableLocks(payableAccountId: string): void {
  const p = resolvePayable(payableAccountId)
  if (!p) return
  w12PayableOverlays.set(payableAccountId, {
    settledTotal: p.settledTotal,
    invoicedTotal: p.invoicedTotal,
    entryLockVersion: p.entryLockVersion + 1,
    accountLockVersion: p.accountLockVersion + 1,
    status: p.status,
    paymentGate: p.paymentGate,
  })
}
