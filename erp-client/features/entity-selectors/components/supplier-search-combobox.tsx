"use client"

import {
    SupplierCombobox,
    type SupplierComboboxItem,
    type SupplierComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useSupplierSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type SupplierSearchComboboxProps = SmartProps<
    SupplierComboboxProps,
    SupplierComboboxItem
>

export function SupplierSearchCombobox({
    purpose = "form",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: SupplierSearchComboboxProps) {
    const search = useSearchInput()
    const query = useSupplierSelectorQuery(
        { query: search.input, purpose },
        value,
    )
    const { rows, loading, emptyLabel: resolvedEmptyLabel } =
        useRemoteSearchCombobox({
            list: query.list,
            selected: query.selected,
            selectedItem,
            idOf: (item) => item.supplierId,
            emptyLabel,
            fallbackError: "供应商加载失败，请重试",
        })
    return (
        <SupplierCombobox
            {...props}
            value={value}
            suppliers={rows}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(rows.find((item) => item.supplierId === id))
            }}
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
