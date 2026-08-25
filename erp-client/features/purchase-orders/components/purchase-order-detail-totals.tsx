"use client"

import { DocumentTotals, MoneyValue } from "@/components/business"

import type { PurchaseOrderCenterView } from "@/features/purchase-orders/types"

export function PurchaseOrderDetailTotals({
    order,
    costMasked,
    className,
}: {
    order: PurchaseOrderCenterView
    costMasked: boolean
    className?: string
}) {
    return (
        <DocumentTotals
            className={className}
            title="系统合计"
            items={[
                {
                    id: "g",
                    label: "含税",
                    value: costMasked ? (
                        "•••"
                    ) : (
                        <MoneyValue value={order.currentContent.totals.gross} />
                    ),
                },
                {
                    id: "n",
                    label: "不含税",
                    value: costMasked ? (
                        "•••"
                    ) : (
                        <MoneyValue value={order.currentContent.totals.net} />
                    ),
                },
                {
                    id: "t",
                    label: "税额",
                    value: costMasked ? (
                        "•••"
                    ) : (
                        <MoneyValue value={order.currentContent.totals.tax} />
                    ),
                },
            ]}
        />
    )
}
