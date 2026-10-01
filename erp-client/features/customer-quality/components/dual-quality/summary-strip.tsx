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
    policyVersion,
    organizationVersion,
    scopeVersion,
}: {
    scopeSummary: string
    ownershipBasis: string
    asOf: string
    filterSummary: string
    objectCount: number
    orderCount: number
    grossTotal: string
    unpricedCount: number
    policyVersion: number
    organizationVersion: number
    scopeVersion: string
}) {
    return (
        <dl className="grid min-w-0 grid-cols-2 gap-2 rounded-xl border border-border p-3 text-[13px] sm:grid-cols-4">
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
                    对象数 / 订单数
                </dt>
                <dd className="num font-medium">
                    {objectCount} / {orderCount}
                </dd>
            </div>
            <div className="min-w-0">
                <dt className="text-xs text-muted-foreground">
                    含税总额 / 缺版本
                </dt>
                <dd className="num font-medium">
                    <MoneyValue value={grossTotal} taxBasis="gross" /> /{" "}
                    {unpricedCount}
                </dd>
            </div>
            <div className="col-span-2 min-w-0 sm:col-span-4">
                <dt className="text-xs text-muted-foreground">筛选</dt>
                <dd className="break-all">{filterSummary}</dd>
            </div>
            <div className="col-span-2 min-w-0 text-xs text-muted-foreground sm:col-span-4">
                授权时点 {asOf || "—"} · 权限版本 {policyVersion} · 组织版本{" "}
                {organizationVersion} · 范围版本{" "}
                <span className="num break-all">{scopeVersion || "—"}</span>
            </div>
        </dl>
    )
}
