"use client"

import * as React from "react"
import { z } from "zod"
import type { UseMutationResult } from "@tanstack/react-query"

import { useAppForm } from "@/components/form"
import type {
    FormalActionResponse,
    NoteInput,
    SupplierOrderDetailView,
} from "@/features/supplier-orders/types"
import type { SupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

export const noteSchema = z.object({
    comment: z.string().trim().min(2, "请填写协同说明"),
})

/** 审计页协同说明表单；提交走 useAddNoteMutation。 */
export function useSupplierOrderCenterNoteForm(input: {
    orderId: string
    detail: SupplierOrderDetailView | undefined
    noteMutation: UseMutationResult<
        FormalActionResponse<{ lockVersion: number }>,
        Error,
        NoteInput
    >
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
}) {
    const { orderId, detail, noteMutation, setResult } = input

    const noteForm = useAppForm({
        defaultValues: { comment: "" },
        validators: { onChange: noteSchema },
        onSubmit: async ({ value }) => {
            if (!detail) return
            const res = await noteMutation.mutateAsync({
                orderId,
                expectedLockVersion: detail.order.lockVersion,
                comment: value.comment,
                idempotencyKey: `note-${orderId}-${Date.now()}`,
            })
            if (res.status === "succeeded") {
                noteForm.reset()
                setResult({
                    status: "succeeded",
                    title: "协同说明已记录",
                    description: res.message,
                })
            } else {
                setResult({
                    status: res.status === "blocked" ? "blocked" : "rejected",
                    title: "协同说明未写入",
                    description: res.message,
                })
            }
        },
    })

    return noteForm
}

export type SupplierOrderCenterNoteForm = ReturnType<
    typeof useSupplierOrderCenterNoteForm
>
