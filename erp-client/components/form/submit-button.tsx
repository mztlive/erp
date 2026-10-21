"use client"

import { useSyncExternalStore } from "react"
import { getApiFormError, subscribeApiFormError } from "./api-errors"
import { LoadingButton } from "@/components/ui/loading-button"
import { useFormContext } from "@/components/form/form-context"

type SubmitButtonProps = Omit<
    React.ComponentProps<typeof LoadingButton>,
    "type"
> & {
    label?: string
    pendingLabel?: string
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
    id,
    ...props
}: SubmitButtonProps & { id?: string }) {
    const form = useFormContext()
    const serverError = useSyncExternalStore(
        (listener) => subscribeApiFormError(form, listener),
        () => getApiFormError(form),
        () => undefined,
    )

    return (
        <form.Subscribe
            selector={(state) => [state.canSubmit, state.isSubmitting] as const}
        >
            {([canSubmit, isSubmitting]) => (
                <>
                    {typeof serverError === "string" ? (
                        <p
                            role="alert"
                            className="whitespace-pre-line text-sm text-destructive"
                        >
                            {serverError}
                        </p>
                    ) : null}
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
