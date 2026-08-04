"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
import {
  applyWorkItemActionSession,
  bumpWorkItemSubject,
  claimWorkItemSession,
  clearSessionLease,
  closeWorkItemSession,
  completeWorkItemSession,
  ensureWorkItemSubject,
  finalizePendingComplete,
  getIdempotencyEntry,
  getPermissionVersion,
  getSessionLease,
  getSessionLeaseState,
  getWorkItemActionHistory,
  getWorkItemTerminal,
  isPermissionRevoked,
  isWorkItemHeld,
  loseSessionLease,
  queryIdempotencyResult,
  revokeWorkItemPermission,
  restoreWorkItemPermission,
  transferWorkItemSession,
  WorkItemMockError,
  type CompleteSessionResult,
  type SessionLease,
} from "@/mock/session-state"
import {
  FAMILY_LABELS,
  WORK_ITEM_FIXTURES,
  type WorkItemFixture,
} from "@/mock/work-items"

import type {
  CloseWorkItemEnvelope,
  CompleteWorkItemEnvelope,
  QueueWorkItemView,
  TransferWorkItemEnvelope,
  UnifiedTaskQueueView,
  WorkItemActionEnvelope,
} from "./types"
import { buildFilterSummary } from "./filter-work-items"
import type { UnifiedQueueFilters } from "./types"

export const unifiedQueueKeys = {
  all: ["unified-task-queue"] as const,
  view: (filters: UnifiedQueueFilters) =>
    [...unifiedQueueKeys.all, "view", filters] as const,
  permission: () => [...unifiedQueueKeys.all, "permission"] as const,
}

function toViewItem(fixture: WorkItemFixture): QueueWorkItemView | null {
  const terminal = getWorkItemTerminal(fixture.id)
  if (terminal) {
    // Terminal tasks leave the active open queue
    return null
  }

  ensureWorkItemSubject(fixture.id, {
    subjectVersion: fixture.subjectVersion,
    subjectHash: fixture.subjectHash,
    leaseVersion: fixture.leaseVersion,
  })

  const held = isWorkItemHeld(fixture.id)
  const leaseState = getSessionLeaseState(fixture.id)
  const lease = getSessionLease(fixture.id)
  const permissionRevoked = isPermissionRevoked()
  const history = getWorkItemActionHistory(fixture.id)
  const lastAction = history[history.length - 1]

  let effectiveStatusCode = fixture.statusCode
  let status = fixture.status
  const scopeTags = [...fixture.scopeTags]

  if (held) {
    effectiveStatusCode = "PENDING"
    status = { label: "已跳过", tone: "warning" }
    if (!scopeTags.includes("已跳过")) scopeTags.push("已跳过")
  } else if (lease && fixture.statusCode === "UNCLAIMED") {
    effectiveStatusCode = "IN_PROGRESS"
    status = { label: "处理中", tone: "info" }
  }

  const claimedByOther = false // single-user session mock; LEASE_CONFLICT covers multi-user demos

  const impact = permissionRevoked
    ? "（权限已收回，敏感影响已清除）"
    : fixture.impact
  const impactSensitive = permissionRevoked
    ? undefined
    : fixture.impactSensitive
  const summaryFields = permissionRevoked
    ? fixture.summaryFields.map((f) =>
        f.label.includes("金额") ||
        f.label.includes("余额") ||
        f.label.includes("回款") ||
        f.label.includes("成本")
          ? { ...f, value: "•••" }
          : f
      )
    : fixture.summaryFields

  const allowedActions = permissionRevoked ? [] : fixture.allowedActions

  return {
    ...fixture,
    impact,
    impactSensitive,
    summaryFields,
    allowedActions,
    scopeTags,
    status,
    effectiveStatusCode,
    leaseState,
    claimedByOther,
    claimedByLabel: lease ? "我" : undefined,
    lastAction,
    permissionRevoked,
    showClose: fixture.closeAllowed && allowedActions.includes("CLOSE"),
  }
}

export async function fetchUnifiedTaskQueue(
  filters: UnifiedQueueFilters
): Promise<UnifiedTaskQueueView> {
  await mockDelay()
  const items = WORK_ITEM_FIXTURES.map(toViewItem).filter(
    (item): item is QueueWorkItemView => item != null
  )

  const mine = items.filter(
    (i) =>
      i.scopeTags.includes("我的待办") || i.responsibleParty.includes("王敏")
  ).length
  const rolePool = items.filter(
    (i) =>
      i.effectiveStatusCode === "UNCLAIMED" || i.status.label === "待领取"
  ).length
  const overdue = items.filter(
    (i) => i.status.tone === "destructive" || i.dueAt.includes("超期")
  ).length

  const familyPart = filters.family
    ? FAMILY_LABELS[filters.family]
    : "全部类型"
  const filterSummary = buildFilterSummary(filters, items.length, familyPart)

  return {
    queueContextId: `queue:W02:${filters.scope}`,
    permissionVersion: getPermissionVersion(),
    permissionRevoked: isPermissionRevoked(),
    freshness: {
      updatedAt: new Date().toISOString(),
      state: "fresh",
    },
    filterSummary,
    total: items.length,
    counts: { mine, rolePool, overdue },
    items,
  }
}

export function useUnifiedTaskQueueQuery(filters: UnifiedQueueFilters) {
  return useQuery({
    queryKey: unifiedQueueKeys.view(filters),
    queryFn: () => fetchUnifiedTaskQueue(filters),
  })
}

function newIdempotencyKey(prefix: string): string {
  return `${prefix}_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`
}

export function useClaimWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      workItemId: string
      subjectVersion: string
      subjectHash: string
      leaseVersion: number
    }): Promise<SessionLease> => {
      await mockDelay(100)
      return claimWorkItemSession(input)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useWorkItemActionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (
      envelope: WorkItemActionEnvelope<{
        kind: "DEFER" | "SAVE_EVIDENCE" | "QUERY_RESULT"
        note?: string
      }> & { simulateTimeout?: boolean }
    ) => {
      await mockDelay(120)
      return applyWorkItemActionSession({
        workItemId: envelope.workItemId,
        claimToken: envelope.claimToken,
        leaseVersion: envelope.leaseVersion,
        expectedSubjectHash: envelope.expectedSubjectHash,
        idempotencyKey: envelope.idempotencyKey,
        action: envelope.action,
        simulateTimeout: envelope.simulateTimeout,
      })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useCompleteWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (
      envelope: CompleteWorkItemEnvelope<{
        kind: string
        note?: string
        summary?: string
      }> & { simulateTimeout?: boolean }
    ): Promise<CompleteSessionResult> => {
      await mockDelay(140)
      return completeWorkItemSession({
        workItemId: envelope.workItemId,
        claimToken: envelope.claimToken,
        leaseVersion: envelope.leaseVersion,
        expectedSubjectHash: envelope.expectedSubjectHash,
        idempotencyKey: envelope.idempotencyKey,
        decision: envelope.decision,
        simulateTimeout: envelope.simulateTimeout,
      })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useCloseWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (envelope: CloseWorkItemEnvelope & { closeAllowed: boolean }) => {
      await mockDelay(120)
      return closeWorkItemSession({
        workItemId: envelope.workItemId,
        claimToken: envelope.claimToken,
        leaseVersion: envelope.leaseVersion,
        expectedSubjectHash: envelope.expectedSubjectHash,
        idempotencyKey: envelope.idempotencyKey,
        closeAllowed: envelope.closeAllowed,
        closure: envelope.closure,
      })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useTransferWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (envelope: TransferWorkItemEnvelope) => {
      await mockDelay(120)
      return transferWorkItemSession({
        workItemId: envelope.workItemId,
        claimToken: envelope.claimToken,
        leaseVersion: envelope.leaseVersion,
        expectedSubjectHash: envelope.expectedSubjectHash,
        idempotencyKey: envelope.idempotencyKey,
        transfer: envelope.transfer,
      })
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useQueryIdempotencyMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      idempotencyKey: string
      /** If pending COMPLETE, finalize recovery path. */
      completeRecovery?: {
        workItemId: string
        decision: { kind: string; note?: string; summary?: string }
        expectedSubjectHash: string
      }
    }) => {
      await mockDelay(80)
      const entry = queryIdempotencyResult(input.idempotencyKey)
      if (!entry) {
        throw new WorkItemMockError("NOT_FOUND", "未找到该任务号的结果。")
      }
      if (entry.state === "pending" && input.completeRecovery) {
        return {
          state: "succeeded" as const,
          payload: finalizePendingComplete({
            idempotencyKey: input.idempotencyKey,
            ...input.completeRecovery,
          }),
        }
      }
      return entry
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useLoseLeaseMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (workItemId: string) => {
      await mockDelay(40)
      loseSessionLease(workItemId)
      clearSessionLease(workItemId)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useBumpSubjectMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (workItemId: string) => {
      await mockDelay(40)
      return bumpWorkItemSubject(workItemId)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useRevokePermissionMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (mode: "revoke" | "restore") => {
      await mockDelay(40)
      if (mode === "revoke") revokeWorkItemPermission()
      else restoreWorkItemPermission()
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export { WorkItemMockError, getIdempotencyEntry, newIdempotencyKey }
