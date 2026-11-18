"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"

import { sourceReturnProgress } from "../lib/source-return-progress"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export type PurchaseOrderDetailPermissions = {
    canEdit: boolean
    canSubmit: boolean
    canVoid: boolean
    canChange: boolean
    canContinueChange: boolean
}

/**
 * 由采购单允许动作推导详情页各业务操作入口。
 */
export function usePurchaseOrderDetailPermissions(
    order: PurchaseOrderCenterView | null | undefined,
): PurchaseOrderDetailPermissions {
    const profile = useAccountProfileQuery()
    const grants = profile.data?.permissions
    return React.useMemo(() => {
        return {
            canEdit:
                hasPermission(grants, "purchase_order:update") &&
                (order?.allowedActions.includes("EDIT") ?? false),
            canSubmit:
                hasPermission(grants, "purchase_order:submit") &&
                (order?.allowedActions.includes("SUBMIT") ?? false),
            canVoid:
                hasPermission(grants, "purchase_order:delete") &&
                (order?.allowedActions.includes("VOID") ?? false),
            canContinueChange: hasPermission(
                grants,
                "purchase_change_order:submit",
            ),
            canChange:
                hasPermission(grants, "purchase_change_order:create") &&
                !!order?.allowedActions.includes("START_CHANGE") &&
                !sourceReturnProgress(order).blocksChange,
        }
    }, [grants, order])
}
