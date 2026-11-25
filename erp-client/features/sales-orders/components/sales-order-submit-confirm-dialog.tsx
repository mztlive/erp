"use client"

import { useEffect, useMemo, useState } from "react"
import { ArrowRightIcon, FileTextIcon, MapPinIcon } from "lucide-react"

import { DetailWorkspaceMeta, MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { StatusBadge } from "@/components/ui/status-badge"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import type { DocumentApprovalView } from "@/features/approval-workflow/types"
import { DeliveryAllocationOverview } from "@/features/regions"
import type { SalesOrderSubmitSnapshot } from "@/features/sales-orders/components/sales-order-submit-confirm-summary"
import { SalesOrderSubmitPaper } from "@/features/sales-orders/components/sales-order-submit-paper"
import { NATURE_LABEL } from "@/features/sales-orders/lib/labels"
import {
    buildDeliveryAllocationSummary,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { APPROVAL_INSTANCE_STATUS_META } from "@/lib/task-status-dictionary"
import { cn } from "@/lib/utils"

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
    const [activeTab, setActiveTab] = useState<"paper" | "addresses">("paper")
    const [focusedLineId, setFocusedLineId] = useState<string | null>(null)
    const rework = useResubmissionReason({
        open,
        required: resubmitting,
        id: "sales-orders-submit-reason",
        disabled: pending,
    })

    useEffect(() => {
        if (!open) return
        setActiveTab("paper")
        setFocusedLineId(null)
    }, [open])

    const deliveryLines = useMemo<DeliveryAllocationLineInput[]>(
        () =>
            snapshot.lineItems.map((row, index) => ({
                id: row.rowKey,
                lineNo: index + 1,
                itemName: row.name || "—",
                unit: row.unit,
                quantity: row.quantity,
                allocations: row.deliveryAllocations,
            })),
        [snapshot.lineItems],
    )
    const deliverySummary = useMemo(
        () => buildDeliveryAllocationSummary(deliveryLines),
        [deliveryLines],
    )
    const hasDeliveries = deliverySummary.totalRows > 0

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
                className="bg-card"
                closeButtonId="sales-orders-submit-confirm-close"
                showCloseButton={!pending}
            >
                <DialogHeader className="border-border/60 bg-card py-3 text-left">
                    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                        <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex flex-wrap items-center gap-2">
                                <DialogTitle>提交销售单</DialogTitle>
                                <div className="inline-flex shrink-0 items-center gap-1.5">
                                    <StatusBadge
                                        tone={
                                            resubmitting
                                                ? "destructive"
                                                : "neutral"
                                        }
                                        label={resubmitting ? "已驳回" : "草稿"}
                                    />
                                    <ArrowRightIcon
                                        aria-hidden="true"
                                        className="size-3 text-muted-foreground"
                                    />
                                    <StatusBadge
                                        tone={
                                            APPROVAL_INSTANCE_STATUS_META
                                                .RUNNING.tone
                                        }
                                        label={
                                            APPROVAL_INSTANCE_STATUS_META
                                                .RUNNING.label
                                        }
                                    />
                                </div>
                                <Badge variant="neutral">
                                    {NATURE_LABEL[snapshot.nature]}
                                </Badge>
                                <Badge variant="neutral" className="num">
                                    共 {snapshot.lineCount} 项
                                </Badge>
                            </div>
                            <DialogDescription>{description}</DialogDescription>
                            <DetailWorkspaceMeta
                                className="pt-0.5"
                                items={[
                                    snapshot.customerName || null,
                                    snapshot.contractLabel || "未关联归档合同",
                                    snapshot.ownerName || null,
                                ]}
                            />
                        </div>
                        <div className="shrink-0 text-left sm:text-right">
                            <div className="text-xs text-muted-foreground">
                                成交金额（含税）
                            </div>
                            <MoneyValue
                                display="exact"
                                value={snapshot.amountGross}
                                className="text-value-compact font-semibold tracking-tight text-foreground"
                            />
                        </div>
                    </div>
                </DialogHeader>

                {hasDeliveries ? (
                    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-card px-5">
                        <div
                            role="group"
                            aria-label="销售单提交预览视图切换"
                            className="flex flex-wrap items-center gap-6"
                        >
                            <button
                                id="sales-orders-submit-confirm-tab-paper"
                                type="button"
                                aria-pressed={activeTab === "paper"}
                                onClick={() => setActiveTab("paper")}
                                className={cn(
                                    "relative -mb-px inline-flex h-10 cursor-pointer items-center gap-1.5 border-0 bg-transparent px-0 text-body-compact whitespace-nowrap transition-colors outline-none focus-visible:rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-3 focus-visible:outline-primary",
                                    "after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary after:transition-opacity",
                                    activeTab === "paper"
                                        ? "font-medium text-primary after:opacity-100"
                                        : "font-normal text-muted-foreground after:opacity-0 hover:text-foreground",
                                )}
                            >
                                <FileTextIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0"
                                />
                                销售单纸面
                            </button>
                            <button
                                id="sales-orders-submit-confirm-tab-addresses"
                                type="button"
                                aria-pressed={activeTab === "addresses"}
                                onClick={() => setActiveTab("addresses")}
                                className={cn(
                                    "relative -mb-px inline-flex h-10 cursor-pointer items-center gap-1.5 border-0 bg-transparent px-0 text-body-compact whitespace-nowrap transition-colors outline-none focus-visible:rounded-sm focus-visible:outline-2 focus-visible:-outline-offset-3 focus-visible:outline-primary",
                                    "after:pointer-events-none after:absolute after:inset-x-0 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary after:transition-opacity",
                                    activeTab === "addresses"
                                        ? "font-medium text-primary after:opacity-100"
                                        : "font-normal text-muted-foreground after:opacity-0 hover:text-foreground",
                                )}
                            >
                                <MapPinIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0"
                                />
                                <span>
                                    收货地址明细（
                                    <span className="num">
                                        {deliverySummary.uniqueAddressCount}
                                    </span>{" "}
                                    处）
                                </span>
                            </button>
                        </div>
                    </div>
                ) : null}

                <DialogBody className="flex flex-col overflow-hidden bg-card p-0">
                    <div
                        id="sales-orders-submit-confirm-preview"
                        role="region"
                        aria-label="销售单提交预览"
                        className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-card px-5 py-4"
                    >
                        {activeTab === "addresses" && hasDeliveries ? (
                            <DeliveryAllocationOverview
                                idPrefix="sales-orders-submit-confirm-delivery"
                                lines={deliveryLines}
                                summary={deliverySummary}
                                focusedLineId={focusedLineId}
                                onClearFocus={() => setFocusedLineId(null)}
                                onReturnToLine={() => setActiveTab("paper")}
                                density="compact"
                            />
                        ) : (
                            <SalesOrderSubmitPaper
                                snapshot={snapshot}
                                compact
                                onInspectAddresses={
                                    hasDeliveries
                                        ? (lineId) => {
                                              setFocusedLineId(lineId ?? null)
                                              setActiveTab("addresses")
                                          }
                                        : undefined
                                }
                            />
                        )}
                    </div>
                </DialogBody>

                <div className="shrink-0 space-y-2.5 border-t border-border/60 bg-surface-sunken/35 px-5 py-2.5">
                    <SubmissionRouteConfirmation
                        compact
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

                <DialogFooter className="items-center border-border/60 bg-card py-2.5 sm:justify-between">
                    <div className="hidden flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground sm:flex">
                        <span>
                            不含税{" "}
                            <MoneyValue
                                display="exact"
                                value={snapshot.amountNet}
                                className="font-medium text-foreground"
                            />
                        </span>
                        <span
                            aria-hidden="true"
                            className="text-muted-foreground/40"
                        >
                            ·
                        </span>
                        <span>
                            税额{" "}
                            <MoneyValue
                                display="exact"
                                value={snapshot.amountTax}
                                className="font-medium text-foreground"
                            />
                        </span>
                    </div>
                    <div className="flex flex-wrap items-center justify-end gap-2">
                        <Button
                            id="sales-orders-submit-confirm-cancel"
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={pending}
                            onClick={() => onOpenChange(false)}
                        >
                            返回修改
                        </Button>
                        <LoadingButton
                            id="sales-orders-submit-confirm-confirm"
                            loading={pending}
                            type="button"
                            size="sm"
                            disabled={pending || !rework.valid}
                            onClick={() => {
                                void onConfirm(rework.reason)
                            }}
                        >
                            {pending ? "提交中…" : "确认提交"}
                        </LoadingButton>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
