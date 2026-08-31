"use client"

import {
    ProductCombobox,
    type ProductComboboxItem,
    type ProductComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useCompanySkuSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type CompanySkuSearchComboboxProps = SmartProps<
    ProductComboboxProps,
    ProductComboboxItem
>

export function CompanySkuSearchCombobox({
    purpose = "supplier-offering",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: CompanySkuSearchComboboxProps) {
    const search = useSearchInput()
    const query = useCompanySkuSelectorQuery({ query: search.input, purpose })
    const {
        rows,
        loading,
        emptyLabel: resolvedEmptyLabel,
    } = useRemoteSearchCombobox({
        list: query,
        selectedItem,
        idOf: (item) => item.productId,
        emptyLabel,
        fallbackError: "公司 SKU 加载失败，请重试",
    })
    return (
        <ProductCombobox
            {...props}
            value={value}
            products={rows}
            onValueChange={(id) => {
                onValueChange(id)
                onItemChange?.(rows.find((item) => item.productId === id))
            }}
            onSearchChange={search.onSearchChange}
            filterMode="remote"
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
