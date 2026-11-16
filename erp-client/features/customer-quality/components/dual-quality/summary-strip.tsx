"use client"

import { MoneyValue } from "@/components/business"

export function SummaryStrip({
    scopeSummary,
    ownershipBasis,
    asOf,
    filterSummary,
    objectCount,
    orderCount,
    grossTotal,
    unpricedCount,
}: {
    scopeSummary: string
    ownershipBasis: string
    asOf: string
    filterSummary: string
    objectCount: number
    orderCount: number
    grossTotal: string
    unpricedCount: number
}) {
    return (
        <dl className="grid min-w-0 grid-cols-2 gap-2 rounded-xl border border-border p-3 text-body-compact sm:grid-cols-4">
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">范围</dt>
                <dd className="truncate font-medium">{scopeSummary}</dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">归属口径</dt>
                <dd className="truncate font-medium">{ownershipBasis}</dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    分组数 / 订单数
                </dt>
                <dd className="num font-medium">
                    {objectCount} / {orderCount}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    含税总额 / 待核价
                </dt>
                <dd className="num font-medium">
                    <MoneyValue
                        display="compact"
                        value={grossTotal}
                        taxBasis="gross"
                    />{" "}
                    / {unpricedCount}
                </dd>
            </div>
            <div className="col-span-2 min-w-0 sm:col-span-4">
                <dt className="text-xs text-muted-foreground">筛选</dt>
                <dd className="break-all">{filterSummary}</dd>
            </div>
            <div className="col-span-2 min-w-0 text-xs text-muted-foreground sm:col-span-4">
                统计截至 {asOf || "—"}
            </div>
        </dl>
    )
}
