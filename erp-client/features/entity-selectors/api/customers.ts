import type { CustomerComboboxItem } from "@/components/business/entity-comboboxes"
import { fetchSelectorList, type SelectorPage } from "@/lib/selector-list"

import { activeStatus } from "./shared"
import type { CustomerSearch } from "./types"

type CustomerDto = Readonly<{
    id: string
    customer_no: string
    legal_name?: string | null
    short_name?: string | null
    party_no?: string | null
    status: string
    owner_user_id?: string | null
    owner_user_name?: string | null
}>

function customerItem(row: CustomerDto): CustomerComboboxItem {
    const enabled = activeStatus(row.status)
    return {
        id: row.id,
        customerNo: row.customer_no,
        legalName:
            row.legal_name?.trim() || row.party_no?.trim() || row.customer_no,
        shortName: row.short_name?.trim() || undefined,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
        ownerName: row.owner_user_name ?? row.owner_user_id ?? undefined,
    }
}

export async function searchCustomers(
    input: CustomerSearch,
): Promise<SelectorPage<CustomerComboboxItem>> {
    const path =
        input.scope === "all_authorized"
            ? "/admin/customers/all-authorized"
            : "/admin/customers"
    const page = await fetchSelectorList<CustomerDto>(path, {
        scope: input.scope,
        keyword: input.query.trim() || undefined,
        status: input.purpose === "filter" ? undefined : "active",
        sort_by: "updated_at",
        sort_dir: "desc",
    })
    return { ...page, items: page.items.map(customerItem) }
}

export async function fetchCustomerOption(
    customerId: string,
    input: Omit<CustomerSearch, "query"> = {
        purpose: "filter",
        scope: "assigned",
    },
): Promise<CustomerComboboxItem | null> {
    if (!customerId) return null
    const page = await searchCustomers({ ...input, query: "" })
    return page.items.find((row) => row.id === customerId) ?? null
}
