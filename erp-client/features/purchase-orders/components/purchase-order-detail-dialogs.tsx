"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"

import { PurchaseOrderSubmitConfirmDialog } from "@/features/purchase-orders/components/purchase-order-submit-confirm-dialog"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

/**
 * 采购单详情命令确认框。提交确认嵌入通用审批路线，不选择下一节点或审批人。
 */
export function PurchaseOrderDetailDialogs({
    order,
    submitConfirmOpen,
    onSubmitConfirmOpenChange,
    approveConfirmOpen,
    onApproveConfirmOpenChange,
    voidConfirmOpen,
    onVoidConfirmOpenChange,
    changeConfirmOpen,
    onChangeConfirmOpenChange,
    leaveGuardOpen,
    onLeaveGuardOpenChange,
    submitPending,
    savePending,
    reviewPending,
    voidPending,
    changePending,
    onConfirmSubmit,
    onConfirmApprove,
    onConfirmVoid,
    onConfirmChange,
    onSaveAndLeave,
    onDiscardAndLeave,
}: {
    order: PurchaseOrderCenterView
    submitConfirmOpen: boolean
    onSubmitConfirmOpenChange: (open: boolean) => void
    approveConfirmOpen: boolean
    onApproveConfirmOpenChange: (open: boolean) => void
    voidConfirmOpen: boolean
    onVoidConfirmOpenChange: (open: boolean) => void
    changeConfirmOpen: boolean
    onChangeConfirmOpenChange: (open: boolean) => void
    leaveGuardOpen: boolean
    onLeaveGuardOpenChange: (open: boolean) => void
    submitPending: boolean
    savePending: boolean
    reviewPending: boolean
    voidPending: boolean
    changePending: boolean
    onConfirmSubmit: () => void
    onConfirmApprove: () => void
    onConfirmVoid: () => void
    onConfirmChange: () => void
    onSaveAndLeave: () => void
    onDiscardAndLeave: () => void
}) {
    return (
        <>
            <PurchaseOrderSubmitConfirmDialog
                open={submitConfirmOpen}
                pending={submitPending || savePending}
                approval={order.approval}
                onOpenChange={onSubmitConfirmOpenChange}
                onConfirm={onConfirmSubmit}
            />

            <FormalActionConfirmDialog
                open={approveConfirmOpen}
                onOpenChange={onApproveConfirmOpenChange}
                title="审批通过"
                actionLabel="通过"
                confirmLabel="确认通过"
                fromStatus={{ label: "审批中", tone: "warning" }}
                toStatus={{ label: "已生效", tone: "success" }}
                lockedFields={[
                    `本次审核的提交内容（销售单 ${order.header.salesOrderNo}）`,
                    "不可变提交头行与销售分配",
                ]}
                effects={[
                    "形成采购版本与应付原始分录",
                    "完成当前审核任务",
                    "不登记实际付款；履约受先款门禁约束",
                ]}
                nextDepartment="履约 / 付款"
                pending={reviewPending}
                onConfirm={onConfirmApprove}
            />

            <FormalActionConfirmDialog
                open={voidConfirmOpen}
                onOpenChange={onVoidConfirmOpenChange}
                title="作废采购草稿"
                actionLabel="作废"
                confirmLabel="确认作废"
                fromStatus={{ label: "草稿", tone: "neutral" }}
                toStatus={{ label: "已作废", tone: "destructive" }}
                lockedFields={[
                    `采购草稿 ${order.identity.draftLabel ?? order.identity.purchaseOrderId}`,
                    `来源销售单 ${order.header.salesOrderNo}`,
                ]}
                effects={[
                    "释放本草稿占用的销售待分配供给数量",
                    "同步更新供给分配任务和可选依据",
                ]}
                irreversibleEffects={["作废后的采购草稿不能恢复或再次提交"]}
                pending={voidPending}
                onConfirm={onConfirmVoid}
            />

            <FormalActionConfirmDialog
                open={changeConfirmOpen}
                onOpenChange={onChangeConfirmOpenChange}
                title="发起采购变更"
                actionLabel="创建变更"
                confirmLabel="创建工作副本"
                fromStatus={{
                    label: order.identity.statusLabel,
                    tone: order.identity.statusTone,
                }}
                toStatus={{ label: "变更工作副本", tone: "warning" }}
                lockedFields={[
                    `基准版本 v${order.identity.revisionNo ?? 1}`,
                    "已发生入库/发货/付款/发票记录不回退",
                ]}
                effects={[
                    "创建采购变更工作副本（同对象页签）",
                    "不得在原版本表单直接覆写",
                ]}
                pending={changePending}
                onConfirm={onConfirmChange}
            />

            <Dialog open={leaveGuardOpen} onOpenChange={onLeaveGuardOpenChange}>
                <DialogContent className="sm:max-w-md">
                    <DialogHeader>
                        <DialogTitle>有未保存的修改</DialogTitle>
                        <DialogDescription>
                            当前编辑内容尚未保存，离开后修改将丢失。建议先保存草稿。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <DialogClose
                            render={<Button type="button" variant="outline" />}
                        >
                            继续编辑
                        </DialogClose>
                        <Button
                            type="button"
                            variant="outline"
                            disabled={savePending}
                            onClick={onSaveAndLeave}
                        >
                            保存并离开
                        </Button>
                        <Button
                            type="button"
                            variant="destructive"
                            disabled={savePending}
                            onClick={onDiscardAndLeave}
                        >
                            放弃修改并离开
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    )
}
