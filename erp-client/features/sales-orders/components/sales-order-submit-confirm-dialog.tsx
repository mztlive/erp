"use client"

import { ArrowRightIcon } from "lucide-react"

import { PaperDocumentViewport } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { StatusBadge } from "@/components/ui/status-badge"
import type { SalesOrderSubmitSnapshot } from "@/features/sales-orders/components/sales-order-submit-confirm-summary"
import { SalesOrderSubmitPaper } from "@/features/sales-orders/components/sales-order-submit-paper"

/**
 * 实物及服务销售单提交确认：纸质预览当前填写内容，确认后进入审批。
 */
export function SalesOrderSubmitConfirmDialog({
    open,
    pending,
    snapshot,
    description = "提交后进入审批；任一层驳回后将从第一节点开始下一轮。",
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    snapshot: SalesOrderSubmitSnapshot
    description?: string
    onOpenChange: (open: boolean) => void
    onConfirm: () => void
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId="sales-orders-submit-confirm-close"
                className="flex h-[90vh] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
            >
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 text-left">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="min-w-0">
                            <DialogTitle>提交销售单</DialogTitle>
                            <DialogDescription>{description}</DialogDescription>
                        </div>
                        <div className="flex shrink-0 items-center gap-2">
                            <StatusBadge tone="neutral" label="草稿" />
                            <ArrowRightIcon aria-hidden="true" />
                            <StatusBadge tone="warning" label="审批中" />
                        </div>
                    </div>
                </DialogHeader>

                <PaperDocumentViewport
                    fitKey={`${snapshot.nature}-${snapshot.lineCount}`}
                >
                    <SalesOrderSubmitPaper snapshot={snapshot} />
                </PaperDocumentViewport>

                <DialogFooter className="shrink-0 border-t border-border px-6 py-4">
                    <Button
                        id="sales-orders-submit-confirm-cancel"
                        type="button"
                        variant="outline"
                        disabled={pending}
                        onClick={() => onOpenChange(false)}
                    >
                        返回修改
                    </Button>
                    <Button
                        id="sales-orders-submit-confirm-confirm"
                        type="button"
                        disabled={pending}
                        onClick={() => {
                            void onConfirm()
                        }}
                    >
                        {pending ? "提交中…" : "确认提交"}
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
