"use client"

import { useEffect } from "react"
import { FormalActionResult } from "@/components/business"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { usePurchaseOrderCenterQuery } from "../hooks/queries"
import { usePurchaseOrderDetailCommandState } from "../hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "../hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailEditGuard } from "../hooks/use-purchase-order-detail-edit-guard"
import { usePurchaseOrderDetailPermissions } from "../hooks/use-purchase-order-detail-permissions"
import type { PurchaseOrderCenterView } from "../types"
import { EditSurface } from "./purchase-order-surfaces"
import { PurchaseOrderDetailDialogs } from "./purchase-order-detail-dialogs"
import { PurchaseOrderSourceUpdateNotice } from "./purchase-order-source-update-notice"

/** 弹窗内复用采购草稿编辑与提交，离开前保留未保存确认。 */
export function PurchaseSourceUpdateDraftDialog({
    order,
    onClose,
}: {
    order: PurchaseOrderCenterView
    onClose: () => void
}) {
    const purchaseOrderId = order.identity.purchaseOrderId
    const query = usePurchaseOrderCenterQuery(purchaseOrderId)
    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)
    const permissions = usePurchaseOrderDetailPermissions(order)
    const actions = usePurchaseOrderDetailEditActions({
        purchaseOrderId,
        order,
        mode: "edit",
        refetch: query.refetch,
        commandLedger,
        setResult,
        getPaymentTermCode: () => order.header.paymentTermCode,
        setDraftPaymentTermCode: () => undefined,
        onFinished: onClose,
    })
    const guard = usePurchaseOrderDetailEditGuard({
        mode: "edit",
        order,
        paymentTermCode: order.header.paymentTermCode,
        note: "",
        lineEdits: actions.lineEdits,
        onSave: actions.handleSave,
    })
    const pending = actions.savePending || actions.submitPending
    const uncertain = result?.status === "unknown"

    useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (pending || actions.submitConfirmOpen || !permissions.canEdit)
                return
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key.toLowerCase() === "s"
            ) {
                event.preventDefault()
                void actions.handleSave()
            }
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                permissions.canSubmit
            ) {
                event.preventDefault()
                actions.setSubmitConfirmOpen(true)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [actions, pending, permissions.canEdit, permissions.canSubmit])

    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open && !pending && !uncertain)
                        guard.requestLeave(onClose)
                }}
            >
                <DialogContent
                    className="flex max-h-[90vh] flex-col sm:max-w-5xl"
                    closeButtonId="purchase-source-update-draft-close"
                >
                    <DialogHeader>
                        <DialogTitle>修改采购原单</DialogTitle>
                        <DialogDescription>
                            按来源销售的新要求调整。保存后仍需提交审批，采购依据更新才算处理完成。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="min-h-0 overflow-auto space-y-4">
                        {result ? <FormalActionResult {...result} /> : null}
                        <PurchaseOrderSourceUpdateNotice
                            order={order}
                            showActions={false}
                            canEdit={false}
                            canVoid={false}
                            canChange={false}
                            onEdit={() => undefined}
                            onRequestVoid={() => undefined}
                            onRequestChange={() => undefined}
                        />
                        <fieldset
                            disabled={pending || !permissions.canEdit}
                            className="min-w-0"
                        >
                            <EditSurface
                                order={order}
                                lineEdits={actions.lineEdits}
                                setLineEdits={actions.setLineEdits}
                                draftEditToken={actions.draftEditToken}
                                canSubmit={permissions.canSubmit}
                                savePending={actions.savePending}
                                onSave={() => void actions.handleSave()}
                                onSubmitOpen={() =>
                                    actions.setSubmitConfirmOpen(true)
                                }
                            />
                        </fieldset>
                    </div>
                </DialogContent>
            </Dialog>
            <PurchaseOrderDetailDialogs
                order={order}
                submitConfirmOpen={actions.submitConfirmOpen}
                onSubmitConfirmOpenChange={actions.setSubmitConfirmOpen}
                voidConfirmOpen={false}
                onVoidConfirmOpenChange={() => undefined}
                leaveGuardOpen={guard.leaveGuardOpen}
                onLeaveGuardOpenChange={guard.setLeaveGuardOpen}
                submitPending={actions.submitPending}
                savePending={actions.savePending}
                voidPending={false}
                onConfirmSubmit={() => void actions.handleSubmit()}
                onConfirmVoid={() => undefined}
                onSaveAndLeave={() => void guard.saveAndLeave()}
                onDiscardAndLeave={guard.discardAndLeave}
            />
        </>
    )
}
