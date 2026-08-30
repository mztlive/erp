/**
 * W20 · API 供应商连接 · 列表与不透明引用选项请求。
 */

import { apiGet, type Page } from "@/lib/api"
import type { ConnectionListView } from "@/features/supplier-api-connections/types"
import {
    type BackendConnectionListItem,
    secsToIso,
    toListItem,
} from "@/features/supplier-api-connections/api/mapping"

export type ListQueryInput = {
    environment: string
    status?: string
    health?: string
    capability?: string
    catalogFreshness?: string
    supplierId?: string
    q?: string
    page: number
    pageSize?: number
}

export async function fetchConnectionList(
    input: ListQueryInput,
): Promise<ConnectionListView> {
    const page = Math.max(1, input.page)
    const pageSize = input.pageSize ?? 20
    const environment = input.environment.toUpperCase()
    const query: Record<string, unknown> = {
        page,
        page_size: pageSize,
        sort_by: "updated_at",
        sort_dir: "desc",
    }
    if (input.supplierId) query.supplier_id = input.supplierId
    if (input.q?.trim()) query.connection_code = input.q.trim()
    if (environment !== "ALL")
        query.environment =
            environment === "PRODUCTION" ? "production" : "testing"
    if (input.status) {
        const status = input.status.split(",")[0]?.trim().toUpperCase()
        if (status === "ENABLED") query.status = "active"
        if (status === "DISABLED") query.status = "disabled"
        if (status === "FAULTED") query.status = "fault"
    }

    const pageResult = await apiGet<Page<BackendConnectionListItem>>(
        "/admin/supplier-api-connections",
        query,
    )
    const items = pageResult.items.map((connection) =>
        toListItem(
            connection,
            connection.capabilities,
            connection.supplier_name ?? undefined,
        ),
    )
    return {
        metrics: {
            enabled: items.filter((item) => item.status === "ENABLED").length,
            faulted: items.filter((item) => item.status === "FAULTED").length,
            pendingConfig: items.filter(
                (item) => item.status === "PENDING_CONFIG",
            ).length,
            healthAbnormal: items.filter((item) =>
                ["FAILED", "AUTH_FAILED", "PARTIAL", "UNKNOWN"].includes(
                    item.healthResult,
                ),
            ).length,
            catalogStale: 0,
        },
        items,
        total: pageResult.total,
        page: pageResult.page,
        pageSize: pageResult.page_size,
        emptyReason: items.length === 0 ? "NO_CONNECTIONS" : undefined,
        hasModulePermission: true,
        hasDataScope: true,
        projectedAt:
            secsToIso(
                Math.max(0, ...pageResult.items.map((item) => item.created_at)),
            ) ?? new Date(0).toISOString(),
        credentialOpaqueOptions: [],
        endpointOpaqueOptions: [],
    }
}

export async function fetchOpaqueReferenceOptions(
    kind: "credential" | "endpoint",
) {
    const view = await fetchConnectionList({
        environment: "all",
        page: 1,
        pageSize: 1,
    })
    return kind === "credential"
        ? view.credentialOpaqueOptions
        : view.endpointOpaqueOptions
}
