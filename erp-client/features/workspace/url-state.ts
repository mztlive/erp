import type {
  TodayWorkspaceQuery,
  WorkspaceDueFilter,
  WorkspaceFamilyFilter,
  WorkspaceMetricKey,
} from "@/mock/workspace"

const DUE_VALUES = new Set<WorkspaceDueFilter>(["today", "overdue"])
const FAMILY_VALUES = new Set<WorkspaceFamilyFilter>([
  "approval",
  "finance",
  "fulfillment",
  "exception",
])
const SCENARIO_VALUES = new Set(["forbidden", "no_scope", "empty"] as const)

export type WorkspaceUrlState = {
  scope: "mine" | "role_pool"
  due?: WorkspaceDueFilter
  family?: WorkspaceFamilyFilter
  focusWorkItemId?: string
  /** Mock-only QA override; not part of shareable filter UX. */
  scenario?: "forbidden" | "no_scope" | "empty"
}

export function parseWorkspaceSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): WorkspaceUrlState {
  const scopeRaw = searchParams.get("scope")
  const scope = scopeRaw === "role_pool" ? "role_pool" : "mine"

  const dueRaw = searchParams.get("due")
  const due =
    dueRaw && DUE_VALUES.has(dueRaw as WorkspaceDueFilter)
      ? (dueRaw as WorkspaceDueFilter)
      : undefined

  const familyRaw = searchParams.get("family")
  const family =
    familyRaw && FAMILY_VALUES.has(familyRaw as WorkspaceFamilyFilter)
      ? (familyRaw as WorkspaceFamilyFilter)
      : undefined

  const focusWorkItemId = searchParams.get("focusWorkItemId") ?? undefined

  const scenarioRaw = searchParams.get("scenario")
  const scenario =
    scenarioRaw &&
    (SCENARIO_VALUES as Set<string>).has(scenarioRaw)
      ? (scenarioRaw as WorkspaceUrlState["scenario"])
      : undefined

  return { scope, due, family, focusWorkItemId, scenario }
}

export function buildWorkspaceSearchParams(
  state: WorkspaceUrlState
): string {
  const params = new URLSearchParams()
  // Default scope=mine is omitted so `/workspace` stays clean.
  if (state.scope === "role_pool") params.set("scope", "role_pool")
  if (state.due) params.set("due", state.due)
  if (state.family) params.set("family", state.family)
  if (state.focusWorkItemId) params.set("focusWorkItemId", state.focusWorkItemId)
  if (state.scenario) params.set("scenario", state.scenario)
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}

export function metricKeyFromUrlState(
  state: Pick<WorkspaceUrlState, "due" | "family">
): WorkspaceMetricKey {
  if (state.due === "today") return "due_today"
  if (state.due === "overdue") return "overdue"
  if (state.family === "exception") return "exception"
  return "mine"
}

export function urlStateFromMetricKey(
  key: WorkspaceMetricKey,
  current: WorkspaceUrlState
): WorkspaceUrlState {
  switch (key) {
    case "due_today":
      return {
        ...current,
        due: "today",
        family: undefined,
        focusWorkItemId: undefined,
      }
    case "overdue":
      return {
        ...current,
        due: "overdue",
        family: undefined,
        focusWorkItemId: undefined,
      }
    case "exception":
      return {
        ...current,
        due: undefined,
        family: "exception",
        focusWorkItemId: undefined,
      }
    case "mine":
    default:
      return {
        ...current,
        due: undefined,
        family: undefined,
        focusWorkItemId: undefined,
      }
  }
}

export function toTodayWorkspaceQuery(
  state: WorkspaceUrlState,
  timezone: string
): TodayWorkspaceQuery {
  return {
    scope: state.scope,
    due: state.due,
    family: state.family,
    timezone,
    scenario: state.scenario,
  }
}

/** Build W02 queue URL carrying current W01 filter context. */
export function buildTaskQueueHref(state: WorkspaceUrlState): string {
  const params = new URLSearchParams()
  params.set("scope", state.scope)
  if (state.due) params.set("due", state.due)
  if (state.family) params.set("family", state.family)
  const qs = params.toString()
  return qs ? `/workspace/tasks?${qs}` : "/workspace/tasks"
}

export function buildGroupAllHref(
  state: WorkspaceUrlState,
  family: WorkspaceFamilyFilter
): string {
  return buildTaskQueueHref({ ...state, family })
}

export const FILTER_SUMMARY: Record<WorkspaceMetricKey, string> = {
  mine: "待我处理",
  due_today: "今日到期",
  overdue: "已超期",
  exception: "同步异常",
}
