import type { QueueWorkItemView, UnifiedQueueFilters } from "./types"

function isOverdue(item: QueueWorkItemView): boolean {
  return (
    item.status.tone === "destructive" ||
    item.dueAt.includes("超期") ||
    item.status.label.includes("超期")
  )
}

function isToday(item: QueueWorkItemView): boolean {
  return item.dueAt.includes("今天") && !isOverdue(item)
}

/**
 * Client-side filter for session-mock queue.
 * Sort: overdue → priority asc (1=urgent) → dueAt → createdAt (enteredDateTime).
 */
export function filterAndSortWorkItems(
  items: readonly QueueWorkItemView[],
  filters: UnifiedQueueFilters,
  options?: {
    /** When converging, keep same type or processor group as focus item. */
    focus?: QueueWorkItemView | null
  }
): QueueWorkItemView[] {
  let result = [...items]

  // Scope
  if (filters.scope === "mine") {
    result = result.filter(
      (item) =>
        item.scopeTags.includes("我的待办") ||
        (item.effectiveStatusCode !== "UNCLAIMED" &&
          item.responsibleParty.includes("王敏"))
    )
  } else if (filters.scope === "role_pool") {
    result = result.filter(
      (item) =>
        item.scopeTags.includes("待领取") ||
        item.effectiveStatusCode === "UNCLAIMED" ||
        item.status.label === "待领取"
    )
  } else if (filters.scope === "team") {
    result = result.filter((item) => item.scopeTags.includes("团队"))
  } else if (filters.scope === "hold") {
    result = result.filter(
      (item) =>
        item.status.label === "已暂挂" || item.scopeTags.includes("已暂挂")
    )
  }

  if (filters.family) {
    result = result.filter((item) => item.family === filters.family)
  }

  if (filters.workItemType) {
    result = result.filter((item) => item.workItemType === filters.workItemType)
  }

  if (filters.due === "overdue") {
    result = result.filter(isOverdue)
  } else if (filters.due === "today") {
    result = result.filter(isToday)
  }

  if (filters.query?.trim()) {
    const q = filters.query.trim().toLowerCase()
    result = result.filter(
      (item) =>
        item.id.toLowerCase().includes(q) ||
        item.businessObject.toLowerCase().includes(q) ||
        item.counterparty.toLowerCase().includes(q) ||
        item.workItemTypeLabel.toLowerCase().includes(q)
    )
  }

  // Formal continuous process: converge to single type or compatible processor group
  if (filters.converge && options?.focus) {
    const focus = options.focus
    result = result.filter(
      (item) =>
        item.workItemType === focus.workItemType ||
        item.processorGroup === focus.processorGroup
    )
  }

  result.sort((a, b) => {
    const aOver = isOverdue(a) ? 0 : 1
    const bOver = isOverdue(b) ? 0 : 1
    if (aOver !== bOver) return aOver - bOver
    if (a.priority !== b.priority) return a.priority - b.priority
    const due = a.dueDateTime.localeCompare(b.dueDateTime)
    if (due !== 0) return due
    return a.enteredDateTime.localeCompare(b.enteredDateTime)
  })

  return result
}

export function buildFilterSummary(
  filters: UnifiedQueueFilters,
  total: number,
  focusLabel?: string
): string {
  const parts: string[] = []
  const scopeLabel =
    filters.scope === "mine"
      ? "我的待办"
      : filters.scope === "role_pool"
        ? "待领取"
        : filters.scope === "team"
          ? "团队"
          : "已暂挂"
  parts.push(scopeLabel)
  if (filters.family) parts.push(`族:${filters.family}`)
  if (filters.workItemType) parts.push(`类型已收敛`)
  else if (filters.converge && focusLabel) parts.push(`连续处理·${focusLabel}`)
  else parts.push("全部类型")
  if (filters.due === "overdue") parts.push("已超期")
  if (filters.due === "today") parts.push("今日到期")
  if (filters.query?.trim()) parts.push(`搜索“${filters.query.trim()}”`)
  parts.push(`共 ${total} 项`)
  return parts.join(" · ")
}
