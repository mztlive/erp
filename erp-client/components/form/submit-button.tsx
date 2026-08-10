"use client"

import { Button } from "@/components/ui/button"
import { useFormContext } from "@/components/form/form-context"

type SubmitButtonProps = Omit<React.ComponentProps<typeof Button>, "type"> & {
    label?: string
    pendingLabel?: string
}

/**
 * 绑定表单提交态的按钮：不可提交 / 提交中自动 disabled。
 * 须放在 `form.AppForm` 内，以便读取 form context。
 */
export function SubmitButton({
    label = "提交",
    pendingLabel = "提交中…",
    children,
    disabled,
    ...props
}: SubmitButtonProps) {
    const form = useFormContext()

    return (
        <form.Subscribe
            selector={(state) => [state.canSubmit, state.isSubmitting] as const}
        >
            {([canSubmit, isSubmitting]) => (
                <Button
                    type="submit"
                    disabled={disabled || !canSubmit || isSubmitting}
                    {...props}
                >
                    {children ?? (isSubmitting ? pendingLabel : label)}
                </Button>
            )}
        </form.Subscribe>
    )
}
