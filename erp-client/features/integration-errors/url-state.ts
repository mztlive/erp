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
import { createUrlStateCodec } from "@/lib/url-state"

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

const VIEW_VALUES = [
  "mine",
  "result_unknown",
  "security",
  "auto_retry",
  "reconciliation",
  "resolved",
] as const

const MODE_VALUES = ["all", "errors", "reconciliation"] as const
const ENV_VALUES = ["all", "production", "verification"] as const
const OWNER_VALUES = ["me", "role_pool", "claimed", "all"] as const

const codec = createUrlStateCodec<IntegrationUrlState>([
  {
    key: "view",
    type: "enum",
    values: VIEW_VALUES,
    defaultValue: "mine",
    buildWhen: () => true,
  },
  { key: "mode", type: "enum", values: MODE_VALUES, defaultValue: "all" },
  {
    key: "environment",
    type: "enum",
    values: ENV_VALUES,
    defaultValue: "production",
  },
  { key: "errorClass", type: "string" },
  { key: "owner", type: "enum", values: OWNER_VALUES, defaultValue: "me" },
  { key: "q", type: "string" },
  {
    key: "queueContextId",
    type: "custom",
    parse: (get, state) =>
      get("queueContextId") ??
      `queue:W29:${String(state.view)}:${String(state.mode)}`,
    build: (value) => (value ? String(value) : undefined),
  },
  { key: "resolveWorkItemId", type: "string" },
  { key: "taskId", name: "currentTaskId", type: "string", aliases: ["currentTaskId"] },
  {
    key: "differenceId",
    name: "currentDifferenceId",
    type: "string",
    aliases: ["currentDifferenceId"],
  },
  { key: "autoNext", type: "boolean", defaultValue: true },
])

export const parseIntegrationSearchParams = codec.parse

export function buildIntegrationSearchParams(
  state: Partial<IntegrationUrlState> & Pick<IntegrationUrlState, "view">
): URLSearchParams {
  return codec.buildParams(state as IntegrationUrlState)
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
