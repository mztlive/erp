"use client"

import Link from "next/link"
import { TriangleAlertIcon } from "lucide-react"

import { BusinessStatusBadge, QuickPreviewSheet } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { SupplierOrderPreviewPanel } from "@/features/supplier-orders/components/supplier-order-preview-panel"
import type { QueryFromPreviewInput } from "@/features/supplier-orders/hooks/use-supplier-orders-query-result"
import type { SupplierOrderDetailView } from "@/features/supplier-orders/types"

export type SupplierOrdersListPreviewSheetProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    preview: SupplierOrderDetailView | undefined
    previewPending: boolean
    onClose: () => void
    queryPending: boolean
    onQueryResult: (input: QueryFromPreviewInput) => Promise<void>
}

export function SupplierOrdersListPreviewSheet({
    open,
    onOpenChange,
    preview,
    previewPending,
    onClose,
    queryPending,
    onQueryResult,
}: SupplierOrdersListPreviewSheetProps) {
    return (
        <QuickPreviewSheet
            id="supplier-orders-list-preview-sheet"
            open={open}
            onOpenChange={onOpenChange}
            size="detail"
            title={preview?.order.supplierName ?? "供应商订单预览"}
            identity={
                preview ? (
                    <span className="num">
                        {preview.order.orderNo}
                        {preview.order.externalOrderNo
                            ? ` · ${preview.order.externalOrderNo}`
                            : " · 外部单号尚未返回"}
                    </span>
                ) : null
            }
            summary={
                preview ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={preview.order.fulfillmentLabel}
                            tone={preview.order.fulfillmentTone}
                        />
                        <Badge variant="secondary">
                            取消 {preview.order.cancelLabel}
                        </Badge>
                        <Badge variant="secondary">
                            退款 {preview.order.refundLabel}
                        </Badge>
                        {preview.order.fulfillmentStatus ===
                        "RESULT_UNKNOWN" ? (
                            <Badge variant="outline" className="gap-1">
                                <TriangleAlertIcon className="size-3" />
                                须先查询
                            </Badge>
                        ) : null}
                    </div>
                ) : null
            }
            footer={
                preview ? (
                    <>
                        <Button
                            id="supplier-orders-list-preview-close"
                            type="button"
                            variant="outline"
                            onClick={onClose}
                        >
                            关闭
                        </Button>
                        <Button
                            id="supplier-orders-list-preview-open"
                            type="button"
                            variant="outline"
                            render={
                                <Link
                                    href={`/supplier-api/orders/${preview.order.id}`}
                                />
                            }
                        >
                            查看详情
                        </Button>
                        {preview.allowedActions.includes("QUERY_RESULT") &&
                        !preview.workItem ? (
                            <Button
                                id="supplier-orders-list-preview-query"
                                type="button"
                                disabled={queryPending}
                                onClick={() => {
                                    void onQueryResult({
                                        orderId: preview.order.id,
                                        lockVersion: preview.order.lockVersion,
                                        placeActionId: preview.placeActionId,
                                    })
                                }}
                            >
                                查询原结果
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
        >
            {previewPending ? (
                <div className="p-5 text-sm text-muted-foreground">
                    加载预览…
                </div>
            ) : preview ? (
                <SupplierOrderPreviewPanel order={preview} />
            ) : (
                <div className="p-5 text-sm text-muted-foreground">
                    未找到该供应商订单
                </div>
            )}
        </QuickPreviewSheet>
    )
}
