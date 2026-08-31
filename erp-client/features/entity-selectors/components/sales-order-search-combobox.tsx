"use client"

import {
    SalesOrderCombobox,
    type SalesOrderComboboxItem,
    type SalesOrderComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useSalesOrderSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type SalesOrderSearchComboboxProps = SmartProps<
    SalesOrderComboboxProps,
    SalesOrderComboboxItem
>

export function SalesOrderSearchCombobox({
    purpose = "filter",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: SalesOrderSearchComboboxProps) {
    const search = useSearchInput()
    const query = useSalesOrderSelectorQuery(
        { query: search.input, purpose },
        value,
    )
    const {
        rows,
        loading,
        emptyLabel: resolvedEmptyLabel,
    } = useRemoteSearchCombobox({
        list: query.list,
        selected: query.selected,
        selectedItem,
        idOf: (item) => item.id,
        emptyLabel,
        fallbackError: "销售单加载失败，请重试",
    })
    return (
        <SalesOrderCombobox
            {...props}
            value={value}
            orders={rows}
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
