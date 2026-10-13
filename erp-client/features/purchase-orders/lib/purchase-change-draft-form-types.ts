import type { ComponentType, ReactNode } from "react"
import type { FieldComponent, ReactFormExtendedApi } from "@tanstack/react-form"
import type { PurchaseChangeDraftValues } from "./purchase-change-draft"

/** 子组件共享表单字段类型；校验泛型沿用项目表单约定。 */
export type PurchaseChangeDraftFormApi = ReactFormExtendedApi<
    PurchaseChangeDraftValues,
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
        PurchaseChangeDraftValues,
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
