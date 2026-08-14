import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { OPTION_PAGE_SIZE, activeStatus } from "./shared"
import type { EntitySearch } from "./types"

type SupplierDto = Readonly<{
    id: string
    supplier_no: string
    party_no?: string | null
    legal_name?: string | null
    short_name?: string | null
    status: string
}>

function supplierItem(row: SupplierDto): SupplierComboboxItem {
    return {
        supplierId: row.id,
        supplierCode: row.supplier_no,
        supplierName:
            row.legal_name?.trim() ||
            row.short_name?.trim() ||
            row.party_no?.trim() ||
            row.supplier_no,
        statusLabel: activeStatus(row.status) ? "启用" : "停用",
        statusTone: activeStatus(row.status) ? "success" : "neutral",
    }
}

export async function searchSuppliers(
    input: EntitySearch,
): Promise<readonly SupplierComboboxItem[]> {
    const page = await apiGet<Page<SupplierDto>>("/admin/suppliers", {
        keyword: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "supplier_no",
        sort_dir: "asc",
    })
    return page.items.map(supplierItem)
}

export async function fetchSupplierOption(
    supplierId: string,
): Promise<SupplierComboboxItem | null> {
    if (!supplierId) return null
    try {
        return supplierItem(
            await apiGet<SupplierDto>(
                `/admin/suppliers/${encodeURIComponent(supplierId)}`,
            ),
        )
    } catch {
        return null
    }
}
