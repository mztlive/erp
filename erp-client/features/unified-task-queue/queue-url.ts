import type { QueueScopeSlug, WorkItemFamily } from "./types"

const SCOPE_SLUGS: QueueScopeSlug[] = ["mine", "role_pool", "team", "hold"]

/**
 * 当前任务焦点不落地址栏（内部 ID 禁止进 URL），经 sessionStorage 传递，
 * 支持 W01 等来源页的深链聚焦（P2-5 / 内部 ID 清零契约）。
 */
export const W02_FOCUS_SESSION_KEY = "w02.focus-work-item"

export function readW02FocusId(): string | null {
  if (typeof window === "undefined") return null
  return window.sessionStorage.getItem(W02_FOCUS_SESSION_KEY)
}

export function writeW02FocusId(id: string | null): void {
  if (typeof window === "undefined") return
  if (id) {
    window.sessionStorage.setItem(W02_FOCUS_SESSION_KEY, id)
  } else {
    window.sessionStorage.removeItem(W02_FOCUS_SESSION_KEY)
  }
}

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
    raw === "exception" ||
    raw === "procurement"
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
  converge?: boolean
}): string {
  const params = new URLSearchParams()
  params.set("scope", options.scope)
  if (options.family) params.set("family", options.family)
  if (options.workItemType) params.set("type", options.workItemType)
  if (options.due) params.set("due", options.due)
  if (options.q?.trim()) params.set("q", options.q.trim())
  if (options.converge) params.set("converge", "1")
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}

export function scopeLabel(scope: QueueScopeSlug): string {
  switch (scope) {
    case "mine":
      return "我的待办"
    case "role_pool":
      return "团队待认领"
    case "team":
      return "团队"
    case "hold":
      return "已跳过"
  }
}
