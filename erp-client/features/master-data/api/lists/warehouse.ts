/** 仓库列表适配：逐仓取最新修订（仓库少，不做全量翻页）。 */

import { apiGet } from "@/lib/api"
import type {
    BackendPage,
    WarehouseDto,
    WarehouseRevisionDto,
} from "@/features/master-data/api/contracts"
import { mapWarehouseRow } from "@/features/master-data/api/list-mappers"
import type {
    MasterDataListItem,
    MasterDataListQuery,
} from "@/features/master-data/types"
import { fetchAllPages } from "./fetch-all"
import { lifecycleStatusParam } from "./query-params"

export async function listWarehouses(
    query: MasterDataListQuery,
    canReadRevisions = true,
): Promise<MasterDataListItem[]> {
    const status = lifecycleStatusParam(query)
    const warehouses = await fetchAllPages<WarehouseDto>("/admin/warehouses", {
        status,
        q: query.q || undefined,
    })
    if (!canReadRevisions)
        return warehouses.map((warehouse) => mapWarehouseRow(warehouse))

    const rows: MasterDataListItem[] = []
    for (const wh of warehouses) {
        const revPage = await apiGet<BackendPage<WarehouseRevisionDto>>(
            "/admin/warehouse-revisions",
            {
                warehouse_id: wh.id,
                page: 1,
                page_size: 1,
                sort_by: "revision_no",
                sort_dir: "desc",
            },
        )
        const revision = revPage.items[0]
        rows.push(mapWarehouseRow(wh, revision))
    }
    return rows
}
