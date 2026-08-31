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
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { XIcon } from "lucide-react"

export type SalesOrderPaperPreviewDialogProps = Readonly<{
    salesOrderId: string | null
    title?: string
    open: boolean
    onOpenChange: (open: boolean) => void
}>

/**
 * 按销售单身份拉取详情并展示纸质预览，不使用供给分配投影。
 */
export function SalesOrderPaperPreviewDialog({
    salesOrderId,
    title,
    open,
    onOpenChange,
}: SalesOrderPaperPreviewDialogProps) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                showCloseButton={false}
                className="flex max-h-[min(96vh,56rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden border-0 bg-transparent p-0 shadow-none ring-0 sm:max-w-5xl dark:ring-0"
            >
                <DialogTitle className="sr-only">
                    {title ? `${title} 纸质预览` : "销售单纸质预览"}
                </DialogTitle>
                <DialogDescription className="sr-only">
                    系统业务数据的打印件；金额与状态以系统记录为准。按 Esc
                    或点击遮罩关闭。版本、附件和关联单据仍在对应工作面查看。
                </DialogDescription>
                <div className="relative min-h-0 flex-1">
                    <DialogClose
                        id="sales-orders-paper-preview-close"
                        render={
                            <Button
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
                        {open && salesOrderId ? (
                            <SalesOrderPaperPreviewBody
                                salesOrderId={salesOrderId}
                            />
                        ) : null}
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}

/** 按销售单身份读取详情并填入纸质件。 */
function SalesOrderPaperPreviewBody({
    salesOrderId,
}: {
    salesOrderId: string
}) {
    const query = useSalesOrderDetailQuery(salesOrderId)

    if (query.isPending) {
        return (
            <div className="flex min-h-64 items-center justify-center rounded-lg bg-card px-6 py-12 shadow-lg">
                <p className="flex items-center gap-2 text-sm text-muted-foreground">
                    <Spinner aria-label="正在读取销售单" />
                    正在读取销售单…
                </p>
            </div>
        )
    }

    if (query.isError) {
        return (
            <div className="rounded-lg bg-card px-6 py-8 shadow-lg">
                <BusinessFailureState
                    title="销售单读取失败"
                    error={query.error}
                    action={
                        <Button
                            id="sales-orders-paper-preview-retry"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => void query.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </div>
        )
    }

    if (!query.data) {
        return (
            <div className="rounded-lg bg-card px-6 py-8 text-center text-sm text-muted-foreground shadow-lg">
                未找到这张销售单，可能已删除或当前角色无权查看。
            </div>
        )
    }

    return <SalesOrderPaperDocument order={query.data} />
}
