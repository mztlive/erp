"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { VoucherSalesOrderApprovalArea } from "@/features/sales-orders/components/voucher-sales-order-approval-area"

/**
 * 卡券销售单提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明。卡券运营是普通单人节点，不得选择下一节点或审批人。
 */
export function VoucherSalesOrderSubmitConfirmDialog({
    open,
    pending,
    approval,
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    approval?: DocumentApprovalView
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
            description={
                <div className="space-y-3">
                    <p>确认后启动审批。任一层驳回后将从第一节点开始下一轮。</p>
                    <VoucherSalesOrderApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            lockedFields={["客户与合同", "卡券明细", "已绑定的审批流程"]}
            effects={[
                "内容锁定并进入审批",
                "卡券运营作为普通审批节点办理",
                "全部节点通过后本单生效",
            ]}
            irreversibleEffects={["形成提交并进入审批"]}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
