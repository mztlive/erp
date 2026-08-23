"use client"

import * as React from "react"
import Link from "next/link"

import { Button } from "@/components/ui/button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import { useSalesOrderDetailPermissions } from "@/features/sales-orders/hooks/use-sales-order-detail-permissions"
import {
    canCreatePurchaseFromSalesOrder,
    fulfillmentWorkspaceHref,
    purchaseOrdersWorkspaceHref,
    receivableWorkspaceHref,
} from "@/features/sales-orders/lib/sales-order-detail-model"

const RELATED_LANE_COPY = {
    purchase: {
        label: "采购单",
        hint: "供应商是否已接单、能否交付",
        actionLabel: "打开采购",
    },
    fulfillment: {
        label: "交付",
        hint: "发货、直发或服务执行",
        actionLabel: "打开交付",
    },
    receipt: {
        label: "回款",
        hint: "登记回款并核销到本单",
        actionLabel: "打开往来",
    },
    invoice: {
        label: "开票",
        hint: "开票单独看，不挡结案",
        actionLabel: "打开往来",
    },
} as const

function RelatedLane({
    lane,
    count,
    status,
    href,
    actionLabel,
    enabled,
    disabledReason,
}: {
    lane: keyof typeof RELATED_LANE_COPY
    count: number
    status: string
    href: string
    actionLabel?: string
    enabled: boolean
    disabledReason?: string
}) {
    const copy = RELATED_LANE_COPY[lane]
    return (
        <li className="flex items-center justify-between gap-3 py-2.5">
            <div className="min-w-0">
                <div className="text-sm font-medium">
                    {copy.label}
                    <span className="num ml-1.5 font-normal text-muted-foreground">
                        {count} 笔
                    </span>
                </div>
                <div className="text-xs text-muted-foreground">
                    {copy.hint} · {status}
                </div>
            </div>
            {enabled ? (
                <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    render={<Link href={href} />}
                >
                    {actionLabel ?? copy.actionLabel}
                </Button>
            ) : (
                <Button
                    type="button"
                    size="sm"
                    variant="secondary"
                    disabled
                    title={disabledReason}
                >
                    {actionLabel ?? copy.actionLabel}
                </Button>
            )}
        </li>
    )
}

export function RelatedLanes({
    order,
    selfReturn,
    lanes,
}: {
    order: SalesOrderDetailView
    selfReturn: string
    lanes: Array<"purchase" | "fulfillment" | "receipt" | "invoice">
}) {
    const permissions = useSalesOrderDetailPermissions()
    const items: React.ReactNode[] = []

    if (lanes.includes("purchase")) {
        const createPurchase = canCreatePurchaseFromSalesOrder(order)
        const gate = createPurchase
            ? permissions.createPurchase(
                  true,
                  "当前不能从本单创建采购单",
              )
            : permissions.openPurchase
        items.push(
            <RelatedLane
                key="purchase"
                lane="purchase"
                count={order.related.purchaseOrders}
                status={order.fulfillment.label}
                href={purchaseOrdersWorkspaceHref(order, selfReturn)}
                actionLabel={createPurchase ? "去建单" : undefined}
                enabled={gate.enabled}
                disabledReason={gate.reason}
            />,
        )
    }
    if (lanes.includes("fulfillment")) {
        const gate = permissions.openFulfillment
        items.push(
            <RelatedLane
                key="fulfillment"
                lane="fulfillment"
                count={order.related.fulfillments}
                status={order.fulfillment.label}
                href={fulfillmentWorkspaceHref(order, selfReturn)}
                enabled={gate.enabled}
                disabledReason={gate.reason}
            />,
        )
    }
    if (lanes.includes("receipt")) {
        const gate = permissions.openReceivable
        items.push(
            <RelatedLane
                key="receipt"
                lane="receipt"
                count={order.related.receipts}
                status={order.collection.label}
                href={receivableWorkspaceHref(order, selfReturn, "receipt")}
                enabled={gate.enabled}
                disabledReason={gate.reason}
            />,
        )
    }
    if (lanes.includes("invoice")) {
        const gate = permissions.openReceivable
        items.push(
            <RelatedLane
                key="invoice"
                lane="invoice"
                count={order.related.invoices}
                status={order.invoicing.label}
                href={receivableWorkspaceHref(order, selfReturn, "invoice")}
                enabled={gate.enabled}
                disabledReason={gate.reason}
            />,
        )
    }
    return <ul className="divide-y divide-grid">{items}</ul>
}
