"use client"

import * as React from "react"
import { useRouter } from "next/navigation"

import { useSalesChangeReviewDecisionMutation } from "@/features/sales-orders/hooks/queries"
import type { SalesChangeOrderSummary } from "@/features/sales-orders/types"
import {
    mapWorkItemDto,
    useWorkItemDetailQuery,
    useWorkItemResponsibilityMutation,
} from "@/features/work-items"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"
import type { ResponsibilityStatus } from "@/components/business/workflow-actions"

export type SalesChangeReviewResult = {
    status: "succeeded" | "blocked" | "rejected" | "unknown"
    title: string
    description: string
    reference: string
    nextResponsible?: string
}

type UseSalesChangeReviewActionsProps = {
    salesOrderId: string
    changeOrder: SalesChangeOrderSummary | null
    workItemId: string
    returnTo: string
    onResult: (result: SalesChangeReviewResult) => void
}

/**
 * W05 销售变更影响/财务复核的动作状态与提交逻辑；由任务处理器与决定共同固定端点。
 */
export function useSalesChangeReviewActions({
    salesOrderId,
    changeOrder,
    workItemId,
    returnTo,
    onResult,
}: UseSalesChangeReviewActionsProps) {
    const router = useRouter()
    const workItemQuery = useWorkItemDetailQuery(workItemId)
    const responsibility = useWorkItemResponsibilityMutation()
    const decision = useSalesChangeReviewDecisionMutation()
    const ledger = React.useRef(new FormalCommandKeyLedger()).current
    const [approveOpen, setApproveOpen] = React.useState(false)
    const [rejectOpen, setRejectOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const workItem = workItemQuery.data
        ? mapWorkItemDto(workItemQuery.data)
        : null
    const handlerKey = workItem?.handlerKey
    const handlerMatches =
        handlerKey === "sales_change_impact_review" ||
        handlerKey === "sales_change_finance_review"
    const valid = Boolean(
        workItem &&
            changeOrder &&
            handlerMatches &&
            workItem.status === "OPEN" &&
            workItem.businessObjectType === "sales_change_review" &&
            workItem.rootBusinessObjectId === salesOrderId,
    )
    const canProcess = Boolean(
        valid && workItem?.allowedActions.includes("PROCESS"),
    )
    const canStart = Boolean(
        valid && workItem?.allowedActions.includes("START_PROCESSING"),
    )
    const canRelease = Boolean(
        valid && workItem?.allowedActions.includes("RELEASE_TO_TEAM"),
    )
    const responsibilityStatus: ResponsibilityStatus = !workItem
        ? "blocked"
        : workItem.status === "COMPLETED"
          ? "completed"
          : workItem.status === "CLOSED"
            ? "closed"
            : workItem.processingState === "APPROVAL_BLOCKED"
              ? "blocked"
              : canStart
                ? "pool_available"
                : canProcess
                  ? "assigned_to_me"
                  : "assigned_to_other"

    const submitDecision = async (nextDecision: "APPROVE" | "REJECT") => {
        if (!workItem || !changeOrder || !handlerMatches) return
        const normalizedReason = reason.trim()
        if (nextDecision === "REJECT" && !normalizedReason) {
            throw new Error("驳回原因不能为空")
        }
        const slot = `${workItem.workItemId}:${nextDecision}`
        const payload = {
            salesChangeOrderId: changeOrder.id,
            handlerKey,
            decision: nextDecision,
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            expectedSubjectVersion: workItem.subjectVersion,
            decisionReason: normalizedReason || undefined,
        } as const
        const command = ledger.acquire(
            slot,
            `w05-change-review:${workItem.workItemId}:${nextDecision}`,
            payload,
        )
        try {
            const changed = await decision.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            ledger.settle(slot, "succeeded")
            onResult({
                status: nextDecision === "APPROVE" ? "succeeded" : "rejected",
                title:
                    nextDecision === "APPROVE"
                        ? "销售变更复核已通过"
                        : "销售变更复核已驳回",
                description:
                    nextDecision === "APPROVE"
                        ? handlerKey === "sales_change_impact_review"
                            ? "已形成财务复核任务。"
                            : "变更已形成新的正式销售版本。"
                        : "变更单已退回修改。",
                reference: changed.id,
                nextResponsible:
                    nextDecision === "APPROVE" &&
                    handlerKey === "sales_change_impact_review"
                        ? "财务"
                        : undefined,
            })
        } catch (error) {
            ledger.settle(slot, classifyFormalCommandError(error))
            throw error
        }
    }

    const startProcessing = async () => {
        if (!workItem) return
        await responsibility.mutateAsync({
            kind: "START_PROCESSING",
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            idempotencyKey: `w05:${workItem.workItemId}:${workItem.taskVersion}:START`,
        })
        await workItemQuery.refetch()
    }

    const releaseToTeam = async () => {
        if (!workItem) return
        await responsibility.mutateAsync({
            kind: "RELEASE_TO_TEAM",
            workItemId: workItem.workItemId,
            expectedTaskVersion: workItem.taskVersion,
            reason: "当前处理人退回责任池",
            idempotencyKey: `w05:${workItem.workItemId}:${workItem.taskVersion}:RELEASE`,
        })
        await workItemQuery.refetch()
    }

    return {
        router,
        workItemQuery,
        responsibility,
        decision,
        workItem,
        handlerKey,
        handlerMatches,
        valid,
        canProcess,
        canStart,
        canRelease,
        responsibilityStatus,
        approveOpen,
        setApproveOpen,
        rejectOpen,
        setRejectOpen,
        reason,
        setReason,
        submitDecision,
        startProcessing,
        releaseToTeam,
        returnTo,
    }
}

export type SalesChangeReviewActions = ReturnType<
    typeof useSalesChangeReviewActions
>
