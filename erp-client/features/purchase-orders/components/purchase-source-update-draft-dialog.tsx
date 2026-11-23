"use client"

import { useEffect } from "react"
import { ArrowLeftIcon, ArrowRightIcon, CheckIcon } from "lucide-react"
import { FormalActionResult } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import { usePurchaseOrderCenterQuery } from "../hooks/queries"
import { usePurchaseOrderDetailCommandState } from "../hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditActions } from "../hooks/use-purchase-order-detail-edit-actions"
import { usePurchaseOrderDetailEditGuard } from "../hooks/use-purchase-order-detail-edit-guard"
import { usePurchaseOrderDetailPermissions } from "../hooks/use-purchase-order-detail-permissions"
import type { PurchaseOrderCenterView } from "../types"
import { PurchaseOrderApprovalArea } from "./purchase-order-approval-area"
import { EditSurface } from "./purchase-order-surfaces"
import { PurchaseOrderSourceUpdateNotice } from "./purchase-order-source-update-notice"

/** 弹窗内完成采购草稿编辑与影响核对两步提交，离开前以行内提示确认未保存修改，杜绝弹窗套娃。 */
export function PurchaseSourceUpdateDraftDialog({
    order,
    onClose,
    onSubmitted,
}: {
    order: PurchaseOrderCenterView
    onClose: () => void
    onSubmitted?: () => void
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
        isEditing: permissions.canEdit,
        refetch: query.refetch,
        commandLedger,
        setResult,
        onFinished: () => {
            onSubmitted?.()
            onClose()
        },
    })
    const guard = usePurchaseOrderDetailEditGuard({
        mode: "edit",
        isEditing: permissions.canEdit,
        order,
        lineEdits: actions.lineEdits,
        busy: actions.savePending || actions.submitPending,
        onSave: actions.handleSave,
    })
    const pending = actions.savePending || actions.submitPending
    const uncertain = result?.status === "unknown"
    const step = actions.submitConfirmOpen ? "confirm" : "edit"
    const resubmitting =
        order.identity.status === "REJECTED" ||
        order.approval?.instance?.status === "REJECTED"
    const rework = useResubmissionReason({
        open: step === "confirm",
        required: resubmitting,
        id: "procurement-orders-detail-submit-confirm-resubmission-reason",
        disabled: pending,
    })

    const changedLines = order.currentContent.lines.filter((line) => {
        const draft = actions.lineEdits[line.lineId]
        if (!draft) return false
        return (
            (draft.quantity !== undefined &&
                draft.quantity !== line.quantity) ||
            (draft.unitCostGross !== undefined &&
                draft.unitCostGross !== line.unitCostGross) ||
            (draft.inputTaxRate !== undefined &&
                draft.inputTaxRate !== line.inputTaxRate) ||
            (draft.expectedDeliveryDate !== undefined &&
                draft.expectedDeliveryDate !==
                    (line.expectedDeliveryDate ?? "")) ||
            draft.deliveryAllocations !== undefined
        )
    })

    useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (pending || !permissions.canEdit) return
            if (step === "edit") {
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
                    guard.setLeaveGuardOpen(false)
                    actions.setSubmitConfirmOpen(true)
                }
                return
            }
            if (
                (event.metaKey || event.ctrlKey) &&
                event.key === "Enter" &&
                rework.valid
            ) {
                event.preventDefault()
                void actions.handleSubmit(rework.reason)
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [
        actions,
        guard,
        pending,
        permissions.canEdit,
        permissions.canSubmit,
        rework.reason,
        rework.valid,
        step,
    ])

    const requestClose = () => {
        if (pending || uncertain) return
        if (step === "confirm") {
            actions.setSubmitConfirmOpen(false)
        }
        guard.requestLeave(onClose)
    }

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) requestClose()
            }}
        >
            <DialogContent
                size="2xl"
                scroll="body"
                closeButtonId="purchase-source-update-draft-close"
                showCloseButton={!pending && !uncertain}
            >
                <DialogHeader>
                    <DialogTitle>修改采购原单</DialogTitle>
                    <DialogDescription>
                        {step === "edit"
                            ? "按来源销售的新要求调整。保存后仍需提交审批，销售变更才算处理完成。"
                            : "请核对变更影响与审批路线后再提交；审批期间不可修改。"}
                    </DialogDescription>
                    <div
                        className="mt-1 flex items-center gap-3 text-xs"
                        aria-label={
                            step === "edit"
                                ? "当前第 1 步：编辑"
                                : "当前第 2 步：影响核对"
                        }
                    >
                        <span
                            className={
                                step === "edit"
                                    ? "font-medium text-foreground"
                                    : "text-muted-foreground"
                            }
                        >
                            1 编辑明细
                        </span>
                        <span
                            aria-hidden="true"
                            className="h-px w-10 bg-border"
                        />
                        <span
                            className={
                                step === "confirm"
                                    ? "font-medium text-foreground"
                                    : "text-muted-foreground"
                            }
                        >
                            2 影响核对与提交
                        </span>
                    </div>
                </DialogHeader>

                <DialogBody className="space-y-4">
                    {guard.leaveGuardOpen ? (
                        <Alert
                            variant="warning"
                            id="procurement-orders-detail-leave-guard"
                        >
                            <AlertTitle>有未保存的修改</AlertTitle>
                            <AlertDescription className="space-y-3">
                                <p>
                                    当前编辑内容尚未保存，离开后修改将丢失。建议先保存草稿。
                                </p>
                                <div className="flex flex-wrap items-center gap-2">
                                    <Button
                                        id="procurement-orders-detail-leave-continue"
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={actions.savePending}
                                        onClick={() =>
                                            guard.setLeaveGuardOpen(false)
                                        }
                                    >
                                        继续编辑
                                    </Button>
                                    <Button
                                        id="procurement-orders-detail-leave-discard"
                                        type="button"
                                        size="sm"
                                        variant="destructive"
                                        disabled={actions.savePending}
                                        onClick={guard.discardAndLeave}
                                    >
                                        放弃修改并离开
                                    </Button>
                                    <LoadingButton
                                        id="procurement-orders-detail-leave-save"
                                        size="sm"
                                        loading={actions.savePending}
                                        type="button"
                                        disabled={actions.savePending}
                                        onClick={() =>
                                            void guard.saveAndLeave()
                                        }
                                    >
                                        {actions.savePending
                                            ? "保存中…"
                                            : "保存并离开"}
                                    </LoadingButton>
                                </div>
                            </AlertDescription>
                        </Alert>
                    ) : null}

                    {result ? <FormalActionResult {...result} /> : null}

                    {step === "edit" ? (
                        <>
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
                                    editLockPending={actions.editLockPending}
                                    editLockError={actions.editLockError}
                                    onRetryEditLock={
                                        actions.retryAcquireEditToken
                                    }
                                    savePending={actions.savePending}
                                />
                            </fieldset>
                        </>
                    ) : (
                        <div className="space-y-4">
                            <section
                                aria-label="状态变化"
                                className="flex flex-wrap items-center gap-2 rounded-lg border border-border bg-muted/40 px-4 py-3"
                            >
                                <StatusBadge
                                    label={resubmitting ? "已驳回" : "草稿"}
                                    tone={
                                        resubmitting ? "destructive" : "neutral"
                                    }
                                />
                                <ArrowRightIcon
                                    aria-label="变更为"
                                    className="size-4 text-muted-foreground"
                                />
                                <StatusBadge label="审批中" tone="info" />
                                <span className="ml-auto text-xs text-muted-foreground">
                                    供应商：{order.header.supplierSnapshot}
                                </span>
                            </section>

                            <PurchaseOrderApprovalArea
                                phase="confirm"
                                approval={order.approval}
                            />

                            <section className="rounded-lg border border-border bg-muted/30 p-4 text-body-compact">
                                <h3 className="font-medium">
                                    本次动作产生的影响
                                </h3>
                                <ul className="mt-2 space-y-1.5 text-muted-foreground">
                                    <li>
                                        全部审批节点通过后形成采购生效版本；审批期间不可修改。
                                    </li>
                                    <li>
                                        提交审批后系统同步核对来源销售变更影响，完成当前采购依据变更待办。
                                    </li>
                                    {changedLines.length > 0 ? (
                                        <li>
                                            本次共调整 {changedLines.length}{" "}
                                            行明细（
                                            {changedLines
                                                .map((line) => line.itemName)
                                                .slice(0, 3)
                                                .join("、")}
                                            {changedLines.length > 3
                                                ? ` 等 ${changedLines.length} 项`
                                                : ""}
                                            ）。
                                        </li>
                                    ) : (
                                        <li>
                                            本次按当前已保存明细提交审批（共{" "}
                                            {order.currentContent.lines.length}{" "}
                                            行）。
                                        </li>
                                    )}
                                </ul>
                            </section>

                            {rework.field}
                        </div>
                    )}
                </DialogBody>

                <DialogFooter>
                    {step === "edit" ? (
                        <>
                            <Button
                                id="purchase-source-update-draft-cancel"
                                type="button"
                                variant="outline"
                                disabled={pending || uncertain}
                                onClick={requestClose}
                            >
                                取消
                            </Button>
                            <LoadingButton
                                id={`procurement-orders-detail-edit-save-${purchaseOrderId}`}
                                loading={actions.savePending}
                                type="button"
                                variant="outline"
                                disabled={
                                    !actions.draftEditToken ||
                                    pending ||
                                    !permissions.canEdit
                                }
                                onClick={() => void actions.handleSave()}
                            >
                                {actions.savePending ? "保存中…" : "保存草稿"}
                            </LoadingButton>
                            <Button
                                id={`procurement-orders-detail-edit-submit-${purchaseOrderId}`}
                                type="button"
                                disabled={
                                    !actions.draftEditToken ||
                                    !permissions.canSubmit ||
                                    pending ||
                                    !permissions.canEdit
                                }
                                onClick={() => {
                                    guard.setLeaveGuardOpen(false)
                                    actions.setSubmitConfirmOpen(true)
                                }}
                            >
                                下一步：影响核对
                                <ArrowRightIcon
                                    data-icon="inline-end"
                                    aria-hidden="true"
                                />
                            </Button>
                        </>
                    ) : (
                        <>
                            <Button
                                id="procurement-orders-detail-submit-confirm-cancel"
                                type="button"
                                variant="outline"
                                disabled={pending}
                                onClick={() =>
                                    actions.setSubmitConfirmOpen(false)
                                }
                            >
                                <ArrowLeftIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                返回编辑
                            </Button>
                            <LoadingButton
                                id="procurement-orders-detail-submit-confirm-confirm"
                                type="button"
                                loading={actions.submitPending}
                                disabled={pending || !rework.valid}
                                onClick={() =>
                                    void actions.handleSubmit(rework.reason)
                                }
                            >
                                {!actions.submitPending ? (
                                    <CheckIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                ) : null}
                                {actions.submitPending
                                    ? "正在提交…"
                                    : "确认提交审批"}
                            </LoadingButton>
                        </>
                    )}
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
