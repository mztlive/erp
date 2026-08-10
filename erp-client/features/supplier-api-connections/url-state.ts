import type {
    ConnectionEnvironment,
    ConnectionSection,
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
}

const ENV_VALUES = ["DEVELOPMENT", "STAGING", "PRODUCTION", "ALL"] as const

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
])

export const parseConnectionsSearchParams = codec.parse
export const buildConnectionsSearchParams = codec.build
