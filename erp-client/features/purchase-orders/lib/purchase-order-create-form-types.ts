import type { ComponentType, ReactNode } from "react"

import type { AppFormApi } from "@/components/form/form-api"
import type { SourcingLineInput } from "@/features/purchase-orders/lib/purchase-order-create-model"

export type PurchaseOrderCreateFormValues = {
    salesOrderId: string
    lines: SourcingLineInput[]
}

/** 供给分配表单实例的共享类型；拆分组件只关心字段与值。 */
export type PurchaseOrderCreateFormApi =
    AppFormApi<PurchaseOrderCreateFormValues> & {
        Subscribe: ComponentType<{
            selector: (state: {
                values: PurchaseOrderCreateFormValues
            }) => unknown
            children: (selected: never) => ReactNode
        }>
    }
