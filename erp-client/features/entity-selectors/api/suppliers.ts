import type { SupplierComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import { fetchSelectorList } from "@/lib/selector-list"

import { activeStatus } from "./shared"
import type { EntitySearch } from "./types"

type SupplierDto = Readonly<{
    id: string
    supplier_no: string
    party_id: string
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
    const page = await fetchSelectorList<SupplierDto>("/admin/suppliers", {
        keyword: input.query.trim() || undefined,
        status: input.purpose === "filter" ? undefined : "active",
        sort_by: "supplier_no",
        sort_dir: "asc",
    })
    return page.items.map(supplierItem)
}

export async function fetchSupplierOption(
    supplierId: string,
    input: Omit<EntitySearch, "query"> = { purpose: "filter" },
): Promise<SupplierComboboxItem | null> {
    if (!supplierId) return null
    const rows = await searchSuppliers({ ...input, query: "" })
    return rows.find((row) => row.supplierId === supplierId) ?? null
}

/** 发票使用往来单位标识，不能使用供应商账户标识。 */
export async function fetchSupplierPartyId(
    supplierId: string,
): Promise<string> {
    const supplier = await apiGet<SupplierDto>(
        `/admin/suppliers/${encodeURIComponent(supplierId)}`,
    )
    if (!supplier.party_id) throw new Error("供应商缺少往来单位标识")
    return supplier.party_id
}
