import type {
  WorkItemActionCode,
  WorkItemFamily,
  WorkItemFixture,
  WorkItemStatusCode,
} from "@/mock/work-items"
import type { WorkItemActionRecord } from "@/mock/session-state"

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

export type DecisionDraft = {
  note: string
  /** Domain completion action identity from fixture.completionAction */
  completionAction: string
}

export type { WorkItemActionCode, WorkItemFamily, WorkItemStatusCode }
