import type { WarehouseComboboxItem } from "@/components/business/entity-comboboxes"
import {
    searchObjectDirectory,
    selectedObjectDirectory,
    type ObjectDirectoryItem,
} from "@/lib/object-directory"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { fetchSelectorList, type SelectorPage } from "@/lib/selector-list"
import { activeStatus } from "./shared"
import type { EntitySearch } from "./types"

type WarehouseDto = Readonly<{
    id: string
    warehouse_code: string
    current_revision_id?: string | null
    status: string
    inbound_handler_user_id?: string | null
    outbound_handler_user_id?: string | null
}>

type WarehouseRevisionDto = Readonly<{
    id: string
    name: string
}>

async function warehouseItem(
    row: WarehouseDto,
): Promise<WarehouseComboboxItem> {
    const revisions = await fetchSelectorList<WarehouseRevisionDto>(
        "/admin/warehouse-revisions",
        { warehouse_id: row.id },
    )
    const warehouseName =
        revisions.items
            .find((revision) => revision.id === row.current_revision_id)
            ?.name?.trim() || row.warehouse_code
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
): Promise<SelectorPage<WarehouseComboboxItem>> {
    if (input.purpose === "filter") {
        const page = await searchObjectDirectory("warehouse-directory", input.query)
        return { ...page, items: page.items.map(directoryItem) }
    }
    const page = await fetchSelectorList<WarehouseDto>("/admin/warehouses", {
        q: input.query.trim() || undefined,
        require_inbound_handler:
            input.purpose === "purchase-receipt" || undefined,
        status: "active",
        sort_by: "warehouse_code",
        sort_dir: "asc",
    })
    return { ...page, items: await Promise.all(page.items.map(warehouseItem)) }
}

export async function fetchWarehouseOption(
    warehouseId: string,
    purpose: EntitySearch["purpose"] = "filter",
): Promise<WarehouseComboboxItem | null> {
    if (!warehouseId) return null
    if (purpose === "filter") {
        const row = await selectedObjectDirectory(
            "warehouse-directory",
            warehouseId,
        )
        return row ? directoryItem(row) : null
    }
    const page = await apiGet<Page<WarehouseDto>>("/admin/warehouses", {
        warehouse_id: warehouseId,
        require_inbound_handler: purpose === "purchase-receipt" || undefined,
        status: "active",
        page: 1,
        page_size: 1,
    })
    const row = page.items.find((item) => item.id === warehouseId)
    if (
        purpose === "purchase-receipt" &&
        !row?.inbound_handler_user_id?.trim()
    ) {
        return null
    }
    return row ? warehouseItem(row) : null
}

function directoryItem(row: ObjectDirectoryItem): WarehouseComboboxItem {
    const enabled = activeStatus(row.status)
    return {
        warehouseId: row.id,
        warehouseCode: row.code,
        warehouseName: row.name,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}
