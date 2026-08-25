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
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    snapshot: SalesOrderSubmitSnapshot
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
}) {
    return (
        <SalesOrderSubmitConfirmDialog
            open={open}
            pending={pending}
            snapshot={snapshot}
            description="提交后进入销售领导 → 运营两级审批；任一层驳回后将从第一节点开始下一轮。"
            onOpenChange={onOpenChange}
            onConfirm={onConfirm}
        />
    )
}
