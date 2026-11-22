"use client"

import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"

import { ArrowRightIcon } from "lucide-react"

import { PaperDocumentViewport } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
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
    resubmitting = false,
    description = "提交后进入审批；任一节点驳回将结束本次审批，退回提交人处理。",
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    snapshot: SalesOrderSubmitSnapshot
    resubmitting?: boolean
    description?: string
    onOpenChange: (open: boolean) => void
    onConfirm: (resubmissionReason?: string) => void | Promise<void>
}) {
    const rework = useResubmissionReason({
        open,
        required: resubmitting,
        id: "sales-orders-submit-reason",
        disabled: pending,
    })
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && pending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId="sales-orders-submit-confirm-close"
                showCloseButton={!pending}
                className="flex h-[90vh] max-h-[90vh] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-6xl"
            >
                <DialogHeader className="shrink-0 border-b border-border px-6 py-4 pr-14 text-left">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="min-w-0">
                            <DialogTitle>提交销售单</DialogTitle>
                            <DialogDescription>{description}</DialogDescription>
                        </div>
                        <div className="flex shrink-0 items-center gap-2">
                            <StatusBadge
                                tone="neutral"
                                label={resubmitting ? "已驳回" : "草稿"}
                            />
                            <ArrowRightIcon
                                aria-hidden="true"
                                className="size-3.5 text-muted-foreground"
                            />
                            <StatusBadge tone="warning" label="审批中" />
                        </div>
                    </div>
                </DialogHeader>

                <PaperDocumentViewport
                    id="sales-orders-submit-confirm-preview"
                    role="region"
                    aria-label="销售单提交预览"
                    tabIndex={0}
                    fitMode="width"
                    fitKey={`${snapshot.nature}-${snapshot.lineCount}`}
                >
                    <SalesOrderSubmitPaper snapshot={snapshot} />
                </PaperDocumentViewport>

                {resubmitting ? (
                    <div className="shrink-0 border-t border-border px-6 py-3">
                        {rework.field}
                    </div>
                ) : null}
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
                    <LoadingButton
                        id="sales-orders-submit-confirm-confirm"
                        loading={pending}
                        type="button"
                        disabled={pending || !rework.valid}
                        onClick={() => {
                            void onConfirm(rework.reason)
                        }}
                    >
                        {pending ? "提交中…" : "确认提交"}
                    </LoadingButton>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
