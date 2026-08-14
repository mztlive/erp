import type { EntitySelectorPurpose } from "@/features/entity-selectors/api/index"

/**
 * 各实体搜索组合框的公共属性：去掉由数据层托管的字段，
 * 换成选择器用途与选中项回调。
 */
export type SmartProps<TProps, TItem> = Omit<
    TProps,
    | "loading"
    | "filterMode"
    | "onSearchChange"
    | "contracts"
    | "customers"
    | "orders"
    | "products"
    | "parties"
    | "suppliers"
    | "warehouses"
> & {
    purpose?: EntitySelectorPurpose
    selectedItem?: TItem
    onItemChange?: (item?: TItem) => void
}
