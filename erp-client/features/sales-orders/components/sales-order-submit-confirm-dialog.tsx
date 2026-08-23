"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { SalesOrderApprovalArea } from "@/features/sales-orders/components/sales-order-approval-area"
import {
    SalesOrderSubmitConfirmSummary,
    type SalesOrderSubmitSnapshot,
} from "@/features/sales-orders/components/sales-order-submit-confirm-summary"

/**
 * 实物及服务销售单提交确认。
 *
 * 以本单摘要为主，仅保留简短审批提示与冻结路线；不堆锁定字段 / 影响套话。
 */
export function SalesOrderSubmitConfirmDialog({
    open,
    pending,
    approval,
    snapshot,
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    approval?: DocumentApprovalView
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
            formContent={
                <div className="space-y-3">
                    <SalesOrderSubmitConfirmSummary snapshot={snapshot} />
                    <SalesOrderApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
