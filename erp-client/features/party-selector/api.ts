import type { SettlementPartyComboboxItem } from "@/components/business/entity-comboboxes"

import { fetchSelectorList, type SelectorPage } from "@/lib/selector-list"
import {
    searchObjectDirectory,
    selectedObjectDirectory,
    type ObjectDirectoryItem,
} from "@/lib/object-directory"
export type PartySelectorPurpose =
    | "filter"
    | "form"
    | "purchase-receipt"
    | "sales-order"
    | "supplier-offering"
type EntitySearch = { query: string; purpose: PartySelectorPurpose }

type PartyDto = Readonly<{
    id: string
    party_no: string
    status: string
    current_revision_id?: string | null
}>

type PartyRevisionDto = Readonly<{
    id: string
    legal_name: string
    short_name?: string | null
}>

async function partyItem(row: PartyDto): Promise<SettlementPartyComboboxItem> {
    let displayName = row.party_no
    const revisions = await fetchSelectorList<PartyRevisionDto>(
        `/admin/parties/${encodeURIComponent(row.id)}/revisions`,
    )
    const revision = revisions.items.find(
        (item) => item.id === row.current_revision_id,
    )
    displayName = revision?.legal_name?.trim() || row.party_no
    const enabled = row.status.toLowerCase() === "active"
    return {
        partyId: row.id,
        partyCode: row.party_no,
        displayName,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}

export async function searchParties(
    input: EntitySearch,
): Promise<SelectorPage<SettlementPartyComboboxItem>> {
    if (input.purpose === "filter") {
        const page = await searchObjectDirectory(
            "settlement-parties",
            input.query,
        )
        return { ...page, items: page.items.map(directoryItem) }
    }
    const page = await fetchSelectorList<PartyDto>("/admin/parties", {
        keyword: input.query.trim() || undefined,
        status: "active",
        sort_by: "party_no",
        sort_dir: "asc",
    })
    return { ...page, items: await Promise.all(page.items.map(partyItem)) }
}

export async function fetchPartyOption(
    partyId: string,
    purpose: PartySelectorPurpose = "filter",
): Promise<SettlementPartyComboboxItem | null> {
    if (!partyId) return null
    if (purpose === "filter") {
        const page = await selectedObjectDirectory(
            "settlement-parties",
            partyId,
        )
        const row = page?.items.find((item) => item.id === partyId)
        return row ? directoryItem(row) : null
    }
    const rows = await searchParties({ query: "", purpose })
    return rows.items.find((row) => row.partyId === partyId) ?? null
}
function directoryItem(row: ObjectDirectoryItem): SettlementPartyComboboxItem {
    const enabled = row.status.toLowerCase() === "active"
    return {
        partyId: row.id,
        partyCode: row.code,
        displayName: row.name,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}
