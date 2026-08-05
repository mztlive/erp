import type {
  TodayWorkspaceQuery,
  WorkspaceDueFilter,
  WorkspaceFamilyFilter,
  WorkspaceMetricKey,
} from "@/mock/workspace"
import { sequentialText } from "@/lib/ui-text"
import { createUrlStateCodec } from "@/lib/url-state"

const DUE_VALUES = ["today", "overdue"] as const
const FAMILY_VALUES = ["approval", "finance", "fulfillment", "exception"] as const
const SCENARIO_VALUES = ["forbidden", "no_scope", "empty"] as const

export type WorkspaceUrlState = {
  scope: "mine" | "role_pool"
  due?: WorkspaceDueFilter
  family?: WorkspaceFamilyFilter
  focusWorkItemId?: string
  /** Mock-only QA override; not part of shareable filter UX. */
  scenario?: "forbidden" | "no_scope" | "empty"
}

const codec = createUrlStateCodec<WorkspaceUrlState>([
  // Default scope=mine is omitted so `/workspace` stays clean.
  { key: "scope", type: "enum", values: ["mine", "role_pool"], defaultValue: "mine" },
  { key: "due", type: "enum", values: DUE_VALUES },
  { key: "family", type: "enum", values: FAMILY_VALUES },
  { key: "focusWorkItemId", type: "string" },
  { key: "scenario", type: "enum", values: SCENARIO_VALUES },
])

export const parseWorkspaceSearchParams = codec.parse
export const buildWorkspaceSearchParams = codec.build

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
  mine: sequentialText.minePending,
  due_today: "今日到期",
  overdue: "已超期",
  exception: "同步异常",
}
