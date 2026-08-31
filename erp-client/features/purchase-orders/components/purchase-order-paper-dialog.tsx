"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { PurchaseOrderPaperDocument } from "@/features/purchase-orders/components/purchase-order-paper-document"
import { usePurchaseOrderCenterQuery } from "@/features/purchase-orders/hooks/queries"
import { XIcon } from "lucide-react"

type PurchaseOrderPaperDialogProps = {
    purchaseOrderId: string | null
    open: boolean
    onOpenChange: (open: boolean) => void
}

/**
 * 采购单纸质预览：透明浮层，点开后再拉对象中心只读详情。
 */
export function PurchaseOrderPaperDialog({
    purchaseOrderId,
    open,
    onOpenChange,
}: PurchaseOrderPaperDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                showCloseButton={false}
                className="flex max-h-[min(96vh,56rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden border-0 bg-transparent p-0 shadow-none ring-0 sm:max-w-5xl dark:ring-0"
            >
                <DialogTitle className="sr-only">采购单纸质预览</DialogTitle>
                <DialogDescription className="sr-only">
                    系统业务数据的打印件；金额与状态以系统记录为准。按 Esc
                    或点击遮罩关闭。
                </DialogDescription>

                <div className="relative min-h-0 flex-1">
                    <DialogClose
                        render={
                            <Button
                                id="procurement-orders-paper-close"
                                type="button"
                                variant="secondary"
                                size="icon-sm"
                                className="absolute top-3 right-3 z-10 rounded-full border border-border bg-card/95 shadow-md backdrop-blur-sm print:hidden"
                            />
                        }
                    >
                        <XIcon aria-hidden="true" />
                        <span className="sr-only">关闭预览</span>
                    </DialogClose>

                    <div className="max-h-[min(96vh,56rem)] overflow-y-auto overscroll-contain">
                        <PurchaseOrderPaperBody
                            purchaseOrderId={purchaseOrderId}
                            enabled={open && Boolean(purchaseOrderId)}
                        />
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}

function PurchaseOrderPaperBody({
    purchaseOrderId,
    enabled,
}: {
    purchaseOrderId: string | null
    enabled: boolean
}) {
    const query = usePurchaseOrderCenterQuery(
        enabled && purchaseOrderId ? purchaseOrderId : "",
    )

    if (!enabled || !purchaseOrderId) return null
    if (query.isPending) return <PaperLoadingState />
    if (query.isError) {
        return (
            <PaperErrorState
                error={query.error}
                onRetry={() => {
                    void query.refetch()
                }}
            />
        )
    }
    if (!query.data) return <PaperMissingState />
    return <PurchaseOrderPaperDocument order={query.data} />
}

function PaperLoadingState() {
    return (
        <div className="flex min-h-64 items-center justify-center rounded-lg bg-card px-6 py-12 shadow-lg">
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <Spinner aria-label="正在读取采购单" />
                正在读取采购单…
            </p>
        </div>
    )
}

function PaperErrorState({
    error,
    onRetry,
}: {
    error: unknown
    onRetry: () => void
}) {
    return (
        <div className="rounded-lg bg-card px-6 py-8 shadow-lg">
            <BusinessFailureState
                title="采购单读取失败"
                error={error}
                action={
                    <Button
                        id="procurement-orders-paper-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={onRetry}
                    >
                        重试
                    </Button>
                }
            />
        </div>
    )
}

function PaperMissingState() {
    return (
        <div className="rounded-lg bg-card px-6 py-8 text-center text-sm text-muted-foreground shadow-lg">
            未找到这张采购单，可能已删除或当前角色无权查看。
        </div>
    )
}
