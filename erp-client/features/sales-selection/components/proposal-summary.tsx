"use client"

import Link from "next/link"

import { MetricItem, MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
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
        <aside
            aria-label="方案摘要"
            className="order-first min-w-0 border-b border-border pb-6 sm:grid sm:grid-cols-2 sm:gap-x-6 xl:order-last xl:block xl:border-b-0 xl:border-l xl:pb-0 xl:pl-8"
        >
            <MetricItem
                label={isRange ? "确认内容" : "方案合计"}
                value={
                    isRange ? (
                        "可选范围"
                    ) : (
                        <MoneyValue value={proposal.total_amount} size="hero" />
                    )
                }
                detail={
                    isRange
                        ? "本次仅确认可选范围，无成交合计。"
                        : countDescription
                }
            />
            <dl className="mt-6 space-y-5 border-t border-border pt-6 text-sm sm:mt-0 sm:border-t-0 sm:pt-0 xl:mt-6 xl:border-t xl:pt-6">
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
                    className="mt-6 space-y-2 border-t border-border pt-6 text-sm"
                    aria-label="收件信息"
                >
                    <h2 className="font-medium">收件信息</h2>
                    <p>
                        {proposal.recipient.name} · {proposal.recipient.phone}
                    </p>
                    <p className="break-words text-muted-foreground">
                        {[
                            proposal.recipient.province,
                            proposal.recipient.city,
                            proposal.recipient.district,
                            proposal.recipient.address,
                        ].join(" ")}
                    </p>
                </section>
            )}
            <Button
                id="selection-proposal-open-book"
                className="mt-8 w-full sm:col-span-2"
                render={
                    <Link
                        href={`/sales/selection/${encodeURIComponent(proposal.booklet_id)}`}
                    />
                }
            >
                查看来源选品册
            </Button>
        </aside>
    )
}
