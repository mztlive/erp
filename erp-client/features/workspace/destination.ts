import {
  getWorkspaceById,
  type WorkspaceId,
} from "@/lib/workspace-registry"
import type { WorkspaceWorkItem } from "@/mock/workspace"

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
 * Primary "处理" navigation: open W02 with the work item focused.
 * One click from W01 task row into the W02 current processor (W02 §12.1 / W01).
 * Specialized W07/W13 handlers are opened from W02, not bypassed from W01.
 */
export function buildProcessHref(item: WorkspaceWorkItem): string {
  const queueContextId = item.queueContextId.includes("W02")
    ? item.queueContextId
    : `queue:W02:mine:${item.family}`
  return resolveWorkspaceHref("W02", {
    scope: "mine",
    family: item.family,
    currentWorkItemId: item.workItemId,
    queueContextId,
  })
}

/** Secondary "查看" navigation when PROCESS is blocked but VIEW is allowed. */
export function buildViewHref(item: WorkspaceWorkItem): string {
  if (item.businessObjectType === "SALES_ORDER" && item.businessObjectId) {
    return `/sales/orders/${item.businessObjectId}`
  }
  return resolveWorkspaceHref(item.destinationWorkspaceId, {
    currentWorkItemId: item.workItemId,
    queueContextId: item.queueContextId,
  })
}

export function buildWarningHref(warning: {
  destinationWorkspaceId: WorkspaceId
  objectId?: string
}): string {
  return resolveWorkspaceHref(warning.destinationWorkspaceId, {
    objectId: warning.objectId,
  })
}
