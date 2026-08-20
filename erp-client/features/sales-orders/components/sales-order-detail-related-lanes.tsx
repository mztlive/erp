"use client"

import * as React from "react"
import Link from "next/link"

import { Button } from "@/components/ui/button"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"
import {
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
}: {
    lane: keyof typeof RELATED_LANE_COPY
    count: number
    status: string
    href: string
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
            <Button
                type="button"
                size="sm"
                variant="secondary"
                render={<Link href={href} />}
            >
                {copy.actionLabel}
            </Button>
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
    const items: React.ReactNode[] = []
    if (lanes.includes("purchase")) {
        items.push(
            <RelatedLane
                key="purchase"
                lane="purchase"
                count={order.related.purchaseOrders}
                status={order.fulfillment.label}
                href={purchaseOrdersWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    if (lanes.includes("fulfillment")) {
        items.push(
            <RelatedLane
                key="fulfillment"
                lane="fulfillment"
                count={order.related.fulfillments}
                status={order.fulfillment.label}
                href={fulfillmentWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    if (lanes.includes("receipt")) {
        items.push(
            <RelatedLane
                key="receipt"
                lane="receipt"
                count={order.related.receipts}
                status={order.collection.label}
                href={receivableWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    if (lanes.includes("invoice")) {
        items.push(
            <RelatedLane
                key="invoice"
                lane="invoice"
                count={order.related.invoices}
                status={order.invoicing.label}
                href={receivableWorkspaceHref(order, selfReturn)}
            />,
        )
    }
    return <ul className="divide-y divide-grid">{items}</ul>
}
