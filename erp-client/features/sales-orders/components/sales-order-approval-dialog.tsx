"use client"

import {
    Dialog,
    DialogBody,
    DialogContent,
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
                size="2xl"
                scroll="body"
                closeButtonId={`${idPrefix}-close`}
                finalFocus={() => document.getElementById(triggerId)}
            >
                <DialogHeader>
                    <DialogTitle>销售单审批链及进度</DialogTitle>
                </DialogHeader>
                <DialogBody>
                    {open ? (
                        <ApprovalPanel
                            order={order}
                            orientation="horizontal"
                            idPrefix={`${idPrefix}-content`}
                        />
                    ) : null}
                </DialogBody>
                <DialogFooter
                    showCloseButton
                    closeButtonId={`${idPrefix}-footer-close`}
                />
            </DialogContent>
        </Dialog>
    )
}
