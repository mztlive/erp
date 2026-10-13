"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
    voidConfirmOpen,
    onVoidConfirmOpenChange,
    leaveGuardOpen,
    onLeaveGuardOpenChange,
    submitPending,
    savePending,
    voidPending,
    onConfirmSubmit,
    onConfirmVoid,
    onSaveAndLeave,
    onDiscardAndLeave,
}: {
    order: PurchaseOrderCenterView
    submitConfirmOpen: boolean
    onSubmitConfirmOpenChange: (open: boolean) => void
    voidConfirmOpen: boolean
    onVoidConfirmOpenChange: (open: boolean) => void
    leaveGuardOpen: boolean
    onLeaveGuardOpenChange: (open: boolean) => void
    submitPending: boolean
    savePending: boolean
    voidPending: boolean
    onConfirmSubmit: () => void
    onConfirmVoid: () => void
    onSaveAndLeave: () => void
    onDiscardAndLeave: () => void
}) {
    return (
        <>
            <PurchaseOrderSubmitConfirmDialog
                idPrefix="procurement-orders-detail-submit-confirm"
                open={submitConfirmOpen}
                pending={submitPending || savePending}
                approval={order.approval}
                onOpenChange={onSubmitConfirmOpenChange}
                onConfirm={onConfirmSubmit}
            />

            <FormalActionConfirmDialog
                actionVariant="destructive"
                idPrefix="procurement-orders-detail-void-confirm"
                open={voidConfirmOpen}
                onOpenChange={onVoidConfirmOpenChange}
                title="作废采购草稿"
                actionLabel="作废"
                confirmLabel="确认作废"
                fromStatus={{ label: "草稿", tone: "neutral" }}
                toStatus={{ label: "已作废", tone: "destructive" }}
                summary={[
                    order.identity.draftLabel?.trim() || "采购草稿",
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

            <Dialog open={leaveGuardOpen} onOpenChange={onLeaveGuardOpenChange}>
                <DialogContent
                    className="sm:max-w-md"
                    closeButtonId="procurement-orders-detail-leave-guard-close"
                >
                    <DialogHeader>
                        <DialogTitle>有未保存的修改</DialogTitle>
                        <DialogDescription>
                            当前编辑内容尚未保存，离开后修改将丢失。建议先保存草稿。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    id="procurement-orders-detail-leave-continue"
                                    type="button"
                                    variant="outline"
                                />
                            }
                        >
                            继续编辑
                        </DialogClose>
                        <LoadingButton
                            id="procurement-orders-detail-leave-save"
                            loading={savePending}
                            type="button"
                            variant="outline"
                            disabled={savePending}
                            onClick={onSaveAndLeave}
                        >
                            {savePending ? "保存中…" : "保存并离开"}
                        </LoadingButton>
                        <Button
                            id="procurement-orders-detail-leave-discard"
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
