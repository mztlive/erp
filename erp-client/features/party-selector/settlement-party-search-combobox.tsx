"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    SettlementPartyCombobox,
    type SettlementPartyComboboxItem,
    type SettlementPartyComboboxProps,
} from "@/components/business/entity-comboboxes"
import { apiGet, type Page } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"

const OPTION_PAGE_SIZE = 30
const STALE_TIME = 5 * 60 * 1000

type PartySelectorPurpose =
    | "filter"
    | "form"
    | "purchase-receipt"
    | "sales-order"
    | "supplier-offering"

type PartyDto = Readonly<{
    id: string
    party_no: string
    status: string
    current_revision_id?: string | null
}>

type PartyRevisionDto = Readonly<{
    id: string
    legal_name: string
}>

export type SettlementPartySearchComboboxProps = Omit<
    SettlementPartyComboboxProps,
    "loading" | "filterMode" | "onSearchChange" | "parties"
> & {
    purpose?: PartySelectorPurpose
    selectedItem?: SettlementPartyComboboxItem
    onItemChange?: (item?: SettlementPartyComboboxItem) => void
    /** 已选客户对应主体；空搜索时只列出该主体，输入关键词后仍可搜全部。 */
    restrictToPartyId?: string
}

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
    const enabled = row.status.toLowerCase() === "active"
    return {
        partyId: row.id,
        partyCode: row.party_no,
        displayName,
        statusLabel: enabled ? "启用" : "停用",
        statusTone: enabled ? "success" : "neutral",
    }
}

async function searchParties(
    query: string,
): Promise<readonly SettlementPartyComboboxItem[]> {
    const page = await apiGet<Page<PartyDto>>("/admin/parties", {
        keyword: query.trim() || undefined,
        status: "active",
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "party_no",
        sort_dir: "asc",
    })
    return Promise.all(page.items.map(partyItem))
}

async function fetchPartyOption(
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

/** 独立的结算主体公共选择器；不得依赖 master-data feature。 */
export function SettlementPartySearchCombobox({
    purpose = "form",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    restrictToPartyId,
    ...props
}: SettlementPartySearchComboboxProps) {
    const [input, setInput] = React.useState("")
    const [query, setQuery] = React.useState("")
    React.useEffect(() => {
        const timer = window.setTimeout(() => setQuery(input.trim()), 250)
        return () => window.clearTimeout(timer)
    }, [input])

    const list = useQuery({
        queryKey: ["party-selector", "list", { purpose, query }],
        queryFn: () => searchParties(query),
        staleTime: STALE_TIME,
        placeholderData: (previous) => previous,
    })
    const selected = useQuery({
        queryKey: ["party-selector", "detail", value ?? ""],
        queryFn: () => fetchPartyOption(value ?? ""),
        enabled: Boolean(value),
        staleTime: STALE_TIME,
    })
    const selectedRow = selectedItem ?? selected.data ?? undefined
    const rows = [...(list.data ?? [])]
    if (
        selectedRow &&
        !rows.some((item) => item.partyId === selectedRow.partyId)
    ) {
        rows.unshift(selectedRow)
    }
    const parties =
        restrictToPartyId && !query
            ? rows.filter((item) => item.partyId === restrictToPartyId)
            : rows

    return (
        <SettlementPartyCombobox
            {...props}
            value={value}
            parties={parties}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(parties.find((item) => item.partyId === id))
            }}
            onSearchChange={setInput}
            filterMode="remote"
            loading={list.isFetching || (selected.isFetching && !selectedRow)}
            emptyLabel={
                list.isError
                    ? getErrorMessage(list.error, "结算主体加载失败，请重试")
                    : restrictToPartyId && !query && parties.length === 0
                      ? "请先选择客户"
                      : emptyLabel
            }
        />
    )
}
