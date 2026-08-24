"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import {
    SalesOrderSubmitConfirmSummary,
    type SalesOrderSubmitSnapshot,
} from "@/features/sales-orders/components/sales-order-submit-confirm-summary"

/**
 * 卡券销售单提交确认。
 *
 * 以本单摘要为主。卡券运营是普通单人节点，不得选择下一节点或审批人，也不展示审批路线卡片。
 * 确认层用横版：状态在标题行右侧，标题与说明左对齐。
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
        <FormalActionConfirmDialog
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交销售单"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description="提交后进入销售领导 → 运营两级审批；任一层驳回后将从第一节点开始下一轮。"
            layout="landscape"
            formContent={<SalesOrderSubmitConfirmSummary snapshot={snapshot} />}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
