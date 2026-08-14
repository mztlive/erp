/**
 * W20 · API 供应商连接 · 连接详情请求。
 */

import { apiGet } from "@/lib/api"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import {
    type BackendConnectionDetail,
    resolveSupplierName,
    toCenter,
} from "@/features/supplier-api-connections/api/mapping"

export async function fetchConnectionCenter(input: {
    connectionId: string
}): Promise<ConnectionCenterView | null> {
    try {
        const detail = await apiGet<BackendConnectionDetail>(
            `/admin/supplier-api-connections/${encodeURIComponent(input.connectionId)}`,
        )
        return toCenter(detail, await resolveSupplierName(detail.supplier_id))
    } catch (error) {
        const apiError = error as { kind?: string; status?: number }
        if (apiError.kind === "Http" && apiError.status === 404) return null
        throw error
    }
}
