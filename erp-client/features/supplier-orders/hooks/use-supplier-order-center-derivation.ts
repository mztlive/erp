"use client"

import * as React from "react"

import type { ResponsibilityStatus } from "@/components/business/workflow-actions"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"
import { compactFixed, multiplyFixed, sumFixed } from "@/lib/fixed-decimal"

export function responsibilityOf(
    workItem: SupplierOrderDetailView["workItem"],
    currentUserId?: string,
): ResponsibilityStatus {
    if (!workItem) return "blocked"
    if (workItem.workItemStatus === "COMPLETED") return "completed"
    if (workItem.workItemStatus === "CLOSED") return "closed"
    if (workItem.processingState === "APPROVAL_BLOCKED") return "blocked"
    if (!workItem.ownerUser?.id || !currentUserId) return "assigned_to_other"
    return workItem.ownerUser.id === currentUserId
        ? "assigned_to_me"
        : "assigned_to_other"
}

export function deriveSupplierOrderTotals(
    items: SupplierOrderDetailView["items"],
): { totalQuantity: string; totalCostGross: string | null } {
    const totalQuantity = compactFixed(
        sumFixed(
            items.map((item) => item.quantity || "0"),
            { maxScale: 6, outputScale: 6 },
        ),
    )
    const totalCostGross = items.every((item) => item.unitCostGross == null)
        ? null
        : sumFixed(
              items.map((item) =>
                  multiplyFixed(
                      item.quantity || "0",
                      item.unitCostGross ?? "0",
                      {
                          leftMaxScale: 6,
                          rightMaxScale: 4,
                          outputScale: 2,
                      },
                  ),
              ),
              { maxScale: 2, outputScale: 2 },
          )
    return { totalQuantity, totalCostGross }
}

export type SupplierOrderCenterDerivation = {
    responsibilityStatus: ResponsibilityStatus
    completionEvidence:
        | NonNullable<SupplierOrderDetailView["lastInvestigation"]>
        | undefined
    canCompleteTask: boolean
    canQuery: boolean
    canReplay: boolean
    canReveal: boolean
    isResultUnknown: boolean
    noQueryCapability: boolean
    totalQuantity: string
    totalCostGross: string | null
}

/**
 * 中心页派生值：责任状态、完成任务前置条件、动作开关与商品合计。
 * detail 尚未加载时返回安全的空值。
 */
export function useSupplierOrderCenterDerivation(input: {
    detail: SupplierOrderDetailView | undefined
    currentUserId?: string
    latestInvestigation?: NonNullable<
        SupplierOrderDetailView["lastInvestigation"]
    >
}): SupplierOrderCenterDerivation {
    const { detail, currentUserId, latestInvestigation } = input

    const responsibilityStatus = React.useMemo(
        () => responsibilityOf(detail?.workItem, currentUserId),
        [detail?.workItem, currentUserId],
    )
    const completionEvidence = React.useMemo(
        () => detail?.lastInvestigation ?? latestInvestigation,
        [detail?.lastInvestigation, latestInvestigation],
    )
    const canCompleteTask = React.useMemo(() => {
        if (!detail) return false
        const evidence = completionEvidence
        return (
            responsibilityStatus === "assigned_to_me" &&
            detail.workItem?.workItemStatus === "OPEN" &&
            detail.allowedActions.includes(
                "CONFIRM_VERIFIED_TERMINAL_RESULT",
            ) &&
            evidence?.outcome === "VERIFIED_TERMINAL" &&
            Boolean(
                evidence.verifiedSupplierActionResultId &&
                evidence.verifiedResolution,
            )
        )
    }, [detail, responsibilityStatus, completionEvidence])
    const canQuery = React.useMemo(
        () => Boolean(detail?.allowedActions.includes("QUERY_RESULT")),
        [detail?.allowedActions],
    )
    const canReplay = React.useMemo(
        () => Boolean(detail?.allowedActions.includes("REPLAY")),
        [detail?.allowedActions],
    )
    const canReveal = React.useMemo(
        () => Boolean(detail?.allowedActions.includes("REVEAL_ADDRESS")),
        [detail?.allowedActions],
    )
    const isResultUnknown = React.useMemo(
        () => detail?.order.fulfillmentStatus === "RESULT_UNKNOWN",
        [detail?.order.fulfillmentStatus],
    )
    const noQueryCapability = React.useMemo(
        () =>
            Boolean(
                detail?.actionBlockers.some(
                    (b) =>
                        b.action === "QUERY_RESULT" &&
                        b.code === "NO_QUERY_CAPABILITY",
                ),
            ),
        [detail?.actionBlockers],
    )
    const totals = React.useMemo(
        () =>
            detail
                ? deriveSupplierOrderTotals(detail.items)
                : { totalQuantity: "0", totalCostGross: null },
        [detail],
    )

    return {
        responsibilityStatus,
        completionEvidence,
        canCompleteTask,
        canQuery,
        canReplay,
        canReveal,
        isResultUnknown,
        noQueryCapability,
        ...totals,
    }
}
