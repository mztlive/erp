import type {
  WorkItemActionCode,
  WorkItemFamily,
  WorkItemFixture,
  WorkItemStatusCode,
} from "@/mock/work-items"
import type {
  SessionLease,
  SessionLeaseState,
  WorkItemActionRecord,
} from "@/mock/session-state"

/** Envelope types aligned with W02 §8.2 (client-side mock contracts). */

export type ClaimWorkItemCommand = {
  workItemId: string
  expectedLeaseVersion?: number
  idempotencyKey: string
}

export type WorkItemActionEnvelope<TAction> = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  action: TAction
}

export type CompleteWorkItemEnvelope<TDecision> = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  decision: TDecision
}

export type CloseWorkItemEnvelope = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  closure: {
    kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED" | "CLOSE_WITH_REPLACEMENT"
    reasonCode: string
    replacementWorkItemId?: string
    closureEvidenceReference: string
    comment?: string
  }
}

export type TransferWorkItemEnvelope = {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectVersion?: string
  expectedSubjectHash: string
  idempotencyKey: string
  transfer: { toUserLabel: string; reason: string }
}

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

export type QueueWorkItemView = WorkItemFixture & {
  /** Derived status after session terminal/hold overlays. */
  effectiveStatusCode: WorkItemStatusCode
  /** Lease state without token. */
  leaseState: SessionLeaseState | null
  claimedByOther: boolean
  claimedByLabel?: string
  lastAction?: WorkItemActionRecord
  /** Sensitive fields masked when permission revoked. */
  permissionRevoked: boolean
  /** Whether CLOSE is exposed in UI (server closeAllowed + not approval path). */
  showClose: boolean
}

export type UnifiedTaskQueueView = {
  queueContextId: string
  permissionVersion: number
  permissionRevoked: boolean
  freshness: { updatedAt: string; state: "fresh" | "stale" }
  filterSummary: string
  total: number
  counts: { mine: number; rolePool: number; overdue: number }
  items: QueueWorkItemView[]
}

export type ActiveClaim = SessionLease

export type InTaskActionKind = "DEFER" | "SAVE_EVIDENCE" | "QUERY_RESULT"

export type DecisionDraft = {
  note: string
  /** Domain completion action identity from fixture.completionAction */
  completionAction: string
}

export type { WorkItemActionCode, WorkItemFamily, WorkItemStatusCode }
