import type { ComponentType, ReactNode } from "react"
import type { FieldComponent, ReactFormExtendedApi } from "@tanstack/react-form"

import type { CreateSalesOrderFormValues } from "@/features/sales-orders/lib/sales-order-create-model"

export type SalesOrderEditorPurpose = "create" | "draft"

/**
 * 建单表单实例的共享类型。校验槽位用 `any`：各拆分组件只关心字段与值，
 * 不需要复述 TanStack Form 的完整校验泛型。
 */
export type SalesOrderCreateFormApi = ReactFormExtendedApi<
    CreateSalesOrderFormValues,
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
        CreateSalesOrderFormValues,
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
    AppForm: ComponentType<{ children?: ReactNode }>
    SubmitButton: ComponentType<any>
}
