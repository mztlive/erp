"use client"

import { FormError } from "./form-error"
import { LoadingButton } from "@/components/ui/loading-button"
import { useFormContext } from "@/components/form/form-context"

type SubmitButtonProps = Omit<
    React.ComponentProps<typeof LoadingButton>,
    "type"
> & {
    label?: string
    pendingLabel?: string
    /** 页面集中展示错误时关闭，避免多个操作按钮重复渲染摘要。 */
    showError?: boolean
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
    showError = true,
    id,
    ...props
}: SubmitButtonProps & { id?: string }) {
    const form = useFormContext()

    return (
        <form.Subscribe
            selector={(state) => [state.canSubmit, state.isSubmitting] as const}
        >
            {([canSubmit, isSubmitting]) => (
                <>
                    {showError ? <FormError /> : null}
                    <LoadingButton
                        {...props}
                        id={id ?? "form-submit"}
                        type="submit"
                        disabled={disabled || !canSubmit}
                        loading={loading || isSubmitting}
                    >
                        {children ??
                            (loading || isSubmitting ? pendingLabel : label)}
                    </LoadingButton>
                </>
            )}
        </form.Subscribe>
    )
}
