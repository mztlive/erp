"use client"

import { useEffect, useMemo } from "react"
import { useSelector } from "@tanstack/react-form"

import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { useAppForm } from "@/components/form"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    bpmReasonSchema,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

function firstErrorMessage(errors: readonly unknown[]): string | undefined {
    for (const item of errors) {
        if (typeof item === "string" && item.trim()) return item
        if (
            item &&
            typeof item === "object" &&
            "message" in item &&
            typeof (item as { message?: unknown }).message === "string" &&
            (item as { message: string }).message.trim()
        ) {
            return (item as { message: string }).message
        }
    }
    return undefined
}

/** 重提说明独立于业务内容；不改内容也须说明再次申请的依据（严格遵循 BPM 512 UTF-8 字节上限）。 */
export function useResubmissionReason({
    open,
    required,
    id,
    disabled = false,
    initialReason = "",
}: {
    open: boolean
    required: boolean
    id: string
    disabled?: boolean
    initialReason?: string
}) {
    const schema = useMemo(
        () => bpmReasonSchema({ required: true, label: "重新提交说明" }),
        [],
    )
    const form = useAppForm({
        defaultValues: { reason: "" },
    })
    useEffect(() => {
        if (open) form.reset({ reason: initialReason })
    }, [open, form, initialReason])
    const reason = useSelector(form.store, (state) =>
        state.values.reason.trim(),
    )
    const valid =
        !required ||
        (reason.length > 0 &&
            utf8ByteLength(reason) <= BPM_REASON_MAX_UTF8_BYTES)
    return {
        reason: required ? reason : undefined,
        valid,
        field: required ? (
            <form.AppField
                name="reason"
                validators={{
                    onChange: schema,
                    onBlur: schema,
                }}
            >
                {(field) => {
                    const showError =
                        field.state.meta.isTouched ||
                        field.state.meta.errors.length > 0
                    const error = showError
                        ? firstErrorMessage(field.state.meta.errors)
                        : undefined
                    return (
                        <ReasonTextareaField
                            id={id}
                            name={field.name}
                            mode="utf8-bytes"
                            max={BPM_REASON_MAX_UTF8_BYTES}
                            label="重新提交说明"
                            placeholder="说明本次修改或按原内容重新提交的依据"
                            rows={3}
                            required
                            value={field.state.value}
                            onChange={field.handleChange}
                            onBlur={field.handleBlur}
                            error={error}
                            invalid={Boolean(error)}
                            disabled={disabled}
                        />
                    )
                }}
            </form.AppField>
        ) : null,
    }
}
