import type {
    TodayWorkspaceQuery,
    WorkspaceDueFilter,
    WorkspaceFamilyFilter,
    WorkspaceMetricKey,
    WorkspaceSort,
    WorkspaceViewFilter,
} from "@/features/workspace/types"
import { createUrlStateCodec } from "@/lib/url-state"

const DUE_VALUES = ["today", "overdue"] as const
const FAMILY_VALUES = [
    "approval",
    "finance",
    "fulfillment",
    "exception",
] as const
const VIEW_VALUES = ["inbox", "started", "managed"] as const
const SORT_VALUES = ["priority_due", "due_asc", "created_desc"] as const

export const WORKSPACE_LEGAL_QUERY_KEYS = [
    "view",
    "due",
    "blocked",
    "family",
    "type",
    "q",
    "sort",
    "currentWorkItemId",
] as const

export type WorkspaceUrlState = {
    view: WorkspaceViewFilter
    due?: WorkspaceDueFilter
    blocked?: boolean
    family?: WorkspaceFamilyFilter
    workItemType?: string
    query?: string
    sort: WorkspaceSort
    currentWorkItemId?: string
}

const codec = createUrlStateCodec<WorkspaceUrlState>([
    {
        key: "view",
        type: "enum",
        values: VIEW_VALUES,
        defaultValue: "inbox",
    },
    { key: "due", type: "enum", values: DUE_VALUES },
    { key: "blocked", type: "boolean", defaultValue: false },
    { key: "family", type: "enum", values: FAMILY_VALUES },
    { key: "type", name: "workItemType", type: "string", trim: true },
    { key: "q", name: "query", type: "string", trim: true },
    {
        key: "sort",
        type: "enum",
        values: SORT_VALUES,
        defaultValue: "priority_due",
    },
    { key: "currentWorkItemId", type: "string", trim: true },
])

export const parseWorkspaceSearchParams = codec.parse
export const buildWorkspaceSearchParams = codec.build

/**
 * 从旧 `/workspace/tasks` 查询中只保留本页仍合法的键。
 */
export const pickLegalWorkspaceQuery = (
    searchParams:
        | URLSearchParams
        | Record<string, string | string[] | undefined>,
): string => {
    const get =
        searchParams instanceof URLSearchParams
            ? (key: string) => searchParams.get(key)
            : (key: string) => {
                  const value = searchParams[key]
                  return Array.isArray(value)
                      ? (value[0] ?? null)
                      : (value ?? null)
              }
    const next = new URLSearchParams()
    const viewRaw = get("view")
    if (viewRaw === "started" || viewRaw === "managed")
        next.set("view", viewRaw)
    if (viewRaw === "approval-blockers") next.set("blocked", "1")
    const due = get("due")
    if (due === "today" || due === "overdue") next.set("due", due)
    if (get("blocked") === "1" || get("blocked") === "true") {
        next.set("blocked", "1")
    }
    const family = get("family")
    if (family && FAMILY_VALUES.includes(family as WorkspaceFamilyFilter)) {
        next.set("family", family)
    }
    const type = get("type")
    if (type) next.set("type", type)
    const query = get("q")
    if (query) next.set("q", query)
    const sort = get("sort")
    if (sort && SORT_VALUES.includes(sort as WorkspaceSort))
        next.set("sort", sort)
    const current = get("currentWorkItemId")
    if (current) next.set("currentWorkItemId", current)
    const qs = next.toString()
    return qs ? `/workspace?${qs}` : "/workspace"
}

/**
 * 由 URL 推导当前选中的指标。
 */
export function metricKeyFromUrlState(
    state: Pick<WorkspaceUrlState, "view" | "due" | "blocked">,
): WorkspaceMetricKey {
    if (state.view === "started") return "started"
    if (state.blocked) return "blocked"
    if (state.due === "overdue") return "overdue"
    return "inbox"
}

/**
 * 指标点击写入 URL，不跳页。
 */
export function urlStateFromMetricKey(
    key: WorkspaceMetricKey,
    current: WorkspaceUrlState,
): WorkspaceUrlState {
    switch (key) {
        case "overdue":
            return {
                ...current,
                view: "inbox",
                due: "overdue",
                blocked: false,
                currentWorkItemId: undefined,
            }
        case "blocked":
            return {
                ...current,
                view: "inbox",
                due: undefined,
                blocked: true,
                currentWorkItemId: undefined,
            }
        case "started":
            return {
                ...current,
                view: "started",
                due: undefined,
                blocked: false,
                currentWorkItemId: undefined,
            }
        case "inbox":
        default:
            return {
                ...current,
                view: "inbox",
                due: undefined,
                blocked: false,
                currentWorkItemId: undefined,
            }
    }
}

/**
 * URL 状态转列表查询。timezone 必须来自当前工作角色所属组织。
 */
export function toTodayWorkspaceQuery(
    state: WorkspaceUrlState,
    timezone: string,
): TodayWorkspaceQuery {
    return {
        view: state.view,
        due: state.due,
        blocked: state.blocked || undefined,
        family: state.family,
        workItemType: state.workItemType,
        query: state.query,
        sort: state.sort,
        currentWorkItemId: state.currentWorkItemId,
        timezone,
    }
}

/**
 * 左列标题。不得出现「团队待处理」。
 */
export function filterSummaryFor(key: WorkspaceMetricKey): string {
    switch (key) {
        case "overdue":
            return "已超期"
        case "blocked":
            return "受阻"
        case "started":
            return "我发起的审批"
        case "inbox":
        default:
            return "待我处理"
    }
}
