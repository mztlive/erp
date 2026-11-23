"use client"

import { DetailWorkspaceMetricItem, MoneyValue } from "@/components/business"
import { formatBookInstant } from "@/features/sales-selection/lib/presentation"
import type { ProposalView } from "@/features/sales-selection/types"

export function ProposalSummary({ proposal }: { proposal: ProposalView }) {
    const isPackage = proposal.form === "PACKAGE"
    const count = proposal.display_lines.length
    const countLabel = isPackage ? "套餐款数" : "商品款数"
    const countDescription = `共 ${count} 款${isPackage ? "套餐" : "商品"}`

    return (
        <div className="min-w-0 sm:grid sm:grid-cols-2 sm:gap-x-6 lg:block">
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label="方案合计"
                value={<MoneyValue value={proposal.total_amount} />}
                detail={countDescription}
            />
            <dl className="mt-6 space-y-5 border-t border-border/60 pt-6 text-body-compact sm:mt-0 sm:border-t-0 sm:pt-0 lg:mt-6 lg:border-t lg:pt-6">
                <div className="flex items-baseline justify-between gap-4">
                    <dt className="text-muted-foreground">{countLabel}</dt>
                    <dd className="num font-medium">{count} 款</dd>
                </div>
                <div className="flex items-baseline justify-between gap-4">
                    <dt className="shrink-0 text-muted-foreground">提交时间</dt>
                    <dd className="num text-right">
                        {formatBookInstant(proposal.submitted_at)}
                    </dd>
                </div>
            </dl>
        </div>
    )
}
