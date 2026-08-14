"use client"

import * as React from "react"
import { useQueryClient } from "@tanstack/react-query"

import { useAppForm } from "@/components/form"
import {
    actionKey,
    approvalDecision,
    cancelActionKey,
    isUncertainResult,
    rejectSchema,
    type ApprovalResult,
} from "@/features/sales-orders/hooks/use-card-approval-model"
import {
    salesOrderKeys,
    useCancelCardSalesApprovalMutation,
    useSubmitCardSalesApprovalDecisionMutation,
} from "@/features/sales-orders/hooks/queries"
import type {
    CardSalesApproval,
    SalesOrderListItem,
} from "@/features/sales-orders/types"
import { useWorkItemResponsibilityMutation } from "@/features/work-items"
import { getErrorPresentation } from "@/lib/api/errors"

export type { ApprovalResult } from "@/features/sales-orders/hooks/use-card-approval-model"
export {
    actionKey,
    approvalDecision,
    cancelActionKey,
    isUncertainResult,
    rejectSchema,
} from "@/features/sales-orders/hooks/use-card-approval-model"

type CardSalesApprovalActionsProps = {
    order: SalesOrderListItem
    approval: CardSalesApproval
    onResult?: (result: ApprovalResult) => void
}

/**
 * 卡券审批对象中心的动作状态与提交逻辑；动作资格完全取自服务端活动步骤投影。
 */
export function useCardSalesApprovalActions({
    order,
    approval,
    onResult,
}: CardSalesApprovalActionsProps) {
    const queryClient = useQueryClient()
    const responsibilityMutation = useWorkItemResponsibilityMutation()
    const decisionMutation = useSubmitCardSalesApprovalDecisionMutation()
    const { mutateAsync: cancelApproval, isPending: isCancelling } =
        useCancelCardSalesApprovalMutation()
    const [confirmApprove, setConfirmApprove] = React.useState(false)
    const [confirmReject, setConfirmReject] = React.useState(false)
    const [confirmTerminate, setConfirmTerminate] = React.useState(false)
    const [confirmCancel, setConfirmCancel] = React.useState(false)
    const [rejectPayload, setRejectPayload] = React.useState<{
        reasonCode: string
        comment: string
    } | null>(null)
    const [terminatePayload, setTerminatePayload] = React.useState<{
        reasonCode: string
        comment: string
    } | null>(null)

    const rejectForm = useAppForm({
        defaultValues: { reasonCode: "", comment: "" },
        validators: { onChange: rejectSchema },
        onSubmit: async ({ value }) => {
            setRejectPayload({
                reasonCode: value.reasonCode.trim(),
                comment: value.comment.trim(),
            })
            setConfirmReject(true)
        },
    })
    const terminateForm = useAppForm({
        defaultValues: { reasonCode: "", comment: "" },
        validators: { onChange: rejectSchema },
        onSubmit: async ({ value }) => {
            setTerminatePayload({
                reasonCode: value.reasonCode.trim(),
                comment: value.comment.trim(),
            })
            setConfirmTerminate(true)
        },
    })

    const publishResult = React.useCallback(
        (next: ApprovalResult) => onResult?.(next),
        [onResult],
    )
    const actionableApproval =
        approval.processingState === "READY" ? approval : null
    const isManager = approval.expectedReviewStatus === "PENDING_SALES_LEAD"
    const isReady = actionableApproval?.workItemStatus === "OPEN"
    const canStart =
        isReady &&
        actionableApproval?.assignmentMode === "POOL" &&
        approval.allowedActions.includes("START_PROCESSING")
    const canApprove = isReady && approval.allowedActions.includes("APPROVE")
    const canReject = isReady && approval.allowedActions.includes("REJECT")
    const canTerminate =
        isReady && approval.allowedActions.includes("TERMINATE")
    const canCancel = approval.allowedActions.includes("CANCEL")

    const submitDecision = React.useCallback(
        async (
            reviewDecision: "APPROVE" | "REJECT" | "TERMINATE",
            decisionReason?: { reasonCode: string; comment: string },
        ) => {
            if (!actionableApproval) {
                throw new Error("审批当前受阻，不能提交普通决定")
            }
            const result = await decisionMutation.mutateAsync({
                approvalInstanceId: actionableApproval.approvalInstanceId,
                expectedInstanceVersion: actionableApproval.instanceVersion,
                approvalStepInstanceId:
                    actionableApproval.approvalStepInstanceId,
                expectedStepVersion: actionableApproval.stepVersion,
                workItemId: actionableApproval.workItemId,
                expectedTaskVersion: actionableApproval.taskVersion,
                expectedSubjectVersion: actionableApproval.subjectVersion,
                decision: approvalDecision(
                    order,
                    actionableApproval,
                    reviewDecision,
                    decisionReason,
                ),
                idempotencyKey: actionKey(actionableApproval, reviewDecision),
            })
            return result
        },
        [actionableApproval, decisionMutation, order],
    )

    const startProcessing = async () => {
        if (!actionableApproval) return
        const idempotencyKey = actionKey(actionableApproval, "START_PROCESSING")
        try {
            await responsibilityMutation.mutateAsync({
                kind: "START_PROCESSING",
                workItemId: actionableApproval.workItemId,
                expectedTaskVersion: actionableApproval.taskVersion,
                idempotencyKey,
            })
            await queryClient.invalidateQueries({
                queryKey: salesOrderKeys.detail(order.id),
            })
            publishResult({
                status: "succeeded",
                title: "已开始处理",
                description: "当前审批已分配给你；页面刷新后可提交审批决定。",
                reference: order.documentNumber,
            })
        } catch (error) {
            if (isUncertainResult(error)) {
                publishResult({
                    status: "unknown",
                    title: "开始处理结果待确认",
                    description:
                        "请求结果尚未确认。请刷新销售单核对当前责任，不要重复操作。",
                    reference: idempotencyKey,
                })
                return
            }
            const failure = getErrorPresentation(
                error,
                "开始处理失败，请刷新任务责任后重试。",
            )
            publishResult({
                status: "blocked",
                title: failure.title,
                description: failure.description,
                reference: order.documentNumber,
            })
        }
    }

    const confirmApproveDecision = async () => {
        if (!canApprove) return
        try {
            const result = await submitDecision("APPROVE")
            const outcome = result.business_result
            const blocked = result.approval.instance.status === "BLOCKED"
            publishResult({
                status: blocked ? "blocked" : "succeeded",
                title: blocked
                    ? "领导决定已保存，运营步骤等待恢复"
                    : outcome.outcome === "MANAGER_APPROVED"
                      ? "领导已通过，请运营继续审批"
                      : "运营已通过，销售单已生效",
                description: blocked
                    ? "下一步骤责任解析失败，未形成开放运营任务；请由审批管理员恢复当前步骤。"
                    : outcome.outcome === "MANAGER_APPROVED"
                      ? "已激活唯一运营步骤并形成新的开放任务。"
                      : "销售版本、应收与执行投影已由同一事务形成。",
                reference: outcome.workflow_action_id,
                nextResponsible: blocked
                    ? "审批管理员"
                    : outcome.outcome === "MANAGER_APPROVED"
                      ? "运营"
                      : "票款与商城执行",
            })
        } catch (error) {
            if (isUncertainResult(error)) {
                publishResult({
                    status: "unknown",
                    title: "审批结果待确认",
                    description:
                        "请求结果尚未确认。请刷新审批状态，确认结果后再继续。",
                    reference: actionKey(actionableApproval!, "APPROVE"),
                })
                return
            }
            const failure = getErrorPresentation(
                error,
                "审批结果未确认；请刷新实例、步骤和任务版本后再处理。",
            )
            publishResult({
                status: "blocked",
                title: failure.title,
                description: failure.description,
                reference: order.documentNumber,
            })
        }
    }

    const confirmRejectDecision = async () => {
        if (!canReject || !rejectPayload) return
        try {
            const result = await submitDecision("REJECT", rejectPayload)
            const outcome = result.business_result
            publishResult({
                status: "rejected",
                title: "已驳回，请销售修改后重提",
                description:
                    "当前审批实例已结束；修改后将从领导审批重新开始。",
                reference: outcome.workflow_action_id,
                nextResponsible: "销售",
            })
        } catch (error) {
            if (isUncertainResult(error)) {
                publishResult({
                    status: "unknown",
                    title: "驳回结果待确认",
                    description:
                        "请求结果尚未确认。请刷新审批状态，确认结果后再继续。",
                    reference: actionKey(actionableApproval!, "REJECT"),
                })
                return
            }
            const failure = getErrorPresentation(
                error,
                "驳回结果未确认；请刷新实例、步骤和任务版本后再处理。",
            )
            publishResult({
                status: "blocked",
                title: failure.title,
                description: failure.description,
                reference: order.documentNumber,
            })
        }
    }

    const confirmTerminateDecision = async () => {
        if (!canTerminate || !terminatePayload) return
        try {
            const result = await submitDecision("TERMINATE", terminatePayload)
            const outcome = result.business_result
            publishResult({
                status: "succeeded",
                title: "审批已终止",
                description:
                    "本次审批已结束，冻结提交已失效；销售可重新编辑。",
                reference: outcome.workflow_action_id,
                nextResponsible: "销售",
            })
        } catch (error) {
            if (isUncertainResult(error)) {
                publishResult({
                    status: "unknown",
                    title: "终止结果待确认",
                    description:
                        "请求结果尚未确认。请刷新审批状态，确认结果后再继续。",
                    reference: actionKey(actionableApproval!, "TERMINATE"),
                })
                return
            }
            const failure = getErrorPresentation(
                error,
                "终止结果未确认；请刷新实例、步骤和任务版本后再处理。",
            )
            publishResult({
                status: "blocked",
                title: failure.title,
                description: failure.description,
                reference: order.documentNumber,
            })
        }
    }

    const confirmCancelDecision = async () => {
        if (!canCancel) return
        const idempotencyKey = cancelActionKey(approval)
        try {
            const result = await cancelApproval({
                approvalInstanceId: approval.approvalInstanceId,
                currentStepInstanceId: approval.approvalStepInstanceId,
                workItemId: approval.workItemId,
                expectedInstanceVersion: approval.instanceVersion,
                expectedStepVersion: approval.stepVersion,
                expectedTaskVersion: approval.taskVersion,
                expectedSubjectVersion: approval.subjectVersion,
                reason: "申请人撤回并继续修改",
                idempotencyKey,
            })
            publishResult({
                status: "succeeded",
                title: "审批已撤回",
                description:
                    result.business_result.sales_order_commercial_status ===
                    "DRAFT"
                        ? "销售单已恢复为草稿，可以修改后重新提交。"
                        : "审批已取消，请刷新销售单确认当前状态。",
                reference: order.documentNumber,
                nextResponsible: "销售",
            })
        } catch (error) {
            if (isUncertainResult(error)) {
                publishResult({
                    status: "unknown",
                    title: "撤回结果待确认",
                    description:
                        "请求结果尚未确认。请刷新审批状态，不要重复撤回。",
                    reference: order.documentNumber,
                })
                return
            }
            const failure = getErrorPresentation(
                error,
                "撤回失败；请刷新审批、步骤和待处理事项版本后重试。",
            )
            publishResult({
                status: "blocked",
                title: failure.title,
                description: failure.description,
                reference: order.documentNumber,
            })
        }
    }

    return {
        isManager,
        isReady,
        canStart,
        canApprove,
        canReject,
        canTerminate,
        canCancel,
        isCancelling,
        isDecisionPending: decisionMutation.isPending,
        isStartPending: responsibilityMutation.isPending,
        confirmApprove,
        setConfirmApprove,
        confirmReject,
        setConfirmReject,
        confirmTerminate,
        setConfirmTerminate,
        confirmCancel,
        setConfirmCancel,
        rejectForm,
        terminateForm,
        startProcessing,
        confirmApproveDecision,
        confirmRejectDecision,
        confirmTerminateDecision,
        confirmCancelDecision,
    }
}

export type CardSalesApprovalActions = ReturnType<
    typeof useCardSalesApprovalActions
>
