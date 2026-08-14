"use client"

import {
    WarehouseCombobox,
    type WarehouseComboboxItem,
    type WarehouseComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useWarehouseSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type WarehouseSearchComboboxProps = SmartProps<
    WarehouseComboboxProps,
    WarehouseComboboxItem
>

export function WarehouseSearchCombobox({
    purpose = "filter",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: WarehouseSearchComboboxProps) {
    const search = useSearchInput()
    const query = useWarehouseSelectorQuery(
        { query: search.input, purpose },
        value,
    )
    const { rows, loading, emptyLabel: resolvedEmptyLabel } =
        useRemoteSearchCombobox({
            list: query.list,
            selected: query.selected,
            selectedItem,
            idOf: (item) => item.warehouseId,
            emptyLabel,
            fallbackError: "仓库加载失败，请重试",
        })
    return (
        <WarehouseCombobox
            {...props}
            value={value}
            warehouses={rows}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(rows.find((item) => item.warehouseId === id))
            }}
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
