"use client"

import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import type { SalesOrderDetailView } from "../api/contracts"
import { ApprovalPanel } from "./sales-order-detail-approval-panel"

/** 从流程图原地查看本单审批链及进度，关闭后返回原节点。 */
export function SalesOrderApprovalDialog({
    order,
    open,
    onOpenChange,
    idPrefix,
    triggerId,
}: {
    order: SalesOrderDetailView
    open: boolean
    onOpenChange: (open: boolean) => void
    idPrefix: string
    triggerId: string
}) {
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                id={`${idPrefix}-dialog`}
                closeButtonId={`${idPrefix}-close`}
                finalFocus={() => document.getElementById(triggerId)}
                className="flex max-h-[85dvh] flex-col overflow-hidden sm:max-w-5xl"
            >
                <DialogHeader className="shrink-0 pr-8">
                    <DialogTitle>销售单审批链及进度</DialogTitle>
                    <DialogDescription>
                        查看各节点审批人、当前进度与处理意见。
                    </DialogDescription>
                </DialogHeader>
                <div className="min-h-0 flex-1 overflow-y-auto pr-1">
                    {open ? (
                        <ApprovalPanel
                            order={order}
                            orientation="horizontal"
                            idPrefix={`${idPrefix}-content`}
                        />
                    ) : null}
                </div>
                <DialogFooter
                    className="shrink-0"
                    showCloseButton
                    closeButtonId={`${idPrefix}-footer-close`}
                />
            </DialogContent>
        </Dialog>
    )
}
