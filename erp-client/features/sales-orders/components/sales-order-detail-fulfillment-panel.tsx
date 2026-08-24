"use client"

import * as React from "react"

import { surfaceInsetClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import { FulfillmentOperationsPage } from "@/features/fulfillment-operations/pages/fulfillment-operations-page"
import { AcceptanceWorkspace } from "@/features/sales-orders/components/acceptance-workspace"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import { cn } from "@/lib/utils"

function AcceptanceSummary({
    order,
    canAccept,
    expanded,
    onExpand,
    onCollapse,
}: {
    order: SalesOrderDetailView
    canAccept: boolean
    expanded: boolean
    onExpand: () => void
    onCollapse: () => void
}) {
    const permissions = useSalesOrderDetailPermissions()
    const acceptGate = permissions.registerAcceptance(
        canAccept,
        "当前不能验收，请先完成交付或确认业务条件。",
    )
    const latest = order.acceptance

    if (expanded) {
        return (
            <div className="space-y-3">
                <div className="flex items-center justify-between gap-2">
                    <h3 className="text-sm font-medium">客户验收</h3>
                    <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={onCollapse}
                    >
                        收起验收
                    </Button>
                </div>
                <AcceptanceWorkspace salesOrderId={order.id} />
            </div>
        )
    }

    return (
        <div className={cn(surfaceInsetClassName, "px-3 py-3")}>
            <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0 space-y-1">
                    <h3 className="text-sm font-medium">客户验收</h3>
                    <p className="text-xs text-muted-foreground">
                        {latest
                            ? `最近 ${latest.reference} · ${latest.postedAt}${latest.note ? ` · ${latest.note}` : ""}`
                            : "还没有验收记录。客户确认完成后，本单才算交付完毕。"}
                    </p>
                    <p className="text-xs text-muted-foreground">
                        交付进度：{order.fulfillment.label}
                    </p>
                </div>
                <Button
                    type="button"
                    size="sm"
                    disabled={!acceptGate.enabled}
                    title={acceptGate.reason}
                    onClick={onExpand}
                >
                    登记验收
                </Button>
            </div>
        </div>
    )
}

export function FulfillmentPanel({
    order,
    acceptanceExpanded,
    canAccept,
    onExpandAcceptance,
    onCollapseAcceptance,
    onDataChanged,
}: {
    order: SalesOrderDetailView
    acceptanceExpanded: boolean
    canAccept: boolean
    onExpandAcceptance: () => void
    onCollapseAcceptance: () => void
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
                <div className="flex flex-col gap-4">
                    <FulfillmentOperationsPage
                        embeddedSalesOrderId={order.id}
                        onSalesOrderChanged={onDataChanged}
                        onOpenAcceptance={onExpandAcceptance}
                    />
                    <AcceptanceSummary
                        order={order}
                        canAccept={canAccept}
                        expanded={acceptanceExpanded}
                        onExpand={onExpandAcceptance}
                        onCollapse={onCollapseAcceptance}
                    />
                </div>
            )}
        </div>
    )
}
