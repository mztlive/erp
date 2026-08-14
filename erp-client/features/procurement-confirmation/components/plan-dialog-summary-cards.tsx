"use client"

import { money } from "@/features/procurement-confirmation/lib/format"

type PlanSummaryCardsProps = {
    salesGross: string
    purchaseGross: number
    grossMargin: number
}

export function PlanSummaryCards({
    salesGross,
    purchaseGross,
    grossMargin,
}: PlanSummaryCardsProps) {
    return (
        <div className="grid gap-3 sm:grid-cols-3">
            <div className="rounded-lg border border-border p-3">
                <p className="text-xs text-muted-foreground">销售含税金额</p>
                <p className="num mt-1 font-semibold">
                    {money.format(Number(salesGross))}
                </p>
            </div>
            <div className="rounded-lg border border-border p-3">
                <p className="text-xs text-muted-foreground">预计采购金额</p>
                <p className="num mt-1 font-semibold">
                    {money.format(purchaseGross)}
                </p>
            </div>
            <div className="rounded-lg border border-border p-3">
                <p className="text-xs text-muted-foreground">预计毛利</p>
                <p className="num mt-1 font-semibold">
                    {money.format(grossMargin)}
                </p>
            </div>
        </div>
    )
}
