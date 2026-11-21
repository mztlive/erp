"use client"

import { useSyncExternalStore } from "react"

import { getApiFormError, subscribeApiFormError } from "./api-errors"
import { useFormContext } from "@/components/form/form-context"
import { cn } from "@/lib/utils"

/** 在表单操作区之外集中展示服务端摘要。须放在 `form.AppForm` 内。 */
export function FormError({ className, ...props }: React.ComponentProps<"p">) {
    const form = useFormContext()
    const serverError = useSyncExternalStore(
        (listener) => subscribeApiFormError(form, listener),
        () => getApiFormError(form),
        () => undefined,
    )

    if (typeof serverError !== "string") return null

    return (
        <p
            role="alert"
            className={cn(
                "whitespace-pre-line text-body-compact text-destructive",
                className,
            )}
            {...props}
        >
            {serverError}
        </p>
    )
}
