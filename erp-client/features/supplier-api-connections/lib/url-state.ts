import type { ConnectionSection } from "@/features/supplier-api-connections/types"
import { SECTIONS } from "@/features/supplier-api-connections/types"
import { createUrlStateCodec } from "@/lib/url-state"

export type ConnectionsUrlState = {
    status?: string
    health?: string
    capability?: string
    catalogFreshness?: string
    supplierId?: string
    q?: string
    page: number
    pageSize: number
}

const codec = createUrlStateCodec<ConnectionsUrlState>([
    { key: "status", type: "string" },
    { key: "health", type: "string" },
    { key: "capability", type: "string" },
    { key: "catalogFreshness", type: "string" },
    { key: "supplierId", type: "string" },
    { key: "q", type: "string", trim: true },
    { key: "page", type: "number", defaultValue: 1 },
    { key: "pageSize", type: "number", defaultValue: 20, min: 20, max: 100 },
])

export const parseConnectionsSearchParams = codec.parse
export const buildConnectionsSearchParams = codec.build

const detailCodec = createUrlStateCodec<{ section: ConnectionSection }>([
    {
        key: "section",
        type: "enum",
        values: SECTIONS,
        defaultValue: "catalog",
    },
])

export const parseConnectionDetailSearchParams = detailCodec.parse

export function buildConnectionDetailHref(
    connectionId: string,
    listState: ConnectionsUrlState,
    section: ConnectionSection = "catalog",
): string {
    const params = new URLSearchParams(buildConnectionsSearchParams(listState))
    if (section !== "catalog") params.set("section", section)
    const query = params.toString()
    const path = `/supplier-api/connections/${encodeURIComponent(connectionId)}`
    return query ? `${path}?${query}` : path
}
