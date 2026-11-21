"use client"

import { ChevronDownIcon, FilePenLineIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import { PermissionGuard } from "@/features/auth/components/permission-guard"
import { ArrangementButton } from "@/features/delivery-arrangements/components/arrangement-button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCancelApprovalButton } from "@/features/sales-orders/components/sales-order-cancel-approval-button"
import { SalesOrderHandoverButton } from "@/features/sales-orders/components/sales-order-handover-button"
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
    showChangeAction = true,
}: {
    order: SalesOrderDetailView
    canStartChange: boolean
    changeBlocker?: ActionBlocker
    changePending: boolean
    onOpenChangeConfirm: () => void
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
    showChangeAction?: boolean
}) {
    const permissions = useSalesOrderDetailPermissions()
    const canContinue =
        order.activeChangeOrder?.statusCode === "DRAFT" &&
        order.activeChangeOrder.approval?.allowedActions.includes("SUBMIT")
    const startChangeGate = permissions.startChange(
        canStartChange,
        changeBlocker?.reason ??
            order.commercialReadOnlyReason ??
            "当前不能改单",
    )

    return (
        <div className="flex flex-wrap items-center gap-1.5">
            {order.currentRevisionNo &&
            order.nature !== "card_voucher" &&
            !["已关闭", "已作废"].includes(order.primaryStatus.label) ? (
                <PermissionGuard permission="sales_order:update">
                    <ArrangementButton
                        scope="sales-orders"
                        objectId={order.id}
                        label="收货安排"
                        variant="ghost"
                        className={detailWorkspaceStyles.subtleButton}
                    />
                </PermissionGuard>
            ) : null}
            <Popover>
                <PopoverTrigger
                    id="sales-orders-detail-more-actions"
                    render={
                        <Button
                            size="sm"
                            variant="ghost"
                            className={detailWorkspaceStyles.quietButton}
                        />
                    }
                >
                    更多
                    <ChevronDownIcon
                        aria-hidden="true"
                        data-icon="inline-end"
                    />
                </PopoverTrigger>
                <PopoverContent
                    align="end"
                    keepMounted
                    className="w-48 gap-1.5 p-2"
                >
                    <PopoverTitle className="px-2 pt-1 text-xs font-medium text-muted-foreground">
                        销售单操作
                    </PopoverTitle>
                    <div className="flex flex-col gap-0.5">
                        <SalesOrderCancelApprovalButton
                            order={order}
                            onResult={onApprovalResult}
                            triggerVariant="ghost"
                            triggerClassName="w-full justify-start font-normal"
                        />
                        <SalesOrderHandoverButton
                            order={order}
                            onResult={onApprovalResult}
                            triggerVariant="ghost"
                            triggerClassName="w-full justify-start font-normal"
                        />
                    </div>
                </PopoverContent>
            </Popover>
            {showChangeAction ? (
                <PermissionGuard permission={"sales_change_order:create"}>
                    <Button
                        id="sales-orders-detail-start-change"
                        type="button"
                        size="sm"
                        variant={canContinue ? "default" : "ghost"}
                        className={
                            canContinue
                                ? "px-3.5"
                                : detailWorkspaceStyles.subtleButton
                        }
                        disabled={
                            (!startChangeGate.enabled && !canContinue) ||
                            changePending
                        }
                        title={canContinue ? undefined : startChangeGate.reason}
                        onClick={onOpenChangeConfirm}
                    >
                        <FilePenLineIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {changePending
                            ? "正在打开改单…"
                            : canContinue
                              ? "继续改单"
                              : "发起改单"}
                    </Button>
                </PermissionGuard>
            ) : null}
        </div>
    )
}
