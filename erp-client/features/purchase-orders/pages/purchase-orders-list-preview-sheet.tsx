"use client"

import Link from "next/link"

import {
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { PurchaseOrderPreviewPanel } from "@/features/purchase-orders/components/purchase-order-preview-panel"
import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"
import { PURCHASE_TYPE_LABEL } from "@/features/purchase-orders/types"

export type PurchaseOrdersListPreviewSheetProps = {
    previewId: string | null
    onOpenChange: (open: boolean) => void
    onClosePreview: (purchaseOrderId: string) => void
    order: PurchaseOrderCenterView | null | undefined
    pending: boolean
    listReturnHref: string
}

export function PurchaseOrdersListPreviewSheet({
    previewId,
    onOpenChange,
    onClosePreview,
    order,
    pending,
    listReturnHref,
}: PurchaseOrdersListPreviewSheetProps) {
    return (
        <QuickPreviewSheet
            open={previewId != null}
            onOpenChange={onOpenChange}
            size="detail"
            title={order?.header.supplierSnapshot ?? "采购单预览"}
            identity={
                order ? (
                    <span className="num">
                        {order.identity.purchaseNo ?? order.identity.draftLabel}{" "}
                        ·{" "}
                        {order.identity.revisionNo
                            ? `v${order.identity.revisionNo}`
                            : "草稿"}{" "}
                        · {order.header.salesOrderNo}
                    </span>
                ) : null
            }
            summary={
                order ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={order.identity.statusLabel}
                            tone={order.identity.statusTone}
                        />
                        <Badge variant="secondary">
                            {PURCHASE_TYPE_LABEL[order.header.purchaseType]}
                        </Badge>
                    </div>
                ) : null
            }
            footer={
                order ? (
                    <>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => {
                                onClosePreview(order.identity.purchaseOrderId)
                            }}
                        >
                            关闭
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            render={
                                <Link
                                    href={`/procurement/orders/${order.identity.purchaseOrderId}`}
                                />
                            }
                        >
                            查看详情
                        </Button>
                        {order.allowedActions.includes("EDIT") ? (
                            <Button
                                type="button"
                                render={
                                    <Link
                                        href={`/procurement/orders/${order.identity.purchaseOrderId}?mode=edit`}
                                    />
                                }
                            >
                                去编辑
                            </Button>
                        ) : null}
                        {order.allowedActions.includes("REVIEW") ? (
                            <Button
                                type="button"
                                render={
                                    <Link
                                        href={`/procurement/orders/${order.identity.purchaseOrderId}?mode=review`}
                                    />
                                }
                            >
                                去审核
                            </Button>
                        ) : null}
                        {order.allowedActions.includes("FULFILL") ? (
                            <Button
                                type="button"
                                variant="outline"
                                render={
                                    <Link
                                        href={`/fulfillment?lane=procurement&scope=mine&purchaseOrderId=${order.identity.purchaseOrderId}&from=W08&returnTo=${encodeURIComponent(listReturnHref)}`}
                                    />
                                }
                            >
                                去交付
                            </Button>
                        ) : order.actionBlockers.some(
                              (b) => b.action === "FULFILL",
                          ) ? (
                            <Button type="button" variant="outline" disabled>
                                履约已阻断
                            </Button>
                        ) : null}
                    </>
                ) : null
            }
        >
            {pending ? (
                <div className="p-5 text-sm text-muted-foreground">
                    加载预览…
                </div>
            ) : order ? (
                <PurchaseOrderPreviewPanel order={order} />
            ) : (
                <div className="p-5 text-sm text-muted-foreground">
                    无法加载预览
                </div>
            )}
        </QuickPreviewSheet>
    )
}
