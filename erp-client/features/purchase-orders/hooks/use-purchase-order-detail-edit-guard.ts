"use client"

import * as React from "react"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import type { PurchaseOrderDetailMode } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderDetailLineEdits } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-actions"
import { draftLinesDirty } from "@/features/purchase-orders/lib/draft-line-payload"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"

type UsePurchaseOrderDetailEditGuardInput = {
    mode?: PurchaseOrderDetailMode
    /** 统一编辑态判定（含 permissions.canEdit）。 */
    isEditing?: boolean
    order: PurchaseOrderCenterView | null | undefined
    paymentTermCode?: string
    note?: string
    lineEdits: PurchaseOrderDetailLineEdits
    busy?: boolean
    onSave: () => Promise<boolean>
}

/**
 * 详情页编辑态守卫：脏检测、工作台未保存状态上报（含页签圆点、关闭拦截与独立 beforeunload）
 * 与「保存并离开 / 放弃修改 / 继续编辑」确认。
 */
export function usePurchaseOrderDetailEditGuard({
    mode,
    isEditing: isEditingProp,
    order,
    paymentTermCode,
    note,
    lineEdits,
    busy = false,
    onSave,
}: UsePurchaseOrderDetailEditGuardInput) {
    const activeEditing = isEditingProp ?? mode === "edit"

    // 编辑态脏检测：行级数量/单价/税率/交付安排与当前内容不一致
    const editDirty = React.useMemo(() => {
        if (!activeEditing || !order) return false
        if (
            paymentTermCode !== undefined &&
            paymentTermCode !== order.header.paymentTermCode
        )
            return true
        if (note && note.trim()) return true
        return draftLinesDirty(order, lineEdits)
    }, [activeEditing, lineEdits, note, order, paymentTermCode])

    useWorkspaceDirtyState({
        dirty: activeEditing && editDirty,
        busy,
    })

    const [leaveGuardOpen, setLeaveGuardOpen] = React.useState(false)
    const [pendingLeave, setPendingLeave] = React.useState<(() => void) | null>(
        null,
    )

    /** 编辑态离开前弹「保存并离开 / 放弃修改 / 继续编辑」确认 */
    const requestLeave = React.useCallback(
        (go: () => void) => {
            if (activeEditing && editDirty) {
                setPendingLeave(() => go)
                setLeaveGuardOpen(true)
                return
            }
            go()
        },
        [activeEditing, editDirty],
    )

    const saveAndLeave = React.useCallback(async () => {
        const ok = await onSave()
        if (!ok) return
        setLeaveGuardOpen(false)
        pendingLeave?.()
    }, [onSave, pendingLeave])

    const discardAndLeave = React.useCallback(() => {
        setLeaveGuardOpen(false)
        pendingLeave?.()
    }, [pendingLeave])

    return {
        editDirty,
        requestLeave,
        leaveGuardOpen,
        setLeaveGuardOpen,
        saveAndLeave,
        discardAndLeave,
    }
}
