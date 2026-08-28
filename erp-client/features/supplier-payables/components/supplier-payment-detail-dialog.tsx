"use client"

import { BusinessStatusBadge } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Skeleton } from "@/components/ui/skeleton"
import { SupplierPaymentDetailBody } from "@/features/supplier-payables/components/supplier-payment-detail-body"
import { getErrorMessage } from "@/lib/api/errors"
import type { PaymentRow } from "@/features/supplier-payables/types"

/**
 * 供应商付款单详情弹窗：左侧分区导航，右侧只读字段。
 * 替代原先半屏 Sheet，避免核销、回单、收款信息叠在一列里。
 */
export function SupplierPaymentDetailDialog({
    open,
    onOpenChange,
    isPending,
    isError,
    error,
    onRetry,
    row,
    onOpenPayable,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    isPending: boolean
    isError: boolean
    error: unknown
    onRetry: () => void
    row: PaymentRow | null | undefined
    onOpenPayable?: (payableAccountId: string) => void
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent className="flex h-[min(42rem,calc(100vh-2rem))] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl">
                <DialogHeader className="shrink-0 px-6 pt-6 pr-14 pb-4">
                    <div className="flex flex-wrap items-center gap-2">
                        <DialogTitle className="text-lg">付款详情</DialogTitle>
                        {row ? (
                            <BusinessStatusBadge
                                context="preview"
                                label={row.statusLabel}
                                tone={row.statusTone}
                            />
                        ) : null}
                    </div>
                    <DialogDescription>
                        查看付款记录、收款信息、银行回单与核销明细。
                    </DialogDescription>
                </DialogHeader>
                <div className="flex min-h-0 flex-1 flex-col border-t">
                    {isPending ? (
                        <PaymentDetailSkeleton />
                    ) : row ? (
                        <SupplierPaymentDetailBody
                            key={row.paymentId}
                            row={row}
                            onOpenPayable={onOpenPayable}
                        />
                    ) : isError ? (
                        <div className="flex flex-col items-start gap-3 p-6">
                            <p className="text-sm text-muted-foreground">
                                {getErrorMessage(
                                    error,
                                    "付款详情加载失败，请重试。",
                                )}
                            </p>
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onRetry}
                            >
                                重试
                            </Button>
                        </div>
                    ) : (
                        <p className="p-6 text-sm text-muted-foreground">
                            未找到付款详情
                        </p>
                    )}
                </div>
                <DialogFooter className="shrink-0 border-t px-6 py-4">
                    <Button
                        type="button"
                        variant="outline"
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function PaymentDetailSkeleton() {
    return (
        <div className="flex min-h-0 flex-1">
            <div className="flex w-44 shrink-0 flex-col gap-1 border-r p-3 sm:w-52">
                {Array.from({ length: 6 }, (_, index) => (
                    <Skeleton key={index} className="h-10 rounded-lg" />
                ))}
            </div>
            <div className="flex min-w-0 flex-1 flex-col gap-5 p-6">
                <Skeleton className="h-6 w-24" />
                <div className="grid grid-cols-1 gap-5 sm:grid-cols-2">
                    {Array.from({ length: 6 }, (_, index) => (
                        <Skeleton key={index} className="h-16" />
                    ))}
                </div>
            </div>
        </div>
    )
}
