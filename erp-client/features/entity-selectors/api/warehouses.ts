import type { WarehouseComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { OPTION_PAGE_SIZE, activeStatus } from "./shared"
import type { EntitySearch } from "./types"

type WarehouseDto = Readonly<{
    id: string
    warehouse_code: string
    status: string
}>

type WarehouseRevisionDto = Readonly<{
    warehouse_name: string
}>

async function warehouseItem(
    row: WarehouseDto,
): Promise<WarehouseComboboxItem> {
    let warehouseName = row.warehouse_code
    try {
        const revisions = await apiGet<Page<WarehouseRevisionDto>>(
            "/admin/warehouse-revisions",
            {
                warehouse_id: row.id,
                page: 1,
                page_size: 1,
                sort_by: "revision_no",
                sort_dir: "desc",
            },
        )
        warehouseName =
            revisions.items[0]?.warehouse_name?.trim() || row.warehouse_code
    } catch {
        // 仓库代码是稳定且可展示的回退值。
    }
    const enabled = activeStatus(row.status)
    return {
        warehouseId: row.id,
        warehouseCode: row.warehouse_code,
        warehouseName,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}

export async function searchWarehouses(
    input: EntitySearch,
): Promise<readonly WarehouseComboboxItem[]> {
    const page = await apiGet<Page<WarehouseDto>>("/admin/warehouses", {
        warehouse_code: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "warehouse_code",
        sort_dir: "asc",
    })
    return Promise.all(page.items.map(warehouseItem))
}

export async function fetchWarehouseOption(
    warehouseId: string,
): Promise<WarehouseComboboxItem | null> {
    if (!warehouseId) return null
    const page = await apiGet<Page<WarehouseDto>>("/admin/warehouses", {
        status: "active",
        page: 1,
        page_size: 100,
    })
    const row = page.items.find((item) => item.id === warehouseId)
    return row ? warehouseItem(row) : null
}
