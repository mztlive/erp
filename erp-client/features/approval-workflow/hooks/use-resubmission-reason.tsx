"use client"

import { useEffect } from "react"
import { z } from "zod"
import { useSelector } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"

/** 重提说明独立于业务内容；不改内容也须说明再次申请的依据。 */
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
    const form = useAppForm({
        defaultValues: { reason: "" },
    })
    useEffect(() => {
        if (open) form.reset({ reason: initialReason })
    }, [open, form, initialReason])
    const reason = useSelector(form.store, (state) =>
        state.values.reason.trim(),
    )
    return {
        reason: required ? reason : undefined,
        valid: !required || (reason.length > 0 && reason.length <= 1000),
        field: required ? (
            <form.AppField
                name="reason"
                validators={{
                    onBlur: z
                        .string()
                        .trim()
                        .min(1, "请填写重新提交说明")
                        .max(1000, "重新提交说明最多 1000 字"),
                }}
            >
                {(field) => (
                    <field.TextareaField
                        id={id}
                        label="重新提交说明"
                        placeholder="说明本次修改或按原内容重新提交的依据"
                        required
                        maxLength={1000}
                        disabled={disabled}
                    />
                )}
            </form.AppField>
        ) : null,
    }
}
