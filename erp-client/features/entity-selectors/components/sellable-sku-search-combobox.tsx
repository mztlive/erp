"use client"

import {
    ProductCombobox,
    type ProductComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SellableSkuComboboxItem } from "@/features/entity-selectors/api/index"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useSellableSkuSelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"
import { useSearchInput } from "@/features/entity-selectors/hooks/use-search-input"

export type SellableSkuSearchComboboxProps = SmartProps<
    ProductComboboxProps,
    SellableSkuComboboxItem
> & { productKind?: string; excludeProductKind?: string }

export function SellableSkuSearchCombobox({
    purpose = "sales-order",
    productKind,
    excludeProductKind,
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: SellableSkuSearchComboboxProps) {
    const search = useSearchInput()
    const query = useSellableSkuSelectorQuery({
        query: search.input,
        purpose,
        productKind,
        excludeProductKind,
    })
    const {
        rows,
        loading,
        emptyLabel: resolvedEmptyLabel,
    } = useRemoteSearchCombobox({
        list: query,
        selectedItem,
        idOf: (item) => item.productId,
        emptyLabel,
        fallbackError: "商品加载失败，请重试",
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
