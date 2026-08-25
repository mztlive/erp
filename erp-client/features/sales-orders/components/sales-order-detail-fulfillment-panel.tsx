"use client"

import { surfaceInsetClassName } from "@/components/business"
import { FulfillmentOperationsPage } from "@/features/fulfillment-operations/pages/fulfillment-operations-page"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { cn } from "@/lib/utils"

export function FulfillmentPanel({
    order,
    onOpenAcceptance,
    onDataChanged,
}: {
    order: SalesOrderDetailView
    onOpenAcceptance: () => void
    onDataChanged: () => void
}) {
    const isCard = order.nature === "card_voucher"

    return (
        <div className="flex flex-col gap-4">
            {!isCard ? (
                <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                    <h3 className="text-sm font-medium">关联采购</h3>
                    <p className="mt-1 text-xs text-muted-foreground">
                        采购单 {order.related.purchaseOrders} 笔 · 履约状态：
                        {order.fulfillment.label}
                    </p>
                </div>
            ) : null}

            {isCard ? (
                <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
                    <h3 className="text-sm font-medium">卡券交付</h3>
                    <p className="mt-1 text-xs text-muted-foreground">
                        到期即算交付完成。期限{" "}
                        {order.fulfillmentDeadline || "—"} · 当前{" "}
                        {order.fulfillment.label}
                        。消费多少不影响本单是否交付完成。
                    </p>
                </div>
            ) : (
                <FulfillmentOperationsPage
                    embeddedSalesOrderId={order.id}
                    onSalesOrderChanged={onDataChanged}
                    onOpenAcceptance={onOpenAcceptance}
                />
            )}
        </div>
    )
}
