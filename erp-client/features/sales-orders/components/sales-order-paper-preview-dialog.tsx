"use client"

import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Spinner } from "@/components/ui/spinner"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"

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
                size="2xl"
                scroll="body"
                closeButtonId="sales-orders-paper-preview-close"
                className="bg-card"
            >
                <DialogHeader className="border-border/60 bg-card print:hidden">
                    <DialogTitle className="min-w-0 truncate">
                        {title ? `${title} 纸质预览` : "销售单纸质预览"}
                    </DialogTitle>
                    <DialogDescription className="sr-only">
                        系统业务数据的打印件；金额与状态以系统记录为准。按 Esc
                        或点击遮罩关闭。版本、附件和关联单据仍在对应标签页查看。
                    </DialogDescription>
                </DialogHeader>
                <DialogBody className="bg-card px-6 py-6">
                    {open && salesOrderId ? (
                        <SalesOrderPaperPreviewBody
                            salesOrderId={salesOrderId}
                        />
                    ) : null}
                </DialogBody>
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
            <div className="flex min-h-64 items-center justify-center py-12">
                <p className="flex items-center gap-2 text-xs text-muted-foreground">
                    <Spinner aria-label="正在读取销售单" />
                    正在读取销售单…
                </p>
            </div>
        )
    }

    if (query.isError) {
        return (
            <div className="py-2">
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
            <div className="py-8 text-center text-body-compact text-muted-foreground">
                未找到这张销售单，可能已删除或当前角色无权查看。
            </div>
        )
    }

    return <SalesOrderPaperDocument order={query.data} />
}
