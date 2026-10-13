"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    AlertDialog,
    AlertDialogAction,
    AlertDialogCancel,
    AlertDialogContent,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogHeader,
    AlertDialogTitle,
} from "@/components/ui/alert-dialog"
import { Button } from "@/components/ui/button"
import { Textarea } from "@/components/ui/textarea"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useMaintainPurchaseSourceMutation } from "@/features/purchase-orders/hooks/queries"
import type {
    PurchaseOrderCenterView,
    PurchaseSourceUpdateView,
} from "@/features/purchase-orders/types"
import { getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { purchaseSourceUpdateText } from "@/lib/ui-text"

/** 最长维持原因字数，与服务端一致。 */
const MAINTAIN_NOTE_MAX_LENGTH = 500

/**
 * 来源销售变更对本采购的影响提示（第一期 §6.3 第 10 条）。
 * 受影响时按采购状态给出处理入口；未受影响但引用旧销售版本时只作说明。
 */
export function PurchaseOrderSourceUpdateNotice({
    order,
    showActions,
    canEdit,
    canVoid,
    canChange,
    onEdit,
    onRequestVoid,
    onRequestChange,
}: {
    order: PurchaseOrderCenterView
    /** 编辑草稿时只展示原因，不重复给出处理入口。 */
    showActions: boolean
    canEdit: boolean
    canVoid: boolean
    canChange: boolean
    onEdit: () => void
    onRequestVoid: () => void
    onRequestChange: () => void
}) {
    const update = order.sourceUpdate
    if (
        !update ||
        (!update.affected &&
            !update.awaitingSubmission &&
            !update.salesRevisionOutdated)
    ) {
        return null
    }
    const versionText = revisionText(update, order.header.salesOrderNo)
    if (update.awaitingSubmission) {
        return (
            <Alert
                id="procurement-orders-detail-source-update"
                variant="warning"
            >
                <AlertTitle>
                    {purchaseSourceUpdateText.awaitingSubmissionTitle}
                </AlertTitle>
                <AlertDescription>
                    {purchaseSourceUpdateText.awaitingSubmissionDescription}
                </AlertDescription>
            </Alert>
        )
    }
    if (!update.affected) {
        return (
            <Alert
                id="procurement-orders-detail-source-update"
                variant="default"
            >
                <AlertTitle>来源销售已有新版本</AlertTitle>
                <AlertDescription>
                    {versionText}本采购不受影响，无需处理。
                    {update.lastResolutionLabel
                        ? `最近处理：${update.lastResolutionLabel}${update.lastMaintainNote ? `（${update.lastMaintainNote}）` : ""}。`
                        : null}
                </AlertDescription>
            </Alert>
        )
    }
    const status = order.identity.status
    const draft = status === "DRAFT"
    const effective =
        status === "EFFECTIVE" || status === "PARTIAL" || status === "COMPLETED"

    return (
        <Alert id="procurement-orders-detail-source-update" variant="warning">
            <AlertTitle>{purchaseSourceUpdateText.title}</AlertTitle>
            <AlertDescription className="space-y-3">
                <p>{versionText}以下变化影响本采购：</p>
                <ul className="space-y-0.5">
                    {update.reasons.map((reason) => (
                        <li key={`${reason.lineNo}-${reason.description}`}>
                            第 {reason.lineNo} 行 {reason.itemName}：
                            {reason.description}
                        </li>
                    ))}
                </ul>
                <p>
                    {draft
                        ? "请修改原单：按新销售版本调整数量、预计交付日或收件分配后保存，再提交审批；无可采购数量的行填 0 即可移除。提交审批后才算处理完成。不再需要时可作废。"
                        : effective
                          ? "请发起采购变更，按新销售版本调整数量、预计交付日或收件分配；已发生的入库、发货和付款按对应纠正单处理。"
                          : "请撤回审批后修改原单。"}
                    供应商无法调整时，可登记维持原采购。
                    {update.blocksPayment
                        ? "处理完成前，本采购暂停付款申请和付款。"
                        : null}
                </p>
                {showActions ? (
                    <div className="flex flex-wrap gap-2">
                        {draft && canEdit ? (
                            <Button
                                id="procurement-orders-detail-source-update-edit"
                                type="button"
                                size="sm"
                                onClick={onEdit}
                            >
                                修改原单
                            </Button>
                        ) : null}
                        {effective && canChange ? (
                            <Button
                                id="procurement-orders-detail-source-update-change"
                                type="button"
                                size="sm"
                                onClick={onRequestChange}
                            >
                                发起采购变更
                            </Button>
                        ) : null}
                        {draft && canVoid ? (
                            <Button
                                id="procurement-orders-detail-source-update-void"
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onRequestVoid}
                            >
                                作废草稿
                            </Button>
                        ) : null}
                        <MaintainSourceButton
                            purchaseOrderId={order.identity.purchaseOrderId}
                            update={update}
                        />
                    </div>
                ) : null}
            </AlertDescription>
        </Alert>
    )
}

/** 描述来源销售的版本变化。 */
function revisionText(update: PurchaseSourceUpdateView, salesOrderNo: string) {
    if (update.currentSalesRevisionNo && update.purchaseSalesRevisionNo) {
        return `销售单 ${salesOrderNo} 已更新到第 ${update.currentSalesRevisionNo} 版，本采购按第 ${update.purchaseSalesRevisionNo} 版创建。`
    }
    return `销售单 ${salesOrderNo} 已变更。`
}

/** 登记维持原采购：填写无法调整的原因后关闭依据更新。 */
function MaintainSourceButton({
    purchaseOrderId,
    update,
}: {
    purchaseOrderId: string
    update: PurchaseSourceUpdateView
}) {
    const [open, setOpen] = React.useState(false)
    const [reason, setReason] = React.useState("")
    const [error, setError] = React.useState<string | null>(null)
    const profileQuery = useAccountProfileQuery()
    const mutation = useMaintainPurchaseSourceMutation()
    const allowed = hasPermission(
        profileQuery.data?.permissions,
        "purchase_order:maintain_source",
    )
    const disabledReason = profileQuery.isPending
        ? "正在核对权限，请稍候。"
        : !allowed
          ? "当前账号没有登记维持原采购的权限"
          : update.recordVersion == null
            ? "采购依据已更新，请刷新后重试。"
            : undefined
    const reasonId = `procurement-orders-detail-maintain-reason-${purchaseOrderId}`

    const submit = async () => {
        const trimmed = reason.trim()
        if (!trimmed) {
            setError("请填写维持原采购的原因")
            return
        }
        if (update.recordVersion == null) return
        setError(null)
        try {
            await mutation.mutateAsync({
                purchaseOrderId,
                expectedVersion: update.recordVersion,
                reason: trimmed,
            })
            setOpen(false)
        } catch (cause) {
            setError(
                getErrorPresentation(cause, "登记失败，请刷新后重试。")
                    .description,
            )
        }
    }

    return (
        <>
            <Button
                id="procurement-orders-detail-source-update-maintain"
                type="button"
                size="sm"
                variant="outline"
                disabled={disabledReason != null}
                title={disabledReason}
                onClick={() => {
                    setReason("")
                    setError(null)
                    setOpen(true)
                }}
            >
                {purchaseSourceUpdateText.maintain}
            </Button>
            <AlertDialog
                open={open}
                onOpenChange={(next) => {
                    if (!mutation.isPending) setOpen(next)
                }}
            >
                <AlertDialogContent className="sm:max-w-md">
                    <AlertDialogHeader>
                        <AlertDialogTitle>
                            {purchaseSourceUpdateText.maintain}
                        </AlertDialogTitle>
                        <AlertDialogDescription>
                            登记后本采购不再提示依据待更新，采购单内容不变。超出销售数量的部分按入库、退货或库存调整处理。
                        </AlertDialogDescription>
                    </AlertDialogHeader>
                    <div className="space-y-2">
                        <label
                            htmlFor={reasonId}
                            className="text-sm font-medium"
                        >
                            无法调整的原因
                        </label>
                        <Textarea
                            id={reasonId}
                            value={reason}
                            onChange={(event) => setReason(event.target.value)}
                            placeholder="例如：供应商不接受减量，已备货"
                            maxLength={MAINTAIN_NOTE_MAX_LENGTH}
                            rows={3}
                            disabled={mutation.isPending}
                            aria-describedby={
                                error ? `${reasonId}-error` : undefined
                            }
                        />
                        {error ? (
                            <p
                                id={`${reasonId}-error`}
                                className="text-sm text-destructive"
                                role="alert"
                            >
                                {error}
                            </p>
                        ) : null}
                    </div>
                    <AlertDialogFooter>
                        <AlertDialogCancel
                            id="procurement-orders-detail-maintain-cancel"
                            disabled={mutation.isPending}
                        >
                            取消
                        </AlertDialogCancel>
                        <AlertDialogAction
                            id="procurement-orders-detail-maintain-confirm"
                            loading={mutation.isPending}
                            onClick={(event) => {
                                event.preventDefault()
                                void submit()
                            }}
                        >
                            确认登记
                        </AlertDialogAction>
                    </AlertDialogFooter>
                </AlertDialogContent>
            </AlertDialog>
        </>
    )
}
