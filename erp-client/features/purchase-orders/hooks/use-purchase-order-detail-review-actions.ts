"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { getErrorMessage } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"
import { responsibilityText } from "@/lib/ui-text"
import {
    REJECT_REASON_LABEL,
    type PurchaseOrderCenterView,
} from "@/features/purchase-orders/types"
import {
    useReviewPurchaseOrderMutation,
} from "@/features/purchase-orders/hooks/queries"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
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
 * 详情页财务审核动作：通过、驳回、开始处理与退回团队的正式命令编排。
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
    const responsibilityMutation = useWorkItemResponsibilityMutation()

    const [approveConfirmOpen, setApproveConfirmOpen] = React.useState(false)
    const [releaseConfirmOpen, setReleaseConfirmOpen] = React.useState(false)
    const [releaseReason, setReleaseReason] = React.useState("")

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

    async function handleStartProcessing() {
        const workItem = order?.reviewWorkItem
        if (!workItem?.responsibilityActions.includes("START_PROCESSING"))
            return
        const payload = {
            kind: "START_PROCESSING" as const,
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
        }
        const command = commandLedger.acquire(
            "review-responsibility",
            `w08:${workItem.workItemId}:${workItem.taskVersion}:start`,
            payload,
        )
        try {
            await responsibilityMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle("review-responsibility", "succeeded")
            await refetch()
        } catch (error) {
            const settlement = classifyFormalCommandError(error)
            commandLedger.settle("review-responsibility", settlement)
            setResult({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title: responsibilityText.changed,
                description: getErrorMessage(
                    error,
                    settlement === "unknown"
                        ? "处理结果待确认，请使用本次操作重试"
                        : "开始处理失败，请刷新后重试",
                ),
                reference:
                    settlement === "unknown" ? documentReference : undefined,
            })
        }
    }

    async function handleReleaseToTeam() {
        const workItem = order?.reviewWorkItem
        const reason = releaseReason.trim()
        if (
            !workItem?.responsibilityActions.includes("RELEASE_TO_TEAM") ||
            !reason
        )
            return
        const payload = {
            kind: "RELEASE_TO_TEAM" as const,
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            reason,
        }
        const command = commandLedger.acquire(
            "review-responsibility",
            `w08:${workItem.workItemId}:${workItem.taskVersion}:release`,
            payload,
        )
        try {
            await responsibilityMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle("review-responsibility", "succeeded")
            setReleaseConfirmOpen(false)
            setReleaseReason("")
            setResult({
                status: "succeeded",
                title: responsibilityText.releaseToTeam,
                description: "当前审核任务仍为开放状态，已回到团队待处理。",
            })
            await refetch()
        } catch (error) {
            const settlement = classifyFormalCommandError(error)
            commandLedger.settle("review-responsibility", settlement)
            setResult({
                status: settlement === "unknown" ? "unknown" : "blocked",
                title: responsibilityText.changed,
                description: getErrorMessage(
                    error,
                    settlement === "unknown"
                        ? "处理结果待确认，请使用本次操作重试"
                        : "退回团队失败，请刷新后重试",
                ),
                reference:
                    settlement === "unknown" ? documentReference : undefined,
            })
        }
    }

    return {
        approveConfirmOpen,
        setApproveConfirmOpen,
        releaseConfirmOpen,
        setReleaseConfirmOpen,
        releaseReason,
        setReleaseReason,
        reviewPending: reviewMutation.isPending,
        responsibilityPending: responsibilityMutation.isPending,
        handleApprove,
        handleReject,
        handleStartProcessing,
        handleReleaseToTeam,
    }
}
