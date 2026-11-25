"use client"

import { useEffect, useMemo, useState } from "react"
import { FileTextIcon, MapPinIcon, PrinterIcon } from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceMeta,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
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
import { Spinner } from "@/components/ui/spinner"
import { StatusBadge } from "@/components/ui/status-badge"
import { DeliveryAllocationOverview } from "@/features/regions"
import { SalesOrderPaperDocument } from "@/features/sales-orders/components/sales-order-paper-dialog"
import { useSalesOrderDetailQuery } from "@/features/sales-orders/hooks/queries"
import { NATURE_LABEL } from "@/features/sales-orders/lib/labels"
import {
    buildDeliveryAllocationSummary,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { cn } from "@/lib/utils"

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
    const [activeTab, setActiveTab] = useState<"paper" | "addresses">("paper")
    const [focusedLineId, setFocusedLineId] = useState<string | null>(null)
    const query = useSalesOrderDetailQuery(
        open && salesOrderId ? salesOrderId : "",
    )
    const order = query.data

    useEffect(() => {
        if (!open) return
        setActiveTab("paper")
        setFocusedLineId(null)
    }, [open, salesOrderId])

    const deliveryLines = useMemo<DeliveryAllocationLineInput[]>(() => {
        if (!order) return []
        return order.lineItems.map((row, index) => ({
            id: row.id,
            lineNo: index + 1,
            itemName: row.name,
            specification: row.sku,
            unit: row.unitName ?? "",
            quantity: row.quantity,
            allocations: row.deliveryAllocations,
        }))
    }, [order])

    const deliverySummary = useMemo(
        () => buildDeliveryAllocationSummary(deliveryLines),
        [deliveryLines],
    )
    const hasDeliveries = Boolean(order && deliverySummary.totalRows > 0)

    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                size="2xl"
                scroll="body"
                closeButtonId="sales-orders-paper-preview-close"
                className="bg-card"
            >
                <DialogHeader className="border-border/60 bg-card py-3 text-left print:hidden">
                    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                        <div className="min-w-0 flex-1 space-y-1">
                            <div className="flex flex-wrap items-center gap-2">
                                <DialogTitle className="min-w-0 truncate">
                                    {title
                                        ? `${title} 纸质预览`
                                        : "销售单纸质预览"}
                                </DialogTitle>
                                {order ? (
                                    <>
                                        <StatusBadge
                                            tone={order.primaryStatus.tone}
                                            label={order.primaryStatus.label}
                                        />
                                        <Badge variant="neutral">
                                            {NATURE_LABEL[order.nature]}
                                        </Badge>
                                        <Badge
                                            variant="neutral"
                                            className="num"
                                        >
                                            {order.currentRevisionNo == null
                                                ? "尚未生效"
                                                : `v${order.currentRevisionNo}`}
                                        </Badge>
                                    </>
                                ) : null}
                            </div>
                            <DialogDescription className="sr-only">
                                系统业务数据的打印件；金额与状态以系统记录为准。按
                                Esc
                                或点击遮罩关闭。版本、附件和关联单据仍在对应标签页查看。
                            </DialogDescription>
                            {order ? (
                                <DetailWorkspaceMeta
                                    items={[
                                        <span key="no" className="num">
                                            {order.documentNumber}
                                        </span>,
                                        order.customerName,
                                        order.contractNumber || "无合同",
                                        order.ownerName,
                                    ]}
                                />
                            ) : null}
                        </div>
                        {order ? (
                            <div className="shrink-0 text-left sm:text-right">
                                <div className="text-xs text-muted-foreground">
                                    成交金额（含税）
                                </div>
                                <MoneyValue
                                    display="exact"
                                    value={order.amountGross}
                                    className="text-value-compact font-semibold tracking-tight text-foreground"
                                />
                            </div>
                        ) : null}
                    </div>
                </DialogHeader>

                {hasDeliveries ? (
                    <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 bg-card px-5 print:hidden">
                        <div
                            role="group"
                            aria-label="销售单预览视图切换"
                            className="flex flex-wrap items-center gap-6"
                        >
                            <button
                                id="sales-orders-paper-preview-tab-paper"
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
                                id="sales-orders-paper-preview-tab-addresses"
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

                <DialogBody className="bg-card px-5 py-4">
                    {!open || !salesOrderId ? null : query.isPending ? (
                        <div className="flex min-h-56 items-center justify-center py-10">
                            <p className="flex items-center gap-2 text-xs text-muted-foreground">
                                <Spinner aria-label="正在读取销售单" />
                                正在读取销售单…
                            </p>
                        </div>
                    ) : query.isError ? (
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
                    ) : !order ? (
                        <div className="py-8 text-center text-body-compact text-muted-foreground">
                            未找到这张销售单，可能已删除或当前角色无权查看。
                        </div>
                    ) : activeTab === "addresses" && hasDeliveries ? (
                        <DeliveryAllocationOverview
                            idPrefix="sales-orders-paper-preview-delivery"
                            lines={deliveryLines}
                            summary={deliverySummary}
                            focusedLineId={focusedLineId}
                            onClearFocus={() => setFocusedLineId(null)}
                            onReturnToLine={() => setActiveTab("paper")}
                            density="compact"
                        />
                    ) : (
                        <SalesOrderPaperDocument
                            order={order}
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
                </DialogBody>

                <DialogFooter className="items-center border-border/60 bg-card py-2.5 sm:justify-between print:hidden">
                    {order ? (
                        <DetailWorkspaceMeta
                            items={[
                                <span key="lines" className="num">
                                    共 {order.lineItems.length} 项
                                </span>,
                                `回款：${order.collection.label}`,
                                `开票：${order.invoicing.label}`,
                            ]}
                        />
                    ) : (
                        <span />
                    )}
                    <div className="flex items-center gap-2">
                        {order ? (
                            <Button
                                id="sales-orders-paper-preview-print"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.quietButton}
                                onClick={() => window.print()}
                            >
                                <PrinterIcon data-icon="inline-start" />
                                打印
                            </Button>
                        ) : null}
                        <Button
                            id="sales-orders-paper-preview-footer-close"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() => onOpenChange(false)}
                        >
                            关闭
                        </Button>
                    </div>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    )
}
