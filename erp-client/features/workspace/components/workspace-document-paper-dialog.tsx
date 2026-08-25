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
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import {
    workspacePaperKind,
    type WorkspacePaperKind,
} from "@/features/workspace/lib/paper-kind"
import type { WorkspaceWorkItem } from "@/features/workspace/types"
import { XIcon } from "lucide-react"

type WorkspaceDocumentPaperDialogProps = {
    item: WorkspaceWorkItem
    open: boolean
    onOpenChange: (open: boolean) => void
}

/**
 * 工作台纸质预览：透明浮层，点开后再拉该类型只读详情，不跳目标工作面。
 */
export function WorkspaceDocumentPaperDialog({
    item,
    open,
    onOpenChange,
}: WorkspaceDocumentPaperDialogProps) {
    const kind = workspacePaperKind(item.businessObjectType)

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                showCloseButton={false}
                className="flex max-h-[min(96vh,56rem)] w-full max-w-[calc(100%-1.5rem)] flex-col gap-0 overflow-hidden border-0 bg-transparent p-0 shadow-none ring-0 sm:max-w-5xl dark:ring-0"
            >
                <DialogTitle className="sr-only">
                    {item.stableNumber
                        ? `${item.stableNumber} 纸质预览`
                        : "单据纸质预览"}
                </DialogTitle>
                <DialogDescription className="sr-only">
                    系统业务数据的打印件；金额与状态以系统记录为准。按 Esc
                    或点击遮罩关闭。版本、附件和关联单据仍在对应工作面查看。
                </DialogDescription>

                <div className="relative min-h-0 flex-1">
                    <DialogClose
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
                        {kind ? (
                            <WorkspacePaperBody
                                kind={kind}
                                objectId={item.businessObjectId}
                                enabled={open}
                            />
                        ) : null}
                    </div>
                </div>
            </DialogContent>
        </Dialog>
    )
}

function WorkspacePaperBody({
    kind,
    objectId,
    enabled,
}: {
    kind: WorkspacePaperKind
    objectId: string
    enabled: boolean
}) {
    const salesId = kind === "sales_order" && enabled ? objectId : ""
    const purchaseId = kind === "purchase_order" && enabled ? objectId : ""
    const salesQuery = useSalesOrderDetailQuery(salesId)
    const purchaseQuery = usePurchaseOrderCenterQuery(purchaseId)

    if (kind === "sales_order") {
        if (salesQuery.isPending) return <PaperLoadingState />
        if (salesQuery.isError) {
            return (
                <PaperErrorState
                    error={salesQuery.error}
                    onRetry={() => {
                        void salesQuery.refetch()
                    }}
                />
            )
        }
        if (!salesQuery.data) return <PaperMissingState />
        return <SalesOrderPaperDocument order={salesQuery.data} />
    }

    if (purchaseQuery.isPending) return <PaperLoadingState />
    if (purchaseQuery.isError) {
        return (
            <PaperErrorState
                error={purchaseQuery.error}
                onRetry={() => {
                    void purchaseQuery.refetch()
                }}
            />
        )
    }
    if (!purchaseQuery.data) return <PaperMissingState />
    return <PurchaseOrderPaperDocument order={purchaseQuery.data} />
}

function PaperLoadingState() {
    return (
        <div className="flex min-h-64 items-center justify-center rounded-lg bg-card px-6 py-12 shadow-lg">
            <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <Spinner aria-label="正在读取单据" />
                正在读取单据…
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
                title="单据读取失败"
                error={error}
                onRetry={onRetry}
            />
        </div>
    )
}

function PaperMissingState() {
    return (
        <div className="rounded-lg bg-card px-6 py-8 text-center text-sm text-muted-foreground shadow-lg">
            未找到这张单据，可能已删除或当前角色无权查看。
        </div>
    )
}
