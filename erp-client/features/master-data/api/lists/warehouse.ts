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

export async function listWarehouses(
    query: MasterDataListQuery,
): Promise<MasterDataListItem[]> {
    const status =
        query.lifecycleStatus === "enabled"
            ? "active"
            : query.lifecycleStatus === "disabled"
              ? "disabled"
              : undefined
    const warehouses = await fetchAllPages<WarehouseDto>("/admin/warehouses", {
        status,
        warehouse_code: query.q || undefined,
    })
    const rows: MasterDataListItem[] = []
    for (const wh of warehouses) {
        let revision: WarehouseRevisionDto | undefined
        try {
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
            revision = revPage.items[0]
        } catch {
            // ignore
        }
        rows.push(mapWarehouseRow(wh, revision))
    }
    return rows
}
