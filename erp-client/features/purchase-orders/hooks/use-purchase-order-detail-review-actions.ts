"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import {
    REJECT_REASON_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import { useReviewPurchaseOrderMutation } from "@/features/purchase-orders/hooks/queries"
import type { PurchaseOrderDetailResult } from "@/features/purchase-orders/hooks/use-purchase-order-detail-command-state"

type UsePurchaseOrderDetailReviewActionsInput = {
    purchaseOrderId: string
    order: PurchaseOrderCenterView | null | undefined
    refetch: () => Promise<{ data?: PurchaseOrderCenterView | null }>
    commandLedger: FormalCommandKeyLedger
    setResult: React.Dispatch<
        React.SetStateAction<PurchaseOrderDetailResult | null>
    >
}

/**
 * 详情页财务审核动作：通过与驳回的正式领域命令编排。
 */
export function usePurchaseOrderDetailReviewActions({
    purchaseOrderId,
    order,
    refetch,
    commandLedger,
    setResult,
}: UsePurchaseOrderDetailReviewActionsInput) {
    const router = useRouter()
    const reviewMutation = useReviewPurchaseOrderMutation()

    const [approveConfirmOpen, setApproveConfirmOpen] = React.useState(false)

    const documentReference =
        order?.identity.purchaseNo ?? order?.identity.draftLabel

    async function handleApprove() {
        if (!order?.reviewWorkItem || !order.identity.currentSubmissionId)
            return
        if (commandLedger.peek("review-reject")) {
            setResult({
                status: "unknown",
                title: "审核结果待确认",
                description:
                    "原驳回操作的结果仍待确认，请先重试原操作，不能改为通过。",
                reference: documentReference,
            })
            throw new Error("原驳回操作的结果仍待确认")
        }
        const payload = {
            workItemId: order.reviewWorkItem.workItemId,
            expectedTaskVersion: order.reviewWorkItem.taskVersion,
            expectedSubjectVersion: order.reviewWorkItem.subjectVersion,
            decision: {
                purchaseOrderId,
                submissionId: order.identity.currentSubmissionId,
                expectedPurchaseOrderLockVersion: order.identity.lockVersion,
                reviewResult: "APPROVED",
            },
        } as const
        const command = commandLedger.acquire(
            "review-approve",
            `w08:${order.reviewWorkItem.workItemId}:${order.reviewWorkItem.taskVersion}:approve`,
            payload,
        )
        const response = await reviewMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle("review-approve", response.status)
        setApproveConfirmOpen(false)
        if (response.status === "succeeded") {
            setResult({
                status: "succeeded",
                title: "财务审核已通过",
                description:
                    "已形成采购生效版本与应付原始分录；审核任务完成。实际付款独立推进。",
                reference: response.reference,
                facts: [
                    {
                        label: "版本",
                        value: `v${response.data.revisionNo ?? 1}`,
                    },
                    {
                        label: "应付未结",
                        value: response.data.payableOpenAmount ?? "—",
                    },
                ],
            })
            await refetch()
            router.replace(`/procurement/orders/${purchaseOrderId}`)
        } else if (response.status === "unknown") {
            setResult({
                status: "unknown",
                title: "审核结果未知",
                description: response.message,
                reference: documentReference,
            })
        } else {
            setResult({
                status: "rejected",
                title: "通过失败",
                description: response.message,
            })
        }
    }

    async function handleReject(reasonCode: string, comment: string) {
        if (!order?.reviewWorkItem || !order.identity.currentSubmissionId)
            return
        if (commandLedger.peek("review-approve")) {
            setResult({
                status: "unknown",
                title: "审核结果待确认",
                description:
                    "原通过操作的结果仍待确认，请先重试原操作，不能改为驳回。",
                reference: documentReference,
            })
            throw new Error("原通过操作的结果仍待确认")
        }
        const payload = {
            workItemId: order.reviewWorkItem.workItemId,
            expectedTaskVersion: order.reviewWorkItem.taskVersion,
            expectedSubjectVersion: order.reviewWorkItem.subjectVersion,
            decision: {
                purchaseOrderId,
                submissionId: order.identity.currentSubmissionId,
                expectedPurchaseOrderLockVersion: order.identity.lockVersion,
                reviewResult: "REJECTED",
                reasonCode,
                comment,
            },
        } as const
        const command = commandLedger.acquire(
            "review-reject",
            `w08:${order.reviewWorkItem.workItemId}:${order.reviewWorkItem.taskVersion}:reject`,
            payload,
        )
        const response = await reviewMutation.mutateAsync({
            ...command.payload,
            idempotencyKey: command.idempotencyKey,
        })
        commandLedger.settle("review-reject", response.status)
        if (response.status === "succeeded") {
            setResult({
                status: "rejected",
                title: "财务已驳回",
                description:
                    "已记录驳回结论并完成当前审核任务；不创建替代任务。采购可改草稿后重新提交。",
                reference: response.reference,
                facts: [
                    {
                        label: "原因",
                        value: REJECT_REASON_LABEL[reasonCode] ?? reasonCode,
                    },
                    { label: "说明", value: comment },
                ],
            })
            await refetch()
            router.replace(`/procurement/orders/${purchaseOrderId}?mode=edit`)
        } else if (response.status === "unknown") {
            setResult({
                status: "unknown",
                title: "驳回结果未知",
                description: response.message,
                reference: documentReference,
            })
        } else {
            setResult({
                status: "rejected",
                title: "驳回失败",
                description: response.message,
            })
        }
    }

    return {
        approveConfirmOpen,
        setApproveConfirmOpen,
        reviewPending: reviewMutation.isPending,
        handleApprove,
        handleReject,
    }
}
