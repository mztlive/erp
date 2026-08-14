"use client"

import * as React from "react"

import { FormalCommandKeyLedger } from "@/lib/formal-command"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export type PurchaseOrderDetailPermissions = {
    canEdit: boolean
    canSubmit: boolean
    canOpenReview: boolean
    canApprove: boolean
    canReject: boolean
    canStartReview: boolean
    canReleaseReview: boolean
    canChange: boolean
    canFulfill: boolean
    canPay: boolean
    fulfillBlocker:
        | PurchaseOrderCenterView["actionBlockers"][number]
        | undefined
    changeBlocker:
        | PurchaseOrderCenterView["actionBlockers"][number]
        | undefined
}

/**
 * 由采购单允许动作、审核任务处理态与命令账本待确认结果
 * 推导详情页各操作入口的可用性。
 */
export function usePurchaseOrderDetailPermissions(
    order: PurchaseOrderCenterView | null | undefined,
    commandLedger: FormalCommandKeyLedger,
): PurchaseOrderDetailPermissions {
    return React.useMemo(() => {
        const reviewWorkItem = order?.reviewWorkItem
        const approveResultPending = Boolean(
            commandLedger.peek("review-approve"),
        )
        const rejectResultPending = Boolean(commandLedger.peek("review-reject"))
        const responsibilityResultPending = Boolean(
            commandLedger.peek("review-responsibility"),
        )
        return {
            canEdit: order?.allowedActions.includes("EDIT") ?? false,
            canSubmit: order?.allowedActions.includes("SUBMIT") ?? false,
            canOpenReview: Boolean(reviewWorkItem),
            canApprove: Boolean(
                reviewWorkItem?.processingState === "READY" &&
                    reviewWorkItem.domainAllowedActions.includes("APPROVE") &&
                    !rejectResultPending &&
                    !responsibilityResultPending,
            ),
            canReject: Boolean(
                reviewWorkItem?.processingState === "READY" &&
                    reviewWorkItem.domainAllowedActions.includes("REJECT") &&
                    !approveResultPending &&
                    !responsibilityResultPending,
            ),
            canStartReview: Boolean(
                reviewWorkItem?.processingState === "READY" &&
                    reviewWorkItem.responsibilityActions.includes(
                        "START_PROCESSING",
                    ) &&
                    !approveResultPending &&
                    !rejectResultPending,
            ),
            canReleaseReview: Boolean(
                reviewWorkItem?.processingState === "READY" &&
                    reviewWorkItem.responsibilityActions.includes(
                        "RELEASE_TO_TEAM",
                    ) &&
                    !approveResultPending &&
                    !rejectResultPending,
            ),
            canChange: order?.allowedActions.includes("START_CHANGE") ?? false,
            canFulfill: order?.allowedActions.includes("FULFILL") ?? false,
            canPay: order?.allowedActions.includes("PAY") ?? false,
            fulfillBlocker: order?.actionBlockers.find(
                (b) => b.action === "FULFILL",
            ),
            changeBlocker: order?.actionBlockers.find(
                (b) => b.action === "START_CHANGE",
            ),
        }
    }, [order, commandLedger])
}
