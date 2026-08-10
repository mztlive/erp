/**
 * W02 统一待办队列 — 客户端契约类型（对齐 docs/ui-workspaces/w02 §8）。
 * 运行时数据来自 `/admin/work-items`。
 */

import type { StatusTone } from "@/components/ui/status-badge"

export type WorkItemFamily =
  | "approval"
  | "finance"
  | "fulfillment"
  | "exception"
  | "procurement"

export type WorkItemStatusCode =
  | "UNCLAIMED"
  | "PENDING"
  | "IN_PROGRESS"
  | "COMPLETED"
  | "TRANSFERRED"
  | "CLOSED"

export type WorkItemActionCode =
  | "CLAIM"
  | "DEFER"
  | "SAVE_EVIDENCE"
  | "QUERY_RESULT"
  | "TRANSFER"
  | "CLOSE"
  | "COMPLETE"

type WorkItemFixture = Readonly<{
  id: string
  workItemType: string
  workItemTypeLabel: string
  family: WorkItemFamily
  handlerKey: string
  handlerHref?: string
  completionAction: string
  businessObject: string
  counterparty: string
  enteredAt: string
  enteredDateTime: string
  dueAt: string
  dueDateTime: string
  responsibleParty: string
  reason: string
  impact: string
  impactSensitive?: string
  statusCode: WorkItemStatusCode
  status: { label: string; tone: StatusTone }
  priority: number
  priorityLabel: string
  /** 乐观锁版本字符串（后端 `version`）；页面沿用 subjectVersion 字段名 */
  subjectVersion: string
  allowedActions: readonly WorkItemActionCode[]
  actionBlockers?: Readonly<Partial<Record<WorkItemActionCode, string>>>
  closeAllowed: boolean
  scopeTags: readonly string[]
  summaryFields: readonly { label: string; value: string; numeric?: boolean }[]
  checkItems?: readonly string[]
  actionLabel?: string
  processorGroup: string
}>

export type QueueScopeSlug = "mine" | "role_pool" | "team" | "hold"

export type UnifiedQueueFilters = {
  scope: QueueScopeSlug
  family?: WorkItemFamily
  workItemType?: string
  due?: "today" | "overdue"
  query?: string
  /** When set, continuous-process mode: same type or processor group. */
  converge?: boolean
}

export type WorkItemActionRecord = {
  actionRecordId: string
  actionKind: string
  workItemStatus: "IN_PROGRESS"
  evidenceNote?: string
  recordedAt: string
}

export type QueueWorkItemView = WorkItemFixture & {
  effectiveStatusCode: WorkItemStatusCode
  claimedByLabel?: string
  lastAction?: WorkItemActionRecord
  permissionRevoked: boolean
  showClose: boolean
}

export type UnifiedTaskQueueView = {
  queueContextId: string
  permissionVersion: number
  permissionRevoked: boolean
  freshness: { updatedAt: string; state: "fresh" | "stale" }
  filterSummary: string
  total: number
  counts: {
    mine: number
    rolePool: number
    team: number
    hold: number
    overdue: number
  }
  items: QueueWorkItemView[]
}

export type InTaskActionKind = "DEFER" | "SAVE_EVIDENCE" | "QUERY_RESULT"

export type SessionLease = {
  workItemId: string
  ownerUserId: string
  subjectVersion?: string
}

export type CompleteSessionResult = {
  workItemId: string
  workItemStatus: "COMPLETED"
  completionRecordId: string
  businessResult: { kind: string; reference: string; summary: string }
  subjectVersion?: string
}

export type CloseSessionResult = {
  workItemId: string
  workItemStatus: "CLOSED"
  closureRecordId: string
  reasonCode: string
  replacementWorkItemId?: string
}

export type TransferSessionResult = {
  workItemId: string
  transferRecordId: string
  targetUserId: string
  subjectVersion?: string
}

export const FAMILY_LABELS: Record<WorkItemFamily, string> = {
  approval: "审批与确认",
  finance: "票款与结算",
  fulfillment: "履约与库存",
  exception: "数据治理与异常",
  procurement: "采购确认",
}
