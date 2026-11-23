"use client"

import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"

import { ArrowRightIcon } from "lucide-react"

import { PaperDocumentViewport } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { StatusBadge } from "@/components/ui/status-badge"
import { APPROVAL_INSTANCE_STATUS_META } from "@/lib/task-status-dictionary"
import type { SalesOrderSubmitSnapshot } from "@/features/sales-orders/components/sales-order-submit-confirm-summary"
import { SalesOrderSubmitPaper } from "@/features/sales-orders/components/sales-order-submit-paper"

/**
 * 实物及服务销售单提交确认：纸质预览当前填写内容与审批路线，确认后进入审批。
 */
export function SalesOrderSubmitConfirmDialog({
    open,
    pending,
    snapshot,
    approval,
    documentType = "sales_order",
    resubmitting = false,
    description = "提交后进入审批；任一节点驳回将结束本次审批，退回提交人处理。",
    onOpenChange,
    onConfirm,
}: {
    open: boolean
    pending: boolean
    snapshot: SalesOrderSubmitSnapshot
    approval?: DocumentApprovalView
    documentType?: "sales_order" | "voucher_sales_order"
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
                size="2xl"
                scroll="body"
                className="h-[92dvh]"
                closeButtonId="sales-orders-submit-confirm-close"
                showCloseButton={!pending}
            >
                <DialogHeader className="text-left">
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div className="min-w-0">
                            <DialogTitle>提交销售单</DialogTitle>
                            <DialogDescription>{description}</DialogDescription>
                        </div>
                        <div className="flex shrink-0 items-center gap-2">
                            <StatusBadge
                                tone={resubmitting ? "destructive" : "neutral"}
                                label={resubmitting ? "已驳回" : "草稿"}
                            />
                            <ArrowRightIcon
                                aria-hidden="true"
                                className="size-3.5 text-muted-foreground"
                            />
                            <StatusBadge
                                tone={
                                    APPROVAL_INSTANCE_STATUS_META.RUNNING.tone
                                }
                                label={
                                    APPROVAL_INSTANCE_STATUS_META.RUNNING.label
                                }
                            />
                        </div>
                    </div>
                </DialogHeader>

                <DialogBody className="flex flex-col overflow-hidden p-0">
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
                </DialogBody>

                <div className="shrink-0 space-y-3 border-t border-border/80 px-overlay-padding py-3">
                    <SubmissionRouteConfirmation
                        documentType={documentType}
                        definition={approval?.definition}
                        fallbackDescription={
                            approval
                                ? undefined
                                : "提交后将按当前已发布的销售单审批流程自动绑定审批人并启动审批；任一节点驳回将退回提交人修改重提。"
                        }
                    />
                    {resubmitting ? rework.field : null}
                </div>
                <DialogFooter>
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
