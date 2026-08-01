/**
 * W29 URL 状态：view / mode / errorClass / environment / owner /
 * current task|difference / autoNext / resolveWorkItemId / queueContextId
 */

import type {
  IntegrationEnvironment,
  IntegrationMode,
  IntegrationOwnerFilter,
  IntegrationResolutionQuery,
  IntegrationView,
} from "./types"

export type IntegrationUrlState = {
  view: IntegrationView
  mode: IntegrationMode
  environment: IntegrationEnvironment | "all"
  errorClass?: string
  owner: IntegrationOwnerFilter
  q?: string
  queueContextId: string
  resolveWorkItemId?: string
  currentTaskId?: string
  currentDifferenceId?: string
  autoNext: boolean
}

const VIEWS: IntegrationView[] = [
  "mine",
  "result_unknown",
  "security",
  "auto_retry",
  "reconciliation",
  "resolved",
]

const MODES: IntegrationMode[] = ["all", "errors", "reconciliation"]
const ENVS = ["all", "production", "verification"] as const
const OWNERS: IntegrationOwnerFilter[] = ["me", "role_pool", "claimed", "all"]

export function parseIntegrationSearchParams(
  params: URLSearchParams
): IntegrationUrlState {
  const viewRaw = params.get("view") ?? "mine"
  const view = VIEWS.includes(viewRaw as IntegrationView)
    ? (viewRaw as IntegrationView)
    : "mine"

  const modeRaw = params.get("mode") ?? "all"
  const mode = MODES.includes(modeRaw as IntegrationMode)
    ? (modeRaw as IntegrationMode)
    : "all"

  const envRaw = params.get("environment") ?? "production"
  const environment = ENVS.includes(envRaw as (typeof ENVS)[number])
    ? (envRaw as IntegrationEnvironment | "all")
    : "production"

  const ownerRaw = params.get("owner") ?? "me"
  const owner = OWNERS.includes(ownerRaw as IntegrationOwnerFilter)
    ? (ownerRaw as IntegrationOwnerFilter)
    : "me"

  const autoNextParam = params.get("autoNext")
  const autoNext =
    autoNextParam === "0" ? false : autoNextParam === "1" ? true : true

  return {
    view,
    mode,
    environment,
    errorClass: params.get("errorClass") ?? undefined,
    owner,
    q: params.get("q") ?? undefined,
    queueContextId:
      params.get("queueContextId") ?? `queue:W29:${view}:${mode}`,
    resolveWorkItemId: params.get("resolveWorkItemId") ?? undefined,
    currentTaskId: params.get("taskId") ?? params.get("currentTaskId") ?? undefined,
    currentDifferenceId:
      params.get("differenceId") ??
      params.get("currentDifferenceId") ??
      undefined,
    autoNext,
  }
}

export function buildIntegrationSearchParams(
  state: Partial<IntegrationUrlState> & Pick<IntegrationUrlState, "view">
): URLSearchParams {
  const params = new URLSearchParams()
  params.set("view", state.view)
  if (state.mode && state.mode !== "all") params.set("mode", state.mode)
  if (state.environment && state.environment !== "production") {
    params.set("environment", state.environment)
  }
  if (state.errorClass) params.set("errorClass", state.errorClass)
  if (state.owner && state.owner !== "me") params.set("owner", state.owner)
  if (state.q) params.set("q", state.q)
  if (state.queueContextId) params.set("queueContextId", state.queueContextId)
  if (state.resolveWorkItemId) {
    params.set("resolveWorkItemId", state.resolveWorkItemId)
  }
  if (state.currentTaskId) params.set("taskId", state.currentTaskId)
  if (state.currentDifferenceId) {
    params.set("differenceId", state.currentDifferenceId)
  }
  if (state.autoNext === false) params.set("autoNext", "0")
  if (state.autoNext === true) params.set("autoNext", "1")
  return params
}

export function toResolutionQuery(
  state: IntegrationUrlState
): IntegrationResolutionQuery {
  return {
    view: state.view,
    mode: state.mode,
    environment: state.environment,
    errorClass: state.errorClass,
    owner: state.owner,
    q: state.q,
    queueContextId: state.queueContextId,
    resolveWorkItemId: state.resolveWorkItemId,
    currentTaskId: state.currentTaskId,
    currentDifferenceId: state.currentDifferenceId,
    autoNext: state.autoNext,
  }
}
