"use client"

import { ChevronDownIcon, FilePenLineIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
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
}: {
    order: SalesOrderDetailView
    canStartChange: boolean
    changeBlocker?: ActionBlocker
    changePending: boolean
    onOpenChangeConfirm: () => void
    onApprovalResult?: (result: SalesOrderDetailActionResult) => void
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
        <div className="flex flex-wrap items-center gap-2">
            <Button
                id="sales-orders-detail-start-change"
                type="button"
                size="sm"
                variant="outline"
                disabled={
                    (!startChangeGate.enabled && !canContinue) || changePending
                }
                title={canContinue ? undefined : startChangeGate.reason}
                onClick={onOpenChangeConfirm}
            >
                <FilePenLineIcon data-icon="inline-start" aria-hidden="true" />
                {changePending
                    ? "正在打开改单…"
                    : canContinue
                      ? "继续改单"
                      : "发起改单"}
            </Button>
            <Popover>
                <PopoverTrigger
                    id="sales-orders-detail-more-actions"
                    render={<Button size="sm" variant="outline" />}
                >
                    更多
                    <ChevronDownIcon
                        aria-hidden="true"
                        data-icon="inline-end"
                    />
                </PopoverTrigger>
                <PopoverContent align="end" keepMounted className="w-56 gap-3">
                    <PopoverTitle className="text-sm">销售单操作</PopoverTitle>
                    <div className="flex flex-col gap-2">
                        <SalesOrderCancelApprovalButton
                            order={order}
                            onResult={onApprovalResult}
                        />
                        <SalesOrderHandoverButton
                            order={order}
                            onResult={onApprovalResult}
                        />
                    </div>
                </PopoverContent>
            </Popover>
        </div>
    )
}
