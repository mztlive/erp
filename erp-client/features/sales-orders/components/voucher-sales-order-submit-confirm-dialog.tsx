"use client"

import { SalesOrderSubmitConfirmDialog } from "@/features/sales-orders/components/sales-order-submit-confirm-dialog"
import type { SalesOrderSubmitSnapshot } from "@/features/sales-orders/components/sales-order-submit-confirm-summary"

/**
 * 卡券销售单提交确认：与实物单共用纸质预览，说明文案按卡券审批路径。
 */
export function VoucherSalesOrderSubmitConfirmDialog({
    open,
    pending,
    snapshot,
    resubmitting,
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    snapshot: SalesOrderSubmitSnapshot
    resubmitting?: boolean
    onOpenChange: (open: boolean) => void
    onConfirm: (resubmissionReason?: string) => void | Promise<void>
}) {
    return (
        <SalesOrderSubmitConfirmDialog
            open={open}
            pending={pending}
            snapshot={snapshot}
            resubmitting={resubmitting}
            description="提交后按已绑定的审批流程办理；任一节点驳回将结束本次审批，退回提交人处理。"
            onOpenChange={onOpenChange}
            onConfirm={onConfirm}
        />
    )
}
