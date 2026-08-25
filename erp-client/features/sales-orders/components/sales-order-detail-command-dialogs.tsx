"use client"

import * as React from "react"
import { FilePenLineIcon } from "lucide-react"

import { FormalActionConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCancelApprovalButton } from "@/features/sales-orders/components/sales-order-cancel-approval-button"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import type { ActionBlocker } from "@/features/sales-orders/types"

export function SalesOrderDetailSecondaryActions({
    order,
    canStartChange,
    changeBlocker,
    changePending,
    onOpenChangeConfirm,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    canStartChange: boolean
    changeBlocker?: ActionBlocker
    changePending: boolean
    onOpenChangeConfirm: () => void
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const permissions = useSalesOrderDetailPermissions()
    const startChangeGate = permissions.startChange(
        canStartChange,
        changeBlocker?.reason ??
            order.commercialReadOnlyReason ??
            "当前不能改单",
    )

    return (
        <div className="flex flex-wrap items-center gap-2">
            <SalesOrderCancelApprovalButton
                order={order}
                onResult={onApprovalResult}
            />
            <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!startChangeGate.enabled || changePending}
                title={startChangeGate.reason}
                onClick={onOpenChangeConfirm}
            >
                <FilePenLineIcon data-icon="inline-start" aria-hidden="true" />
                发起改单
            </Button>
        </div>
    )
}

export function SalesOrderDetailCommandDialogs({
    order,
    changeConfirmOpen,
    onChangeConfirmOpenChange,
    onChangeConfirm,
}: {
    order: SalesOrderDetailView
    changeConfirmOpen: boolean
    onChangeConfirmOpenChange: (open: boolean) => void
    onChangeConfirm: () => Promise<void>
}) {
    return (
        <>
            <FormalActionConfirmDialog
                open={changeConfirmOpen}
                onOpenChange={onChangeConfirmOpenChange}
                title="发起改单"
                actionLabel="创建改单"
                confirmLabel="确认创建"
                fromStatus={{
                    label:
                        order.currentRevisionNo == null
                            ? "尚无生效版本"
                            : `当前 v${order.currentRevisionNo}`,
                    tone: "success",
                }}
                toStatus={{ label: "改单草稿", tone: "warning" }}
                lockedFields={["销售单号", "订单类型", "已生效版本"]}
                effects={[
                    "生成一笔改单，不改掉当前客户正在执行的版本",
                    "已有交付、回款、开票记录都会保留",
                    "提交后按已绑定的审批流程办理，全部通过后新版本生效",
                ]}
                onConfirm={onChangeConfirm}
            />
        </>
    )
}
