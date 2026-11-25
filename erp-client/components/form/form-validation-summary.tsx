"use client"

import { useStore, type AnyFormApi } from "@tanstack/react-form"

import type { AppFormApi } from "@/components/form/form-api"
import { useFormContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import { cn } from "@/lib/utils"

export type FormValidationSummaryProps<TValues = unknown> = {
    /** 脱离 `form.AppForm` 使用时显式传入；置于 `form.AppForm` 内可省略以自动读取上下文。 */
    form?: AppFormApi<TValues>
    className?: string
    id?: string
}

function FormValidationSummaryInner({
    form,
    className,
    id,
}: {
    form: AnyFormApi
    className?: string
    id?: string
}) {
    const errors = useStore(form.store, (state) => state.errors)
    const messages = [
        ...new Set(
            toFieldErrors(errors)
                .map((item) => item?.message?.trim())
                .filter((message): message is string => Boolean(message)),
        ),
    ]

    if (!messages.length) return null

    return (
        <p
            id={id}
            role="alert"
            className={cn("text-body-compact text-destructive", className)}
        >
            {messages.join("；")}
        </p>
    )
}

function ContextFormValidationSummary({
    className,
    id,
}: {
    className?: string
    id?: string
}) {
    const form = useFormContext()
    return (
        <FormValidationSummaryInner form={form} className={className} id={id} />
    )
}

/** 把整表校验错误收成一条提示；支持传入 `form` 或在 `form.AppForm` 内直接读取上下文。 */
export function FormValidationSummary<TValues = unknown>({
    form,
    className,
    id,
}: FormValidationSummaryProps<TValues>) {
    if (form) {
        return (
            <FormValidationSummaryInner
                form={form}
                className={className}
                id={id}
            />
        )
    }

    return <ContextFormValidationSummary className={className} id={id} />
}
