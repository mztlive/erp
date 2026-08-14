"use client"

import * as React from "react"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import type { PurchaseOrderDetailMode } from "@/features/purchase-orders/pages/purchase-order-detail-helpers"
import type { PurchaseOrderDetailLineEdits } from "@/features/purchase-orders/hooks/use-purchase-order-detail-edit-actions"

type UsePurchaseOrderDetailEditGuardInput = {
    mode: PurchaseOrderDetailMode
    order: PurchaseOrderCenterView | null | undefined
    paymentTermCode: string
    note: string
    lineEdits: PurchaseOrderDetailLineEdits
    onSave: () => Promise<boolean>
}

/**
 * 详情页编辑态守卫：脏检测、刷新/关页拦截与
 * 「保存并离开 / 放弃修改 / 继续编辑」确认。
 */
export function usePurchaseOrderDetailEditGuard({
    mode,
    order,
    paymentTermCode,
    note,
    lineEdits,
    onSave,
}: UsePurchaseOrderDetailEditGuardInput) {
    // 编辑态脏检测：行级数量/单价/税率或付款条件与当前内容不一致
    const editDirty = React.useMemo(() => {
        if (mode !== "edit" || !order) return false
        if (paymentTermCode !== order.header.paymentTermCode) return true
        if (note.trim()) return true
        return order.currentContent.lines.some((line) => {
            const edit = lineEdits[line.lineId]
            if (!edit) return false
            return (
                (edit.quantity ?? line.quantity) !== line.quantity ||
                (edit.unitCostGross ?? line.unitCostGross) !==
                    line.unitCostGross ||
                edit.inputTaxRate !== line.inputTaxRate
            )
        })
    }, [paymentTermCode, note, lineEdits, mode, order])

    // 编辑态刷新/关页守卫
    React.useEffect(() => {
        if (mode !== "edit" || !editDirty) return
        const onBeforeUnload = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = ""
        }
        window.addEventListener("beforeunload", onBeforeUnload)
        return () => window.removeEventListener("beforeunload", onBeforeUnload)
    }, [editDirty, mode])

    const [leaveGuardOpen, setLeaveGuardOpen] = React.useState(false)
    const [pendingLeave, setPendingLeave] = React.useState<(() => void) | null>(
        null,
    )

    /** 编辑态离开前弹「保存并离开 / 放弃修改 / 继续编辑」确认 */
    const requestLeave = React.useCallback(
        (go: () => void) => {
            if (mode === "edit" && editDirty) {
                setPendingLeave(() => go)
                setLeaveGuardOpen(true)
                return
            }
            go()
        },
        [editDirty, mode],
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
