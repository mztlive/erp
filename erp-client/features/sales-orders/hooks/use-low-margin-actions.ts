"use client"

import * as React from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"

import { completeLowMarginManagerConfirmation } from "@/features/sales-orders/api/sales-orders"
import { salesOrderKeys } from "@/features/sales-orders/hooks/queries"
import type {
    ActiveLowMarginManagerConfirmation,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"

export type LowMarginManagerResult = {
    status: "succeeded" | "blocked" | "rejected" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

/**
 * 低毛利上级确认的动作状态与提交逻辑；动作完全由详情 allowedActions 驱动。
 */
export function useLowMarginManagerActions({
    order,
    confirmation,
    onResult,
}: {
    order: SalesOrderListItem
    confirmation: ActiveLowMarginManagerConfirmation
    onResult: (result: LowMarginManagerResult) => void
}) {
    const queryClient = useQueryClient()
    const responsibility = useWorkItemResponsibilityMutation()
    const decision = useMutation({
        mutationFn: completeLowMarginManagerConfirmation,
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(order.id),
            })
        },
    })
    const [approveOpen, setApproveOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [reasonCode, setReasonCode] = React.useState("")
    const [comment, setComment] = React.useState("")
    const common = {
        salesOrderId: order.id,
        workItemId: confirmation.workItemId,
        taskVersion: confirmation.taskVersion,
        subjectVersion: confirmation.subjectVersion,
        lowMarginSubmissionId: confirmation.lowMarginSubmissionId,
        rejectedProcurementConfirmationId:
            confirmation.rejectedProcurementConfirmationId,
        expectedSalesOrderLockVersion: order.lockVersion,
    }

    const startProcessing = async () => {
        await responsibility.mutateAsync({
            kind: "START_PROCESSING",
            workItemId: confirmation.workItemId,
            expectedTaskVersion: confirmation.taskVersion,
            idempotencyKey: `w05:${confirmation.workItemId}:${confirmation.taskVersion}:START`,
        })
        await queryClient.invalidateQueries({
            queryKey: salesOrderKeys.detail(order.id),
        })
    }

    const confirmApprove = async () => {
        const result = await decision.mutateAsync({
            ...common,
            decision: "APPROVE",
            idempotencyKey: `w05:${confirmation.workItemId}:${confirmation.taskVersion}:APPROVE`,
        })
        onResult({
            status: "succeeded",
            title: "已同意低毛利承接",
            description: "已创建新的采购确认待办。",
            reference:
                result.outcome ===
                "LOW_MARGIN_APPROVED_AND_PROCUREMENT_RESUBMITTED"
                    ? result.newProcurementConfirmationId
                    : result.workflowActionId,
            nextResponsible: "采购",
        })
    }

    const confirmReject = async () => {
        if (!reasonCode.trim() || !comment.trim())
            throw new Error("原因代码和驳回说明不能为空")
        const result = await decision.mutateAsync({
            ...common,
            decision: "REJECT",
            reasonCode,
            comment,
            idempotencyKey: `w05:${confirmation.workItemId}:${confirmation.taskVersion}:REJECT`,
        })
        onResult({
            status: "rejected",
            title: "已驳回低毛利承接",
            description: "销售已回到采购驳回固定处置。",
            reference: result.workflowActionId,
            nextResponsible: "销售",
        })
    }

    return {
        approveOpen,
        setApproveOpen,
        rejectOpen,
        setRejectOpen,
        reasonCode,
        setReasonCode,
        comment,
        setComment,
        isPending: decision.isPending,
        isStartPending: responsibility.isPending,
        startProcessing,
        confirmApprove,
        confirmReject,
    }
}
