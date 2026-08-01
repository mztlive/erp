import type {
  ConnectionEnvironment,
  ConnectionSection,
  DemoRole,
  HealthResult,
} from "@/features/supplier-api-connections/types"
import { SECTIONS } from "@/features/supplier-api-connections/types"

export type ConnectionsUrlState = {
  environment: ConnectionEnvironment | "ALL"
  status?: string
  health?: string
  capability?: string
  catalogFreshness?: string
  supplierId?: string
  q?: string
  page: number
  connectionId?: string
  section: ConnectionSection
  role: DemoRole
  /** 演示：无模块权限 / 无数据范围 */
  demoFlag?: "no-permission" | "no-scope"
}

const ENV_SET = new Set(["DEVELOPMENT", "STAGING", "PRODUCTION", "ALL"])
const ROLE_SET = new Set(["procurement", "ops", "admin"])

export function parseConnectionsSearchParams(
  searchParams: URLSearchParams | { get(name: string): string | null }
): ConnectionsUrlState {
  const envRaw = (searchParams.get("environment") ?? "PRODUCTION").toUpperCase()
  const environment: ConnectionEnvironment | "ALL" = ENV_SET.has(envRaw)
    ? (envRaw as ConnectionEnvironment | "ALL")
    : "PRODUCTION"

  const status = searchParams.get("status") ?? undefined
  const health = searchParams.get("health") ?? undefined
  const capability = searchParams.get("capability") ?? undefined
  const catalogFreshness = searchParams.get("catalogFreshness") ?? undefined
  const supplierId = searchParams.get("supplierId") ?? undefined
  const q = searchParams.get("q") ?? undefined
  const connectionId =
    searchParams.get("connectionId") ?? searchParams.get("id") ?? undefined

  const sectionRaw = searchParams.get("section")
  const section: ConnectionSection =
    sectionRaw && (SECTIONS as string[]).includes(sectionRaw)
      ? (sectionRaw as ConnectionSection)
      : "overview"

  const pageRaw = Number(searchParams.get("page") ?? "1")
  const page =
    Number.isFinite(pageRaw) && pageRaw >= 1 ? Math.floor(pageRaw) : 1

  const roleRaw = searchParams.get("role") ?? searchParams.get("demoRole")
  const role: DemoRole =
    roleRaw && ROLE_SET.has(roleRaw) ? (roleRaw as DemoRole) : "admin"

  const flagRaw = searchParams.get("demoFlag")
  const demoFlag =
    flagRaw === "no-permission" || flagRaw === "no-scope"
      ? flagRaw
      : undefined

  return {
    environment,
    status,
    health,
    capability,
    catalogFreshness,
    supplierId,
    q,
    page,
    connectionId,
    section,
    role,
    demoFlag,
  }
}

export function buildConnectionsSearchParams(
  state: ConnectionsUrlState
): string {
  const params = new URLSearchParams()
  if (state.environment !== "PRODUCTION") {
    params.set("environment", state.environment)
  }
  if (state.status) params.set("status", state.status)
  if (state.health) params.set("health", state.health)
  if (state.capability) params.set("capability", state.capability)
  if (state.catalogFreshness) {
    params.set("catalogFreshness", state.catalogFreshness)
  }
  if (state.supplierId) params.set("supplierId", state.supplierId)
  if (state.q?.trim()) params.set("q", state.q.trim())
  if (state.page > 1) params.set("page", String(state.page))
  if (state.connectionId) {
    params.set("connectionId", state.connectionId)
    if (state.section !== "overview") params.set("section", state.section)
  }
  if (state.role !== "admin") params.set("role", state.role)
  if (state.demoFlag) params.set("demoFlag", state.demoFlag)
  const qs = params.toString()
  return qs ? `?${qs}` : ""
}

export function parseHealthFilter(raw?: string): HealthResult | undefined {
  if (
    raw === "SUCCESS" ||
    raw === "FAILED" ||
    raw === "PARTIAL" ||
    raw === "UNCHECKED" ||
    raw === "STALE" ||
    raw === "AUTH_FAILED" ||
    raw === "UNKNOWN"
  ) {
    return raw
  }
  return undefined
}
