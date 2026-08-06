import {
  getWorkspaceById,
  type WorkspaceId,
} from "@/lib/workspace-registry"
import { writeW02FocusId } from "@/features/unified-task-queue/queue-url"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

/**
 * Resolve a safe in-app destination from the local workspace registry.
 * Server may only return workspace ids — never raw external URLs (W01 §7.1).
 */
export function resolveWorkspaceHref(
  workspaceId: WorkspaceId,
  query?: Record<string, string | undefined>
): string {
  const entry = getWorkspaceById(workspaceId)
  const base = entry.navHref
  if (!query) return base

  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(query)) {
    if (value) params.set(key, value)
  }

  // Preserve any existing query on navHref (e.g. W06 acceptance section).
  const [path, existingQs] = base.split("?")
  if (existingQs) {
    const existing = new URLSearchParams(existingQs)
    existing.forEach((value, key) => {
      if (!params.has(key)) params.set(key, value)
    })
  }
  const qs = params.toString()
  return qs ? `${path}?${qs}` : path
}

/**
 * Primary action navigation: open W02 with the work item focused.
 * One click from W01 task row into the W02 current processor (W02 §12.1 / W01).
 * 任务身份不落地址栏（内部 ID 禁止进 URL）；焦点经 sessionStorage 传给 W02。
 * Specialized W07/W13 handlers are opened from W02, not bypassed from W01.
 */
export function buildProcessHref(item: WorkspaceWorkItem): string {
  writeW02FocusId(item.workItemId)
  return resolveWorkspaceHref("W02", {
    scope: "mine",
    family: item.family,
  })
}

/** Secondary "查看" navigation when PROCESS is blocked but VIEW is allowed. */
export function buildViewHref(item: WorkspaceWorkItem): string {
  return resolveWorkspaceHref(item.destinationWorkspaceId)
}

export function buildWarningHref(warning: {
  destinationWorkspaceId: WorkspaceId
  objectId?: string
}): string {
  return resolveWorkspaceHref(warning.destinationWorkspaceId, {
    objectId: warning.objectId,
  })
}
