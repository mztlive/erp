"use client"

import * as React from "react"
import { ArrowRightIcon, FilePenLineIcon, LoaderCircleIcon } from "lucide-react"

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
import { StatusBadge } from "@/components/ui/status-badge"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { SalesOrderCancelApprovalButton } from "@/features/sales-orders/components/sales-order-cancel-approval-button"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import type { SalesOrderDetailActionResult } from "@/features/sales-orders/lib/sales-order-detail-model"
import type { ActionBlocker } from "@/features/sales-orders/types"
import { getErrorMessage } from "@/lib/api/errors"

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
                id="sales-orders-detail-start-change"
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

/** 发起改单只创建草稿，不改现行版本；确认层保持短说明。 */
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
    const [pending, setPending] = React.useState(false)
    const [confirmError, setConfirmError] = React.useState<string | null>(null)
    const currentRevisionLabel =
        order.currentRevisionNo == null
            ? "尚无生效版本"
            : `当前 v${order.currentRevisionNo}`

    const handleOpenChange = (open: boolean) => {
        if (pending && !open) return
        if (!open) setConfirmError(null)
        onChangeConfirmOpenChange(open)
    }

    const handleConfirm = async () => {
        setPending(true)
        setConfirmError(null)
        try {
            await onChangeConfirm()
            onChangeConfirmOpenChange(false)
        } catch (error) {
            setConfirmError(getErrorMessage(error, "改单未创建，请稍后重试。"))
        } finally {
            setPending(false)
        }
    }

    return (
        <AlertDialog open={changeConfirmOpen} onOpenChange={handleOpenChange}>
            <AlertDialogContent className="sm:max-w-md">
                <AlertDialogHeader>
                    <AlertDialogTitle>发起改单</AlertDialogTitle>
                    <AlertDialogDescription>
                        创建改单草稿，不改现行版本。交付、回款、开票都保留。
                    </AlertDialogDescription>
                    <div className="flex flex-wrap items-center gap-2">
                        <StatusBadge
                            tone={
                                order.currentRevisionNo == null
                                    ? "neutral"
                                    : "success"
                            }
                            label={currentRevisionLabel}
                        />
                        <ArrowRightIcon
                            aria-label="变更为"
                            className="size-4 text-muted-foreground"
                        />
                        <StatusBadge tone="warning" label="改单草稿" />
                    </div>
                </AlertDialogHeader>
                {confirmError ? (
                    <p className="text-sm text-destructive" role="alert">
                        {confirmError}
                    </p>
                ) : null}
                <AlertDialogFooter>
                    <AlertDialogCancel
                        id="sales-orders-detail-change-cancel"
                        disabled={pending}
                    >
                        取消
                    </AlertDialogCancel>
                    <AlertDialogAction
                        id="sales-orders-detail-change-confirm"
                        type="button"
                        disabled={pending}
                        onClick={() => void handleConfirm()}
                    >
                        {pending ? (
                            <LoaderCircleIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                                className="animate-spin"
                            />
                        ) : null}
                        {pending ? "创建中" : "确认创建"}
                    </AlertDialogAction>
                </AlertDialogFooter>
            </AlertDialogContent>
        </AlertDialog>
    )
}
