import type {
    QueueScopeSlug,
    WorkItemFamily,
} from "@/features/unified-task-queue/types"

const SCOPE_SLUGS: readonly QueueScopeSlug[] = [
    "mine",
    "team",
    "managed",
    "history",
]

const W02_FOCUS_SESSION_KEY = "w02.focus-work-item"

export function readW02FocusId(): string | null {
    if (typeof window === "undefined") return null
    return window.sessionStorage.getItem(W02_FOCUS_SESSION_KEY)
}

export function writeW02FocusId(id: string | null): void {
    if (typeof window === "undefined") return
    if (id) window.sessionStorage.setItem(W02_FOCUS_SESSION_KEY, id)
    else window.sessionStorage.removeItem(W02_FOCUS_SESSION_KEY)
}

export function parseScopeSlug(raw: string | null): QueueScopeSlug {
    return raw && SCOPE_SLUGS.includes(raw as QueueScopeSlug)
        ? (raw as QueueScopeSlug)
        : "mine"
}

export function parseFamily(raw: string | null): WorkItemFamily | undefined {
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

export function parseDue(raw: string | null): "today" | "overdue" | undefined {
    return raw === "today" || raw === "overdue" ? raw : undefined
}

export function parsePriorities(raw: string | null): number[] | undefined {
    if (!raw) return undefined
    const values = raw
        .split(",")
        .map(Number)
        .filter((value) => Number.isInteger(value) && value >= 1 && value <= 4)
    return values.length > 0 ? [...new Set(values)] : undefined
}

export function parseSort(
    raw: string | null,
): "priority_due" | "due_asc" | "created_desc" {
    return raw === "due_asc" || raw === "created_desc" ? raw : "priority_due"
}

export function buildW02SearchParams(options: {
    scope: QueueScopeSlug
    family?: WorkItemFamily | null
    workItemType?: string | null
    due?: "today" | "overdue" | null
    priorities?: readonly number[] | null
    historyStatus?: "COMPLETED" | "CLOSED" | null
    q?: string | null
    sort?: "priority_due" | "due_asc" | "created_desc" | null
    queueContextId?: string | null
    currentWorkItemId?: string | null
    approvalBlockers?: boolean
}): string {
    const params = new URLSearchParams()
    if (options.approvalBlockers) {
        params.set("view", "approval-blockers")
    } else {
        params.set("scope", options.scope)
    }
    if (options.family) params.set("family", options.family)
    if (options.workItemType) params.set("type", options.workItemType)
    if (options.due) params.set("due", options.due)
    if (options.priorities?.length) {
        params.set("priority", options.priorities.join(","))
    }
    if (options.scope === "history" && options.historyStatus) {
        params.set("status", options.historyStatus.toLowerCase())
    }
    if (options.q?.trim()) params.set("q", options.q.trim())
    if (options.sort) params.set("sort", options.sort)
    if (options.queueContextId) {
        params.set("queueContextId", options.queueContextId)
    }
    if (options.currentWorkItemId) {
        params.set("currentWorkItemId", options.currentWorkItemId)
    }
    return `?${params.toString()}`
}

export function scopeLabel(scope: QueueScopeSlug): string {
    return {
        mine: "我的待办",
        team: "团队待处理",
        managed: "团队任务",
        history: "处理历史",
    }[scope]
}
