"use client"

import * as React from "react"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export type PurchaseOrderDetailPermissions = {
    canEdit: boolean
    canSubmit: boolean
    canVoid: boolean
    canChange: boolean
    canFulfill: boolean
    canPay: boolean
    fulfillBlocker:
        | PurchaseOrderCenterView["actionBlockers"][number]
        | undefined
    changeBlocker: PurchaseOrderCenterView["actionBlockers"][number] | undefined
}

/**
 * 由采购单允许动作推导详情页各业务操作入口。
 */
export function usePurchaseOrderDetailPermissions(
    order: PurchaseOrderCenterView | null | undefined,
): PurchaseOrderDetailPermissions {
    return React.useMemo(() => {
        return {
            canEdit: order?.allowedActions.includes("EDIT") ?? false,
            canSubmit: order?.allowedActions.includes("SUBMIT") ?? false,
            canVoid: order?.allowedActions.includes("VOID") ?? false,
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
    }, [order])
}
