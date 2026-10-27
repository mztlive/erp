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

export function buildSupplierCatalogHref(connectionId: string): string {
    return `/supplier-api/catalog?connectionId=${encodeURIComponent(connectionId)}`
}

export function buildSupplierConnectionSettingsHref(
    connectionId: string,
): string {
    return `/supplier-api/connections/${encodeURIComponent(connectionId)}`
}
