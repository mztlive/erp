"use client"

import { DetailWorkspaceMetricItem, MoneyValue } from "@/components/business"
import { formatBookInstant } from "@/features/sales-selection/lib/presentation"
import {
    SUBMIT_MODE_LABEL,
    type ProposalView,
} from "@/features/sales-selection/types"

export function ProposalSummary({ proposal }: { proposal: ProposalView }) {
    const isPackage = proposal.form === "PACKAGE"
    const isRange = proposal.submit_mode === "MALL_REDEEM"
    const count = proposal.display_lines.length
    const countLabel = isPackage ? "套餐款数" : "商品款数"
    const countDescription = `共 ${count} 款${isPackage ? "套餐" : "商品"}`

    return (
        <div className="min-w-0 sm:grid sm:grid-cols-2 sm:gap-x-6 lg:block">
            <DetailWorkspaceMetricItem
                emphasis="primary"
                label={isRange ? "确认内容" : "方案合计"}
                value={
                    isRange ? (
                        "可选范围"
                    ) : (
                        <MoneyValue value={proposal.total_amount} />
                    )
                }
                detail={
                    isRange
                        ? "本次仅确认可选范围，无成交合计。"
                        : countDescription
                }
            />
            <dl className="mt-6 space-y-5 border-t border-border/60 pt-6 text-body-compact sm:mt-0 sm:border-t-0 sm:pt-0 lg:mt-6 lg:border-t lg:pt-6">
                <div className="flex items-baseline justify-between gap-4">
                    <dt className="text-muted-foreground">{countLabel}</dt>
                    <dd className="num font-medium">{count} 款</dd>
                </div>
                <div className="flex items-baseline justify-between gap-4">
                    <dt className="text-muted-foreground">采购方式</dt>
                    <dd className="font-medium">
                        {SUBMIT_MODE_LABEL[proposal.submit_mode]}
                    </dd>
                </div>
                <div className="flex items-baseline justify-between gap-4">
                    <dt className="shrink-0 text-muted-foreground">提交时间</dt>
                    <dd className="num text-right">
                        {formatBookInstant(proposal.submitted_at)}
                    </dd>
                </div>
            </dl>
            {proposal.recipient && (
                <section
                    className="mt-6 space-y-2 border-t border-border/60 pt-6 text-body-compact sm:col-span-2 lg:col-span-1"
                    aria-label="收件信息"
                >
                    <h2 className="text-section-title font-semibold tracking-tight">
                        收件信息
                    </h2>
                    <p>
                        {proposal.recipient.name} ·{" "}
                        <span className="num">{proposal.recipient.phone}</span>
                    </p>
                    <p className="break-words text-muted-foreground">
                        {[
                            proposal.recipient.province,
                            proposal.recipient.city,
                            proposal.recipient.district,
                            proposal.recipient.address,
                        ]
                            .filter(Boolean)
                            .join(" ")}
                    </p>
                </section>
            )}
        </div>
    )
}
