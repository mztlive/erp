"use client"

import * as React from "react"
import type { SalesOrderDetailView } from "@/features/sales-orders/api/sales-orders"

const RELATED_LANE_COPY = {
    purchase: {
        label: "采购单",
        hint: "供应商是否已接单、能否交付",
    },
    fulfillment: {
        label: "交付",
        hint: "发货、直发或服务执行",
    },
    receipt: {
        label: "回款",
        hint: "查看本单回款进度",
    },
    invoice: {
        label: "开票",
        hint: "开票单独看，不挡结案",
    },
} as const

function RelatedLane({
    lane,
    count,
    status,
    progressDetail,
    progressTestId,
}: {
    lane: keyof typeof RELATED_LANE_COPY
    count: number
    status: string
    progressDetail?: string
    progressTestId?: string
}) {
    const copy = RELATED_LANE_COPY[lane]
    return (
        <li className="flex items-center justify-between gap-3 py-2.5">
            <div className="min-w-0" data-testid={progressTestId}>
                <div className="text-sm font-medium">
                    {copy.label}
                    <span className="num ml-1.5 font-normal text-muted-foreground">
                        {count} 笔
                    </span>
                </div>
                <div className="text-xs text-muted-foreground">
                    {copy.hint} · {status}
                </div>
                {progressDetail ? (
                    <div className="num text-xs text-muted-foreground">
                        {progressDetail}
                    </div>
                ) : null}
            </div>
        </li>
    )
}

export function RelatedLanes({
    order,
    lanes,
}: {
    order: SalesOrderDetailView
    lanes: Array<"purchase" | "fulfillment" | "receipt" | "invoice">
}) {
    const items: React.ReactNode[] = []

    if (lanes.includes("purchase")) {
        const progress = order.related.procurementProgress
        items.push(
            <RelatedLane
                key="purchase"
                lane="purchase"
                count={order.related.purchaseOrders}
                status={progress.label}
                progressDetail={`销售总数量 ${progress.salesQuantity} · 已覆盖 ${progress.coveredQuantity} · 剩余 ${progress.remainingQuantity}`}
                progressTestId="sales-order-procurement-progress"
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
            />,
        )
    }
    return <ul className="divide-y divide-grid">{items}</ul>
}
