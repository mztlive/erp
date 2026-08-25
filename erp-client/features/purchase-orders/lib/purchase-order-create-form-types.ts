import type { ComponentType, ReactNode } from "react"
import type { FieldComponent, ReactFormExtendedApi } from "@tanstack/react-form"

import type { SourcingLineInput } from "@/features/purchase-orders/lib/purchase-order-create-model"

export type PurchaseOrderCreateFormValues = {
    salesOrderId: string
    lines: SourcingLineInput[]
}

/**
 * 采购建单表单实例的共享类型。校验槽位用 `any`：拆分组件只关心字段与值。
 */
export type PurchaseOrderCreateFormApi = ReactFormExtendedApi<
    PurchaseOrderCreateFormValues,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any,
    any
> & {
    AppField: FieldComponent<
        PurchaseOrderCreateFormValues,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any,
        any
    >
    Subscribe: ComponentType<{
        selector: (state: { values: PurchaseOrderCreateFormValues }) => unknown
        children: (selected: never) => ReactNode
    }>
}
