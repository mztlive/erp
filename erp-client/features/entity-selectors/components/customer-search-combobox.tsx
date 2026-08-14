"use client"

import {
    CustomerCombobox,
    type CustomerComboboxItem,
    type CustomerComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useCustomerSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type CustomerSearchComboboxProps = SmartProps<
    CustomerComboboxProps,
    CustomerComboboxItem
> & {
    scope?: "mine" | "collaborating" | "assigned" | "all_authorized"
}

export function CustomerSearchCombobox({
    purpose = "form",
    scope = "assigned",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: CustomerSearchComboboxProps) {
    const search = useSearchInput()
    const query = useCustomerSelectorQuery(
        { query: search.input, purpose, scope },
        value,
    )
    const { rows, loading, emptyLabel: resolvedEmptyLabel } =
        useRemoteSearchCombobox({
            list: query.list,
            selected: query.selected,
            selectedItem,
            idOf: (item) => item.id,
            emptyLabel,
            fallbackError: "客户加载失败，请重试",
        })
    return (
        <CustomerCombobox
            {...props}
            value={value}
            customers={rows}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(rows.find((item) => item.id === id))
            }}
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
