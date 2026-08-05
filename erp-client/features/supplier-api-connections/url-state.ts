import type {
  ConnectionEnvironment,
  ConnectionSection,
  DemoRole,
  HealthResult,
} from "@/features/supplier-api-connections/types"
import { SECTIONS } from "@/features/supplier-api-connections/types"
import { createUrlStateCodec } from "@/lib/url-state"

export type ConnectionsUrlState = {
  environment: ConnectionEnvironment | "ALL"
  status?: string
  health?: string
  capability?: string
  catalogFreshness?: string
  supplierId?: string
  q?: string
  page: number
  pageSize: number
  connectionId?: string
  section: ConnectionSection
  role: DemoRole
  /** 演示：无模块权限 / 无数据范围 */
  demoFlag?: "no-permission" | "no-scope"
}

const ENV_VALUES = ["DEVELOPMENT", "STAGING", "PRODUCTION", "ALL"] as const
const ROLE_VALUES = ["procurement", "ops", "admin"] as const
const FLAG_VALUES = ["no-permission", "no-scope"] as const

const codec = createUrlStateCodec<ConnectionsUrlState>([
  {
    key: "environment",
    type: "enum",
    values: ENV_VALUES,
    defaultValue: "PRODUCTION",
    normalize: (raw) => raw.toUpperCase(),
  },
  { key: "status", type: "string" },
  { key: "health", type: "string" },
  { key: "capability", type: "string" },
  { key: "catalogFreshness", type: "string" },
  { key: "supplierId", type: "string" },
  { key: "q", type: "string", trim: true },
  { key: "page", type: "number", defaultValue: 1 },
  { key: "pageSize", type: "number", defaultValue: 20, min: 20, max: 100 },
  { key: "connectionId", type: "string", aliases: ["id"] },
  {
    key: "section",
    type: "enum",
    values: SECTIONS,
    defaultValue: "overview",
    buildWhen: (value, state) =>
      value !== "overview" && Boolean(state.connectionId),
  },
  {
    key: "role",
    type: "enum",
    values: ROLE_VALUES,
    defaultValue: "admin",
    aliases: ["demoRole"],
  },
  { key: "demoFlag", type: "enum", values: FLAG_VALUES },
])

export const parseConnectionsSearchParams = codec.parse
export const buildConnectionsSearchParams = codec.build

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
