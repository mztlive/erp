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
    ...props
}: SettlementPartySearchComboboxProps) {
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
    return (
        <SettlementPartyCombobox
            {...props}
            value={value}
            parties={rows}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(rows.find((item) => item.partyId === id))
            }}
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
