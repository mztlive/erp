"use client"

import { ChevronDownIcon, FilePenLineIcon } from "lucide-react"

import {
    GuardedBusinessAction,
    detailWorkspaceStyles,
} from "@/components/business"
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
import {
    SalesOrderHandoverButton,
    canShowSalesOrderHandover,
} from "@/features/sales-orders/components/sales-order-handover-button"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import { salesOrderAllowsWithdrawApproval } from "@/features/sales-orders/lib/sales-order-detail-permissions"
import type { ActionBlocker } from "@/features/sales-orders/types"
import { hasPermission } from "@/lib/permissions"

export function SalesOrderStartChangeAction({
    order,
    canStartChange,
    changeBlocker,
    changePending,
    onOpenChangeConfirm,
    showChangeAction = true,
}: {
    order: SalesOrderDetailView
    canStartChange: boolean
    changeBlocker?: ActionBlocker
    changePending: boolean
    onOpenChangeConfirm: () => void
    showChangeAction?: boolean
}) {
    const permissions = useSalesOrderDetailPermissions()
    if (!showChangeAction) return null

    const canContinue =
        order.activeChangeOrder?.statusCode === "DRAFT" &&
        order.activeChangeOrder.approval?.allowedActions.includes("SUBMIT")
    const startChangeGate = permissions.startChange(
        canStartChange,
        changeBlocker?.reason ??
            order.commercialReadOnlyReason ??
            "当前不能改单",
    )
    const disabled = (!startChangeGate.enabled && !canContinue) || changePending
    const disabledReason =
        !canContinue && !startChangeGate.enabled
            ? startChangeGate.reason
            : undefined

    return (
        <PermissionGuard permission="sales_change_order:create">
            <GuardedBusinessAction
                id="sales-orders-detail-start-change"
                type="button"
                size="sm"
                variant={canContinue ? "default" : "ghost"}
                className={
                    canContinue ? "px-3.5" : detailWorkspaceStyles.subtleButton
                }
                loading={changePending}
                disabled={disabled}
                reason={disabledReason}
                onClick={onOpenChangeConfirm}
            >
                <FilePenLineIcon data-icon="inline-start" aria-hidden="true" />
                {changePending
                    ? "正在打开改单…"
                    : canContinue
                      ? "继续改单"
                      : "发起改单"}
            </GuardedBusinessAction>
        </PermissionGuard>
    )
}

export function SalesOrderDetailSecondaryActions({
    order,
    onApprovalResult,
}: {
    order: SalesOrderDetailView
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
}) {
    const permissions = useSalesOrderDetailPermissions()
    const isTerminal =
        order.primaryStatus.code === "closed" ||
        order.primaryStatus.code === "voided"
    const showArrangement =
        Boolean(order.currentRevisionNo) &&
        order.nature !== "card_voucher" &&
        !isTerminal &&
        hasPermission(permissions.granted, "sales_order:update")

    const showWithdraw =
        salesOrderAllowsWithdrawApproval(order) &&
        hasPermission(permissions.granted, "sales_order:cancel_approval")
    const showHandover = canShowSalesOrderHandover(order, permissions.granted)
    const moreActionCount = (showWithdraw ? 1 : 0) + (showHandover ? 1 : 0)

    if (!showArrangement && moreActionCount === 0) return null

    return (
        <div className="flex flex-wrap items-center gap-1.5">
            {showArrangement ? (
                <ArrangementButton
                    scope="sales-orders"
                    objectId={order.id}
                    label="收货安排"
                    variant="ghost"
                    className={detailWorkspaceStyles.subtleButton}
                />
            ) : null}
            {moreActionCount > 0 ? (
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
            ) : null}
        </div>
    )
}
