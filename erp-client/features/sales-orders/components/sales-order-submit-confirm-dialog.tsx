"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import {
    SalesOrderSubmitConfirmSummary,
    type SalesOrderSubmitSnapshot,
} from "@/features/sales-orders/components/sales-order-submit-confirm-summary"

/**
 * 实物及服务销售单提交确认。
 *
 * 以本单摘要为主；不堆锁定字段、影响套话或审批路线卡片。
 * 确认层用横版：状态在标题行右侧，标题与说明左对齐。
 */
export function SalesOrderSubmitConfirmDialog({
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
            description="提交后进入审批；任一层驳回后将从第一节点开始下一轮。"
            layout="landscape"
            formContent={<SalesOrderSubmitConfirmSummary snapshot={snapshot} />}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
