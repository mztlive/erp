import type { WorkItemFamily } from "@/mock/work-items"
import type { QueueScopeSlug } from "./types"

const SCOPE_SLUGS: QueueScopeSlug[] = ["mine", "role_pool", "team", "hold"]

export function parseScopeSlug(raw: string | null): QueueScopeSlug {
  if (raw && SCOPE_SLUGS.includes(raw as QueueScopeSlug)) {
    return raw as QueueScopeSlug
  }
  return "mine"
}

export function parseFamily(
  raw: string | null
): WorkItemFamily | undefined {
  if (
    raw === "approval" ||
    raw === "finance" ||
    raw === "fulfillment" ||
    raw === "exception"
  ) {
    return raw
  }
  return undefined
}

export function parseDue(
  raw: string | null
): "today" | "overdue" | undefined {
  if (raw === "today" || raw === "overdue") return raw
  return undefined
}

export function buildW02SearchParams(options: {
  scope: QueueScopeSlug
  family?: WorkItemFamily | null
  workItemType?: string | null
  due?: "today" | "overdue" | null
  q?: string | null
  currentWorkItemId?: string | null
  queueContextId?: string | null
  converge?: boolean
}): string {
  const params = new URLSearchParams()
  params.set("scope", options.scope)
  if (options.family) params.set("family", options.family)
  if (options.workItemType) params.set("type", options.workItemType)
  if (options.due) params.set("due", options.due)
  if (options.q?.trim()) params.set("q", options.q.trim())
  if (options.currentWorkItemId) {
    params.set("currentWorkItemId", options.currentWorkItemId)
  }
  if (options.queueContextId) {
    params.set("queueContextId", options.queueContextId)
  }
  if (options.converge) params.set("converge", "1")
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}

export function scopeLabel(scope: QueueScopeSlug): string {
  switch (scope) {
    case "mine":
      return "我的待办"
    case "role_pool":
      return "待领取"
    case "team":
      return "团队"
    case "hold":
      return "已跳过"
  }
}
