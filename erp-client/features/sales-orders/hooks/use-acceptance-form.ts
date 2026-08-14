"use client"

import * as React from "react"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"

import { useAppForm } from "@/components/form"
import type { ValidationIssue } from "@/components/business"
import {
    collectValidationIssues,
    todayLocalDateTimeInput,
    type AcceptanceFactSelection,
    type LineResultState,
} from "@/features/sales-orders/lib/acceptance-model"

const draftHeaderSchema = z.object({
    acceptedAt: z.string().min(1, "请填写客户验收时间"),
    comment: z.string(),
})

/** 表单实例类型：由本 hook 的返回类型推导，供拆分的面板组件复用。 */
export type AcceptanceFormApi = ReturnType<typeof useAcceptanceForm>["form"]

/**
 * 验收工作台的表单（表头）与提交前校验。
 * 提交时校验来源/行结果；校验通过后回调 onValidSubmit（打开确认框）。
 */
export function useAcceptanceForm({
    selected,
    lineResults,
    onValidSubmit,
}: {
    selected: AcceptanceFactSelection
    lineResults: Map<string, LineResultState>
    onValidSubmit: () => void
}) {
    const [clientIssues, setClientIssues] = React.useState<ValidationIssue[]>(
        [],
    )

    const form = useAppForm({
        defaultValues: {
            acceptedAt: todayLocalDateTimeInput(),
            comment: "",
        },
        validators: { onChange: draftHeaderSchema },
        onSubmit: async () => {
            const issues = collectValidationIssues(selected, lineResults)
            setClientIssues(issues)
            if (issues.length > 0) return
            onValidSubmit()
        },
    })

    const formDirty = useSelector(form.store, (state) => state.isDirty)

    return { form, formDirty, clientIssues }
}
