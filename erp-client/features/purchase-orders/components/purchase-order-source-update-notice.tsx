"use client"

import { PurchaseSourceReturnPanel } from "./purchase-source-return-panel"
import { sourceReturnProgress } from "../lib/source-return-progress"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"

import { FormalActionConfirmDialog } from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { useMaintainPurchaseSourceMutation } from "@/features/purchase-orders/hooks/queries"
import type {
    PurchaseOrderCenterView,
    PurchaseSourceUpdateView,
} from "@/features/purchase-orders/types"
import { hasPermission } from "@/lib/permissions"
import { purchaseSourceUpdateText } from "@/lib/ui-text"

/** 最长维持原因字数，与服务端一致。 */
const MAINTAIN_NOTE_MAX_LENGTH = 500

const maintainReasonSchema = z.string().superRefine((raw, ctx) => {
    const trimmed = raw.trim()
    if (!trimmed) {
        ctx.addIssue({ code: "custom", message: "请填写维持原采购的原因" })
        return
    }
    if (Array.from(trimmed).length > MAINTAIN_NOTE_MAX_LENGTH) {
        ctx.addIssue({
            code: "custom",
            message: `原因不能超过 ${MAINTAIN_NOTE_MAX_LENGTH} 个字符`,
        })
    }
})

function firstFieldError(errors: readonly unknown[]): string | undefined {
    for (const item of errors) {
        if (typeof item === "string" && item.trim()) return item
        if (
            item &&
            typeof item === "object" &&
            "message" in item &&
            typeof (item as { message?: unknown }).message === "string" &&
            (item as { message: string }).message.trim()
        ) {
            return (item as { message: string }).message
        }
    }
    return undefined
}

/**
 * 来源销售变更对本采购的影响提示（第一期 §6.3 第 10 条）。
 * 受影响时按采购状态给出处理入口；未受影响但引用旧销售版本时只作说明。
 */
export function PurchaseOrderSourceUpdateNotice({
    order,
    showActions,
    showGuidance = true,
    canEdit,
    canVoid,
    canChange,
    continueChange = false,
    onEdit,
    onRequestVoid,
    onRequestChange,
}: {
    order: PurchaseOrderCenterView
    /** 编辑草稿时只展示原因，不重复给出处理入口。 */
    showActions: boolean
    /** 工作台仅展示变更依据，当前动作由固定底栏承载。 */
    showGuidance?: boolean
    canEdit: boolean
    canVoid: boolean
    canChange: boolean
    /** 已有可编辑的改单草稿，入口显示「继续改单」。 */
    continueChange?: boolean
    onEdit: () => void
    onRequestVoid: () => void
    onRequestChange: () => void
}) {
    const returnProgress = sourceReturnProgress(order)
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
    const draft = status === "DRAFT" || status === "REJECTED"
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
                {showGuidance ? (
                    <p>
                        {draft
                            ? "请修改原单：按新销售版本调整数量、预计交付日或收件分配后保存，再提交审批；无可采购数量的行填 0 即可移除。提交审批后才算处理完成。不再需要时可作废。"
                            : effective
                              ? returnProgress.physicalPending
                                  ? "请先核实供应商接收退回商品的结果。"
                                  : returnProgress.blocksChange
                                    ? "款票差额尚未处理，请向财务提供退货依据。"
                                    : "请发起改单，按新销售版本调整数量、预计交付日或收件分配；已登记退回的数量会从履约下限中扣除。"
                              : "请撤回审批后修改原单。"}
                        {!returnProgress.hasOpenReturns
                            ? "供应商无法调整时，可登记维持原采购。"
                            : null}
                        {update.blocksPayment
                            ? "处理完成前，本采购暂停付款申请和付款。"
                            : null}
                    </p>
                ) : update.blocksPayment ? (
                    <p>本采购暂停付款申请和付款。</p>
                ) : null}
                {showActions ? (
                    <PurchaseSourceReturnPanel order={order} />
                ) : null}
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
                        {effective &&
                        canChange &&
                        !returnProgress.blocksChange ? (
                            <Button
                                id="procurement-orders-detail-source-update-change"
                                type="button"
                                size="sm"
                                onClick={onRequestChange}
                            >
                                {continueChange ? "继续改单" : "发起改单"}
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
                        {!returnProgress.hasOpenReturns ? (
                            <MaintainSourceButton
                                purchaseOrderId={order.identity.purchaseOrderId}
                                update={update}
                            />
                        ) : null}
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
export function MaintainSourceButton({
    purchaseOrderId,
    update,
    label = purchaseSourceUpdateText.maintain,
    onMaintained,
}: {
    purchaseOrderId: string
    update: PurchaseSourceUpdateView
    label?: string
    onMaintained?: () => void
}) {
    const [open, setOpen] = React.useState(false)
    const profileQuery = useAccountProfileQuery()
    const mutation = useMaintainPurchaseSourceMutation()
    const form = useAppForm({
        defaultValues: { reason: "" },
    })
    const reason = useStore(form.store, (state) => state.values.reason)
    const trimmedReason = reason.trim()
    const reasonValid =
        trimmedReason.length > 0 &&
        Array.from(trimmedReason).length <= MAINTAIN_NOTE_MAX_LENGTH
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

    const handleConfirm = async () => {
        if (!reasonValid || update.recordVersion == null) {
            throw new Error("请填写维持原采购的原因")
        }
        await mutation.mutateAsync({
            purchaseOrderId,
            expectedVersion: update.recordVersion,
            reason: trimmedReason,
        })
        onMaintained?.()
    }

    return (
        <>
            {allowed && (
                <Button
                    id="procurement-orders-detail-source-update-maintain"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={disabledReason != null}
                    title={disabledReason}
                    onClick={() => {
                        form.reset({ reason: "" })
                        setOpen(true)
                    }}
                >
                    {label}
                </Button>
            )}
            <FormalActionConfirmDialog
                layout="compact"
                idPrefix="procurement-orders-detail-maintain"
                open={open}
                onOpenChange={(next) => {
                    if (!mutation.isPending) setOpen(next)
                }}
                title={label}
                actionLabel="登记维持原采购"
                confirmLabel="确认登记"
                cancelLabel="取消"
                fromStatus={{ label: "待核对变更", tone: "warning" }}
                toStatus={{ label: "维持原采购", tone: "neutral" }}
                description="仅在已与供应商核实无法按最新销售要求调整采购时使用。"
                effects={[
                    "登记后本次采购依据变更待办结束，原采购数量和金额不变",
                    "超出销售数量的商品仍需按实际情况另行办理入库、退货或库存调整",
                ]}
                pending={mutation.isPending}
                confirmDisabled={!reasonValid || update.recordVersion == null}
                onConfirm={handleConfirm}
                formContent={
                    <form.AppField
                        name="reason"
                        validators={{
                            onChange: maintainReasonSchema,
                            onBlur: maintainReasonSchema,
                        }}
                    >
                        {(field) => {
                            const showError =
                                field.state.meta.isTouched ||
                                field.state.meta.errors.length > 0
                            const error = showError
                                ? firstFieldError(field.state.meta.errors)
                                : undefined
                            return (
                                <ReasonTextareaField
                                    id={reasonId}
                                    name={field.name}
                                    mode="chars"
                                    max={MAINTAIN_NOTE_MAX_LENGTH}
                                    label="无法调整的原因"
                                    required
                                    value={field.state.value}
                                    onChange={field.handleChange}
                                    onBlur={field.handleBlur}
                                    placeholder="例如：供应商不接受减量，已备货"
                                    rows={3}
                                    disabled={mutation.isPending}
                                    error={error}
                                    invalid={Boolean(error)}
                                    onCtrlEnterSubmit={() => {
                                        if (
                                            !mutation.isPending &&
                                            reasonValid &&
                                            update.recordVersion != null
                                        ) {
                                            document
                                                .getElementById(
                                                    "procurement-orders-detail-maintain-confirm",
                                                )
                                                ?.click()
                                        }
                                    }}
                                />
                            )
                        }}
                    </form.AppField>
                }
            />
        </>
    )
}
