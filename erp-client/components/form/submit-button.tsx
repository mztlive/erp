"use client"

import { useSyncExternalStore, type ComponentProps } from "react"

import { getApiFormError, subscribeApiFormError } from "./api-errors"
import { FormError } from "./form-error"
import { useFormContext } from "@/components/form/form-context"
import { LoadingButton } from "@/components/ui/loading-button"
import { cn } from "@/lib/utils"

export type SubmitButtonErrorPlacement = "none" | "top" | "bottom"

export type SubmitButtonProps = Omit<
    ComponentProps<typeof LoadingButton>,
    "type"
> & {
    label?: string
    pendingLabel?: string
    /**
     * 服务端提交错误摘要的展示位置。
     * - `"top"` / `"bottom"`：有错误时用独立纵向容器包裹摘要与按钮，避免在横排 footer 中注入 Fragment 撑坏布局；
     * - `"none"`：仅渲染提交按钮，由页面独立放置 `<FormError />`。
     */
    errorPlacement?: SubmitButtonErrorPlacement
    /**
     * @deprecated 请改用 `errorPlacement`（`"none"` 等价于 `showError={false}`）。
     * 页面集中展示错误时关闭，避免多个操作按钮重复渲染摘要。
     */
    showError?: boolean
    /** 内置错误摘要外层容器的附加类名。 */
    containerClassName?: string
    id?: string
}

/**
 * 本地校验和字段错误阻止提交；仅有服务端摘要时允许用户再次主动提交。
 * 须放在 `form.AppForm` 内，以便读取 form context。
 */
export function SubmitButton({
    label = "提交",
    pendingLabel = "提交中…",
    children,
    disabled,
    loading = false,
    errorPlacement,
    showError = true,
    containerClassName,
    className,
    id,
    ...props
}: SubmitButtonProps) {
    const form = useFormContext()
    const resolvedErrorPlacement: SubmitButtonErrorPlacement =
        errorPlacement ?? (showError ? "top" : "none")
    const serverError = useSyncExternalStore(
        (listener) => subscribeApiFormError(form, listener),
        () => getApiFormError(form),
        () => undefined,
    )
    const shouldRenderError =
        resolvedErrorPlacement !== "none" && typeof serverError === "string"

    return (
        <form.Subscribe
            selector={(state) => [state.canSubmit, state.isSubmitting] as const}
        >
            {([canSubmit, isSubmitting]) => {
                const button = (
                    <LoadingButton
                        {...props}
                        id={id}
                        type="submit"
                        className={className}
                        disabled={disabled || !canSubmit}
                        loading={loading || isSubmitting}
                    >
                        {children ??
                            (loading || isSubmitting ? pendingLabel : label)}
                    </LoadingButton>
                )

                if (!shouldRenderError) {
                    return button
                }

                return (
                    <div
                        data-slot="submit-button-group"
                        className={cn(
                            "flex flex-col gap-2",
                            typeof className === "string" &&
                                className.includes("w-full") &&
                                "w-full",
                            containerClassName,
                        )}
                    >
                        {resolvedErrorPlacement === "top" ? (
                            <FormError />
                        ) : null}
                        {button}
                        {resolvedErrorPlacement === "bottom" ? (
                            <FormError />
                        ) : null}
                    </div>
                )
            }}
        </form.Subscribe>
    )
}
