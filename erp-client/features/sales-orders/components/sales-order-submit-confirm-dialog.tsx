"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { SalesOrderApprovalArea } from "@/features/sales-orders/components/sales-order-approval-area"

/**
 * 实物及服务销售单提交确认。
 *
 * 只展示服务端冻结路线与固定驳回说明，不得选择下一节点或审批人。
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
    snapshot?: {
        customerName: string
        contractLabel: string
        amountGross: string
        lineCount: number
    }
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
                    {snapshot ? (
                        <ul className="space-y-1 text-sm">
                            <li>客户：{snapshot.customerName || "—"}</li>
                            <li>合同：{snapshot.contractLabel || "—"}</li>
                            <li>含税合计：{snapshot.amountGross || "—"}</li>
                            <li>明细行数：{snapshot.lineCount}</li>
                        </ul>
                    ) : null}
                    <SalesOrderApprovalArea
                        phase="confirm"
                        approval={approval}
                    />
                </div>
            }
            lockedFields={["客户与合同", "销售明细", "已绑定的审批流程"]}
            effects={[
                "内容锁定并进入审批",
                "采购确认作为普通审批节点办理",
                "毛利风险仅作提示，不阻断提交",
            ]}
            irreversibleEffects={["形成提交并进入审批"]}
            pending={pending}
            onConfirm={() => {
                void onConfirm()
            }}
        />
    )
}
