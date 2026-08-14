"use client"

import {
    ProductCombobox,
    type ProductComboboxProps,
} from "@/components/business/entity-comboboxes"
import type { SellableSkuComboboxItem } from "@/features/entity-selectors/api/index"
import type { SmartProps } from "@/features/entity-selectors/components/types"
import { useVoucherCategorySelectorQuery } from "@/features/entity-selectors/hooks/queries"
import { useRemoteSearchCombobox } from "@/features/entity-selectors/hooks/use-remote-search-combobox"

export type VoucherCategorySearchComboboxProps = SmartProps<
    ProductComboboxProps,
    SellableSkuComboboxItem
>

/** 卡券类目优先使用正式档案；档案未启用时回退公司商品池卡券 SKU。 */
export function VoucherCategorySearchCombobox({
    purpose = "sales-order",
    selectedItem,
    onItemChange,
    emptyLabel,
    value,
    onValueChange,
    ...props
}: VoucherCategorySearchComboboxProps) {
    const query = useVoucherCategorySelectorQuery(purpose)
    const { rows, loading, emptyLabel: resolvedEmptyLabel } =
        useRemoteSearchCombobox({
            list: query,
            selectedItem,
            idOf: (item) => item.productId,
            emptyLabel,
            fallbackError: "卡券类目加载失败，请重试",
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
            loading={loading}
            emptyLabel={resolvedEmptyLabel}
        />
    )
}
