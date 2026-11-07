import type { ComponentType, ReactNode } from "react"
import type { FieldComponent, ReactFormExtendedApi } from "@tanstack/react-form"

/**
 * 拆分表单子组件共享的实例类型。
 *
 * 校验槽位用 `any`：子组件只关心字段与值，不复述 TanStack Form 的完整校验泛型。
 * 需要 `form.AppForm` / `form.SubmitButton` 的表单使用 `AppFormWithBarApi`。
 */
export type AppFormApi<TValues> = ReactFormExtendedApi<
    TValues,
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
        TValues,
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
}

export type AppFormWithBarApi<TValues> = AppFormApi<TValues> & {
    AppForm: ComponentType<{ children?: ReactNode }>
    SubmitButton: ComponentType<any>
}
