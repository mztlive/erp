"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import { mockDelay } from "@/lib/mock-delay"
import {
  applyWorkItemActionSession,
  claimWorkItemSession,
  closeWorkItemSession,
  completeWorkItemSession,
  ensureWorkItemSubject,
  getPermissionVersion,
  getSessionLease,
  getWorkItemActionHistory,
  getWorkItemTerminal,
  isPermissionRevoked,
  isWorkItemHeld,
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
  InTaskActionKind,
  QueueWorkItemView,
  UnifiedTaskQueueView,
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
  })

  const held = isWorkItemHeld(fixture.id)
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

export function useClaimWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      workItemId: string
      subjectVersion?: string
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
    mutationFn: async (input: {
      workItemId: string
      expectedSubjectVersion?: string
      ownerUserId?: string
      action: { kind: InTaskActionKind; note?: string }
    }) => {
      await mockDelay(120)
      return applyWorkItemActionSession(input)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useCompleteWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      workItemId: string
      expectedSubjectVersion?: string
      ownerUserId?: string
      decision: { kind: string; note?: string; summary?: string }
    }): Promise<CompleteSessionResult> => {
      await mockDelay(140)
      return completeWorkItemSession(input)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useCloseWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      workItemId: string
      expectedSubjectVersion?: string
      ownerUserId?: string
      closeAllowed: boolean
      closure: {
        kind: "CLOSE_DUPLICATE" | "CLOSE_MISROUTED" | "CLOSE_WITH_REPLACEMENT"
        reasonCode: string
        replacementWorkItemId?: string
        comment?: string
      }
    }) => {
      await mockDelay(120)
      return closeWorkItemSession(input)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export function useTransferWorkItemMutation() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: async (input: {
      workItemId: string
      expectedSubjectVersion?: string
      transfer: { targetUserId: string; reason: string }
    }) => {
      await mockDelay(120)
      return transferWorkItemSession(input)
    },
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: unifiedQueueKeys.all })
    },
  })
}

export { WorkItemMockError }
