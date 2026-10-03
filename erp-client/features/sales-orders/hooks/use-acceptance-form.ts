"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import type { ValidationIssue } from "@/components/business"
import {
    collectValidationIssues,
    todayLocalDateTimeInput,
    type AcceptanceBatchSelection,
} from "@/features/sales-orders/lib/acceptance-model"

const draftHeaderSchema = z.object({
    acceptedAt: z.string().min(1, "请填写客户验收时间"),
    comment: z.string(),
    evidenceFile: z
        .custom<File | null>()
        .refine((file) => file != null, "请上传签收单凭证（图片或 PDF）")
        .refine(
            (file) =>
                !file ||
                [
                    "application/pdf",
                    "image/jpeg",
                    "image/png",
                    "image/webp",
                ].includes(file.type),
            "凭证仅支持 PDF、JPG、PNG 或 WebP",
        )
        .refine(
            (file) => !file || (file.size > 0 && file.size <= 5 * 1024 * 1024),
            "请选择不超过 5 MB 的非空凭证文件",
        ),
})

export type AcceptanceFormApi = ReturnType<typeof useAcceptanceForm>["form"]

export function useAcceptanceForm({
    selected,
    onValidSubmit,
}: {
    selected: AcceptanceBatchSelection
    onValidSubmit: () => void
}) {
    const [clientIssues, setClientIssues] = React.useState<ValidationIssue[]>(
        [],
    )
    const selectedRef = React.useRef(selected)
    selectedRef.current = selected
    const onValidSubmitRef = React.useRef(onValidSubmit)
    onValidSubmitRef.current = onValidSubmit

    const form = useAppForm({
        defaultValues: {
            acceptedAt: todayLocalDateTimeInput(),
            comment: "",
            evidenceFile: null as File | null,
        },
        validators: { onChange: draftHeaderSchema },
        onSubmit: async () => {
            const issues = collectValidationIssues(selectedRef.current)
            setClientIssues(issues)
            if (issues.length > 0) return
            onValidSubmitRef.current()
        },
    })

    const formDirty = useSelector(form.store, (state) => state.isDirty)

    return { form, formDirty, clientIssues, setClientIssues }
}
