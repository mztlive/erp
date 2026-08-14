import type { SettlementPartyComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { OPTION_PAGE_SIZE, activeStatus } from "./shared"
import type { EntitySearch } from "./types"

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
    try {
        const revisions = await apiGet<Page<PartyRevisionDto>>(
            `/admin/parties/${encodeURIComponent(row.id)}/revisions`,
            { page: 1, page_size: 1, sort_by: "revision_no", sort_dir: "desc" },
        )
        const revision =
            revisions.items.find(
                (item) => item.id === row.current_revision_id,
            ) ?? revisions.items[0]
        displayName = revision?.legal_name?.trim() || row.party_no
    } catch {
        // 主体仍可按稳定编号选择；名称修订无权限时不伪造名称。
    }
    const enabled = activeStatus(row.status)
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
): Promise<readonly SettlementPartyComboboxItem[]> {
    const page = await apiGet<Page<PartyDto>>("/admin/parties", {
        keyword: input.query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "party_no",
        sort_dir: "asc",
    })
    return Promise.all(page.items.map(partyItem))
}

export async function fetchPartyOption(
    partyId: string,
): Promise<SettlementPartyComboboxItem | null> {
    if (!partyId) return null
    try {
        return partyItem(
            await apiGet<PartyDto>(
                `/admin/parties/${encodeURIComponent(partyId)}`,
            ),
        )
    } catch {
        return null
    }
}
