"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { ReceiptReversalApprovalArea } from "@/features/customer-receivables/components/receipt-reversal-approval-area"

/**
 * 回款冲正提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
 */
export function ReceiptReversalSubmitConfirmDialog({
    open,
    pending,
    approval,
    onOpenChange,
    onConfirm,
    id = "customer-receivables-reversal-submit-confirm-dialog",
    idPrefix,
}: {
    open: boolean
    pending: boolean
    approval?: DocumentApprovalView
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
    id?: string
    idPrefix?: string
}) {
    return (
        <FormalActionConfirmDialog
            id={idPrefix ?? id}
            open={open}
            onOpenChange={onOpenChange}
            actionLabel="提交冲正"
            confirmLabel="确认提交"
            fromStatus={{ label: "草稿", tone: "neutral" }}
            toStatus={{ label: "审批中", tone: "warning" }}
            description={
                <div className="space-y-3">
                    <p>确认后启动审批。任一层驳回后将从第一节点开始下一轮。</p>
                    <ReceiptReversalApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            lockedFields={["往来主体", "冲正金额", "已绑定的审批流程"]}
            effects={[
                "内容锁定并进入审批",
                "按已绑定的审批流程办理",
                "全部节点通过后过账并冲减原回款",
            ]}
            irreversibleEffects={["形成提交并进入审批"]}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
