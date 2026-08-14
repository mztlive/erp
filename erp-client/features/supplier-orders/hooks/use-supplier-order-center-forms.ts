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
import {
    RELEASE_REASON_OPTIONS,
    WORK_ITEM_STATUS_LABEL,
} from "@/features/supplier-orders/types"
import type {
    WorkItemDto,
    WorkItemResponsibilityCommand,
} from "@/features/work-items/types"
import type { CommandIdentity } from "@/features/supplier-orders/hooks/use-supplier-order-center-identity"
import type { SupplierOrderCenterResult } from "@/features/supplier-orders/hooks/use-supplier-order-center-actions"

export const noteSchema = z.object({
    comment: z.string().trim().min(2, "请填写协同说明"),
})

export const releaseSchema = z.object({
    reasonCode: z.string().min(1, "请选择原因"),
    comment: z.string(),
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

/** 退回团队表单；提交走 useWorkItemResponsibilityMutation。 */
export function useSupplierOrderCenterReleaseForm(input: {
    detail: SupplierOrderDetailView | undefined
    setResult: React.Dispatch<
        React.SetStateAction<SupplierOrderCenterResult | null>
    >
    responsibilityMutation: UseMutationResult<
        WorkItemDto,
        Error,
        WorkItemResponsibilityCommand
    >
    refetch: () => Promise<unknown>
    commandIdentity: (kind: string, objectId: string) => CommandIdentity
    forgetCommandIdentity: (key: string) => void
}) {
    const {
        detail,
        setResult,
        responsibilityMutation,
        refetch,
        commandIdentity,
        forgetCommandIdentity,
    } = input

    const [releaseOpen, setReleaseOpen] = React.useState(false)

    const releaseForm = useAppForm({
        defaultValues: { reasonCode: "WAITING_SUPPLIER", comment: "" },
        validators: { onChange: releaseSchema },
        onSubmit: async ({ value }) => {
            if (!detail?.workItem) return
            const identity = commandIdentity(
                "release-to-team",
                detail.workItem.workItemId,
            )
            const res = await responsibilityMutation.mutateAsync({
                kind: "RELEASE_TO_TEAM",
                workItemId: detail.workItem.workItemId,
                expectedTaskVersion: detail.workItem.taskVersion,
                reason: value.comment.trim()
                    ? `${value.reasonCode}: ${value.comment.trim()}`
                    : value.reasonCode,
                idempotencyKey: identity.idempotencyKey,
            })
            forgetCommandIdentity(identity.key)
            setReleaseOpen(false)
            releaseForm.reset()
            await refetch()
            setResult({
                status: "succeeded",
                title: "任务已退回团队",
                description:
                    "个人责任已退回团队；任务保持待处理，可由团队重新开始处理。",
                reference: res.id,
                facts: [
                    {
                        label: "任务状态",
                        value: WORK_ITEM_STATUS_LABEL[res.status],
                    },
                    {
                        label: "原因",
                        value:
                            RELEASE_REASON_OPTIONS.find(
                                (option) => option.value === value.reasonCode,
                            )?.label ?? value.reasonCode,
                    },
                ],
            })
        },
    })

    return { releaseForm, releaseOpen, setReleaseOpen }
}

export type SupplierOrderCenterReleaseForm = ReturnType<
    typeof useSupplierOrderCenterReleaseForm
>["releaseForm"]
