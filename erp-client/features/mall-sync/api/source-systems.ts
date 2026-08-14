/**
 * 来源系统（D01 source_registry）真实接口。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import type { BackendSourceSystem } from "@/features/mall-sync/api/backend-dtos"
import { mapSourceStatus } from "@/features/mall-sync/api/mappers"
import type {
    SourceSystemItem,
    SourceSystemListParams,
    SourceSystemPage,
    SourceSystemType,
} from "@/features/mall-sync/types"

export const fetchSourceSystems = async (
    params: SourceSystemListParams,
): Promise<SourceSystemPage> => {
    const page = await apiGet<Page<BackendSourceSystem>>(
        "/admin/source-systems",
        {
            page: params.page,
            page_size: params.page_size,
        },
    )
    const items: SourceSystemItem[] = page.items.map((s) => ({
        id: s.id,
        code: s.code,
        name: s.name,
        system_type: s.system_type as SourceSystemType,
        status: mapSourceStatus(s.status),
        created_at: s.created_at,
    }))
    return {
        items,
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }
}
