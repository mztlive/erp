"use client"

import { ArrowRightIcon, TriangleAlertIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import { hasPermission } from "@/lib/permissions"
import {
    SalesOrderDeliveryOverview,
    type DeliveryProgressQuery,
} from "./sales-order-delivery-overview"

/** 销售单交付分区：聚焦商品明细、交付与验收进度；运费待补引导至「附加费用」统一处理。 */
export function PurchasePanel({
    order,
    deliveryQuery,
    canReadDelivery,
    canAccept,
    onStartAcceptance,
    onOpenExtraCost,
}: {
    order: SalesOrderDetailView
    deliveryQuery: DeliveryProgressQuery
    canReadDelivery: boolean
    canAccept?: boolean
    onStartAcceptance?: () => void
    onOpenExtraCost?: () => void
}) {
    const permissions = useSalesOrderDetailPermissions()
    const freightFollowUps = order.related.freightFollowUps ?? []
    const canAddExpense =
        order.nature === "physical_service" &&
        order.currentRevisionNo != null &&
        hasPermission(permissions.granted, "purchase_order:create")

    return (
        <SalesOrderDeliveryOverview
            order={order}
            query={deliveryQuery}
            canRead={canReadDelivery && order.currentRevisionNo != null}
            canAccept={canAccept}
            onStartAcceptance={onStartAcceptance}
            notice={
                canAddExpense && freightFollowUps.length > 0 ? (
                    <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2 rounded-lg border border-warning-border bg-warning-soft/75 px-3.5 py-2.5 text-body-compact text-warning-soft-foreground">
                        <div className="flex min-w-0 items-center gap-2">
                            <TriangleAlertIcon
                                aria-hidden="true"
                                className="size-4 shrink-0"
                            />
                            <span>
                                有{" "}
                                <span className="num font-semibold">
                                    {freightFollowUps.length}
                                </span>{" "}
                                项集采商品运费另计尚未处理，请在「附加费用」中追加运费或于采购单标记免运费。
                            </span>
                        </div>
                        {onOpenExtraCost ? (
                            <Button
                                id="sales-order-delivery-go-extra-cost"
                                type="button"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={onOpenExtraCost}
                            >
                                前往附加费用
                                <ArrowRightIcon
                                    aria-hidden="true"
                                    data-icon="inline-end"
                                />
                            </Button>
                        ) : null}
                    </div>
                ) : null
            }
        />
    )
}
