"use client"

import {
    SettlementPartyCombobox,
    type SettlementPartyComboboxItem,
    type SettlementPartyComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { usePartySelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type SettlementPartySearchComboboxProps = SmartProps<
    SettlementPartyComboboxProps,
    SettlementPartyComboboxItem
>

export function SettlementPartySearchCombobox({
    purpose = "form",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    restrictToPartyId,
    ...props
}: SettlementPartySearchComboboxProps & {
    /** 已选客户对应主体；空搜索时只列出该主体，输入关键词后仍可搜全部。 */
    restrictToPartyId?: string
}) {
    const search = useSearchInput()
    const query = usePartySelectorQuery({ query: search.input, purpose }, value)
    const { rows, loading, emptyLabel: resolvedEmptyLabel } =
        useRemoteSearchCombobox({
            list: query.list,
            selected: query.selected,
            selectedItem,
            idOf: (item) => item.partyId,
            emptyLabel,
            fallbackError: "结算主体加载失败，请重试",
        })
    const parties =
        restrictToPartyId && !search.input.trim()
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
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={
                restrictToPartyId && !search.input.trim() && parties.length === 0
                    ? "请先选择客户"
                    : resolvedEmptyLabel
            }
        />
    )
}
